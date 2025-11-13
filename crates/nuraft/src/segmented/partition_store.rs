use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bop_aof::{Aof, AofConfig, AofManager};
use crc32fast::Hasher;

use crate::error::{RaftError, RaftResult};
use crate::segmented::types::{
    GlobalLogIndex, PartitionAofMetadata, PartitionId, PartitionLogIndex,
};

const PARTITION_RECORD_VERSION: u8 = 1;
const PARTITION_RECORD_HEADER_LEN: usize = 1 + 1 + 8 + 8 + 4 + 4;
const PARTITION_RECORD_FLAG_CHECKSUM: u8 = 0b0000_0001;
const METADATA_LEN: usize = 8 * 4; // partition id + highest durable + next index + last raft index

struct PartitionEntry {
    aof: Aof,
    metadata: PartitionAofMetadata,
    metadata_path: PathBuf,
}

pub struct PartitionLogStore {
    manager: Arc<AofManager>,
    base_config: AofConfig,
    append_timeout: Duration,
    verify_checksums: bool,
    partitions_root: PathBuf,
    partitions: HashMap<PartitionId, PartitionEntry>,
}

impl PartitionLogStore {
    pub fn new(
        manager: Arc<AofManager>,
        base_config: AofConfig,
        partitions_root: PathBuf,
        append_timeout: Duration,
        verify_checksums: bool,
    ) -> RaftResult<Self> {
        fs::create_dir_all(&partitions_root)
            .map_err(|err| map_fs_err("create partition root", &partitions_root, err))?;
        Ok(Self {
            manager,
            base_config,
            append_timeout,
            verify_checksums,
            partitions_root,
            partitions: HashMap::new(),
        })
    }

    pub fn record_durable(
        &mut self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        raft_index: GlobalLogIndex,
        payload: &[u8],
    ) -> RaftResult<()> {
        let verify_checksums = self.verify_checksums;
        let append_timeout = self.append_timeout;
        let record_bytes =
            encode_partition_record(partition_index, raft_index, payload, verify_checksums);
        let entry = self.ensure_partition(partition_id)?;
        let record_id = entry
            .aof
            .append_record_with_timeout(&record_bytes, append_timeout)
            .map_err(map_aof_err)?;
        entry.aof.flush_until(record_id).map_err(map_aof_err)?;
        entry.metadata.mark_durable(partition_index, raft_index);
        persist_metadata(&entry.metadata_path, entry.metadata)?;
        Ok(())
    }

    pub fn metadata(&self, partition_id: PartitionId) -> Option<PartitionAofMetadata> {
        self.partitions
            .get(&partition_id)
            .map(|entry| entry.metadata)
    }

    fn ensure_partition(&mut self, partition_id: PartitionId) -> RaftResult<&mut PartitionEntry> {
        if !self.partitions.contains_key(&partition_id) {
            let partition_dir = self.partitions_root.join(partition_id.to_string());
            fs::create_dir_all(&partition_dir)
                .map_err(|err| map_fs_err("create partition dir", &partition_dir, err))?;

            let mut config = self.base_config.clone();
            config.root_dir = partition_dir.clone();
            let handle = self.manager.handle();
            let aof = Aof::new(handle, config).map_err(map_aof_err)?;

            let metadata_path = partition_dir.join("metadata.bin");
            let metadata = load_metadata(&metadata_path, partition_id)?;
            self.partitions.insert(
                partition_id,
                PartitionEntry {
                    aof,
                    metadata,
                    metadata_path,
                },
            );
        }
        Ok(self
            .partitions
            .get_mut(&partition_id)
            .expect("partition entry exists"))
    }
}

fn encode_partition_record(
    partition_index: PartitionLogIndex,
    raft_index: GlobalLogIndex,
    payload: &[u8],
    verify_checksums: bool,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(PARTITION_RECORD_HEADER_LEN + payload.len());
    let checksum = if verify_checksums {
        compute_checksum(payload)
    } else {
        None
    };
    let mut flags = 0u8;
    if checksum.is_some() {
        flags |= PARTITION_RECORD_FLAG_CHECKSUM;
    }

    buf.push(PARTITION_RECORD_VERSION);
    buf.push(flags);
    buf.extend_from_slice(&partition_index.to_le_bytes());
    buf.extend_from_slice(&raft_index.to_le_bytes());
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(&checksum.unwrap_or_default().to_le_bytes());
    buf.extend_from_slice(payload);
    buf
}

fn load_metadata(path: &Path, partition_id: PartitionId) -> RaftResult<PartitionAofMetadata> {
    match fs::read(path) {
        Ok(bytes) => decode_metadata(&bytes, partition_id, path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Ok(PartitionAofMetadata::new(partition_id, 0, 1, 0))
        }
        Err(err) => Err(map_fs_err("read metadata", path, err)),
    }
}

fn decode_metadata(
    bytes: &[u8],
    partition_id: PartitionId,
    path: &Path,
) -> RaftResult<PartitionAofMetadata> {
    if bytes.len() < METADATA_LEN {
        return Err(RaftError::LogStoreError(format!(
            "metadata file {} truncated",
            path.display()
        )));
    }
    let stored_id = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    if stored_id != partition_id {
        return Err(RaftError::LogStoreError(format!(
            "metadata file {} partition id mismatch (found {stored_id})",
            path.display()
        )));
    }
    let highest = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let next = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let last_raft = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
    Ok(PartitionAofMetadata::new(
        partition_id,
        highest,
        next,
        last_raft,
    ))
}

fn persist_metadata(path: &Path, metadata: PartitionAofMetadata) -> RaftResult<()> {
    let mut buf = Vec::with_capacity(METADATA_LEN);
    buf.extend_from_slice(&metadata.partition_id.to_le_bytes());
    buf.extend_from_slice(&metadata.highest_durable_index.to_le_bytes());
    buf.extend_from_slice(&metadata.next_partition_index.to_le_bytes());
    buf.extend_from_slice(&metadata.last_applied_raft_index.to_le_bytes());

    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &buf).map_err(|err| map_fs_err("write metadata tmp", path, err))?;
    if path.exists() {
        fs::remove_file(path).map_err(|err| map_fs_err("remove existing metadata", path, err))?;
    }
    fs::rename(&tmp, path).map_err(|err| map_fs_err("commit metadata", path, err))?;
    Ok(())
}

fn compute_checksum(payload: &[u8]) -> Option<u32> {
    if payload.is_empty() {
        None
    } else {
        let mut hasher = Hasher::new();
        hasher.update(payload);
        Some(hasher.finalize())
    }
}

fn map_fs_err(action: &str, path: &Path, err: std::io::Error) -> RaftError {
    RaftError::LogStoreError(format!("{action} for {} failed: {err}", path.display()))
}

fn map_aof_err(err: bop_aof::error::AofError) -> RaftError {
    RaftError::LogStoreError(format!("aof error: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bop_aof::store::{Tier0CacheConfig, Tier1Config};
    use bop_aof::{AofConfig, AofManager, AofManagerConfig};
    use tempfile::TempDir;

    fn manager() -> Arc<AofManager> {
        let mut config = AofManagerConfig::default().without_tier2();
        config.runtime_worker_threads = Some(2);
        config.store.tier0 = Tier0CacheConfig::new(32 * 1024 * 1024, 32 * 1024 * 1024);
        config.store.tier1 = Tier1Config::new(64 * 1024 * 1024).with_worker_threads(1);
        Arc::new(AofManager::with_config(config).expect("manager"))
    }

    fn base_config(root: &Path) -> AofConfig {
        let mut cfg = AofConfig::default();
        cfg.root_dir = root.to_path_buf();
        cfg
    }

    #[test]
    fn record_durable_updates_metadata() {
        let tempdir = TempDir::new().expect("tempdir");
        let manager = manager();
        let cfg = base_config(tempdir.path());
        let mut store = PartitionLogStore::new(
            Arc::clone(&manager),
            cfg,
            tempdir.path().join("partitions"),
            Duration::from_secs(1),
            true,
        )
        .expect("store");

        store
            .record_durable(7, 1, 42, b"payload")
            .expect("record durable");

        let metadata = store.metadata(7).expect("metadata");
        assert_eq!(metadata.partition_id, 7);
        assert_eq!(metadata.highest_durable_index, 1);
        assert_eq!(metadata.next_partition_index, 2);
        assert_eq!(metadata.last_applied_raft_index, 42);

        drop(store);
        drop(manager);
    }

    #[test]
    fn reopen_loads_existing_metadata() {
        let tempdir = TempDir::new().expect("tempdir");
        let manager = manager();
        let cfg = base_config(tempdir.path());
        let partitions_root = tempdir.path().join("partitions");

        {
            let mut store = PartitionLogStore::new(
                Arc::clone(&manager),
                cfg.clone(),
                partitions_root.clone(),
                Duration::from_secs(1),
                false,
            )
            .expect("store");
            store
                .record_durable(11, 5, 100, b"alpha")
                .expect("record durable");
        }

        let mut reopened =
            PartitionLogStore::new(manager, cfg, partitions_root, Duration::from_secs(1), false)
                .expect("reopen");
        reopened
            .record_durable(11, 6, 101, b"beta")
            .expect("record durable");
        let metadata = reopened.metadata(11).expect("metadata");
        assert_eq!(metadata.highest_durable_index, 6);
        assert_eq!(metadata.next_partition_index, 7);
        assert_eq!(metadata.last_applied_raft_index, 101);
    }
}
