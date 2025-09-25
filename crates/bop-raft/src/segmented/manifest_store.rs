//! Manifest persistence helpers for segmented partitions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bop_aof::config::SegmentId;
use bop_aof::error::AofError;
use bop_aof::manifest::{
    ManifestLogReader, ManifestLogWriter, ManifestLogWriterConfig, ManifestRecord,
    ManifestRecordPayload, RECORD_HEADER_LEN,
};

use crate::error::{RaftError, RaftResult};
use crate::segmented::manifest::{PartitionManifest, PartitionManifestUpdate};
use crate::segmented::types::{PartitionId, ReplicaId};

const SCHEMA_SEGMENTED_MANIFEST_UPDATE: u32 = 0x5347_4d31; // "SGM1"
const UPDATE_PAYLOAD_VERSION: u8 = 1;
const UPDATE_FLAG_DURABLE: u8 = 0b0001;
const UPDATE_FLAG_APPLIED: u8 = 0b0010;
const UPDATE_FLAG_GLOBAL: u8 = 0b0100;
const UPDATE_FLAG_SNAPSHOT_SET: u8 = 0b1000;
const UPDATE_FLAG_SNAPSHOT_CLEAR: u8 = 0b1_0000;
const UPDATE_FLAG_MASTER_SET: u8 = 0b10_0000;
const UPDATE_FLAG_MASTER_CLEAR: u8 = 0b100_0000;

const MAX_UPDATE_BLOB_LEN: usize = 1  // version
    + 1 // flags
    + 8 // partition id
    + (5 * 8); // up to five optional u64 values
const CUSTOM_EVENT_HEADER_LEN: usize = 4 /* schema id */ + 2 /* blob len */;
const MAX_MANIFEST_RECORD_LEN: usize =
    RECORD_HEADER_LEN + CUSTOM_EVENT_HEADER_LEN + MAX_UPDATE_BLOB_LEN;

fn map_aof_err(err: AofError) -> RaftError {
    RaftError::LogStoreError(format!("manifest error: {err}"))
}

fn encode_update_blob(partition_id: PartitionId, update: &PartitionManifestUpdate) -> Vec<u8> {
    let mut blob = Vec::with_capacity(MAX_UPDATE_BLOB_LEN);
    blob.push(UPDATE_PAYLOAD_VERSION);
    let mut flags = 0u8;
    let mut payload = Vec::with_capacity(32);

    if let Some(durable) = update.durable_index {
        flags |= UPDATE_FLAG_DURABLE;
        payload.extend_from_slice(&durable.to_le_bytes());
    }
    if let Some(applied) = update.applied_index {
        flags |= UPDATE_FLAG_APPLIED;
        payload.extend_from_slice(&applied.to_le_bytes());
    }
    if let Some(global) = update.applied_global {
        flags |= UPDATE_FLAG_GLOBAL;
        payload.extend_from_slice(&global.to_le_bytes());
    }
    if let Some(snapshot_opt) = update.snapshot_index {
        match snapshot_opt {
            Some(snapshot) => {
                flags |= UPDATE_FLAG_SNAPSHOT_SET;
                payload.extend_from_slice(&snapshot.to_le_bytes());
            }
            None => {
                flags |= UPDATE_FLAG_SNAPSHOT_CLEAR;
            }
        }
    }
    if let Some(master_opt) = update.master {
        match master_opt {
            Some(replica) => {
                flags |= UPDATE_FLAG_MASTER_SET;
                let value: u64 = replica.into();
                payload.extend_from_slice(&value.to_le_bytes());
            }
            None => {
                flags |= UPDATE_FLAG_MASTER_CLEAR;
            }
        }
    }

    blob.push(flags);
    blob.extend_from_slice(&partition_id.to_le_bytes());
    blob.extend_from_slice(&payload);
    blob
}

fn decode_update_blob(blob: &[u8]) -> Option<(PartitionId, PartitionManifestUpdate)> {
    if blob.len() < 1 + 1 + 8 {
        return None;
    }
    let version = blob[0];
    if version != UPDATE_PAYLOAD_VERSION {
        return None;
    }
    let flags = blob[1];
    let partition_id = u64::from_le_bytes(blob[2..10].try_into().ok()?);
    let mut cursor = &blob[10..];

    let next_u64 = |cursor: &mut &[u8]| -> Option<u64> {
        if cursor.len() < 8 {
            return None;
        }
        let (value_bytes, rest) = cursor.split_at(8);
        *cursor = rest;
        Some(u64::from_le_bytes(value_bytes.try_into().ok()?))
    };

    let durable_index = if flags & UPDATE_FLAG_DURABLE != 0 {
        next_u64(&mut cursor)
    } else {
        None
    };
    let applied_index = if flags & UPDATE_FLAG_APPLIED != 0 {
        next_u64(&mut cursor)
    } else {
        None
    };
    let applied_global = if flags & UPDATE_FLAG_GLOBAL != 0 {
        next_u64(&mut cursor)
    } else {
        None
    };
    let snapshot_index = if flags & UPDATE_FLAG_SNAPSHOT_SET != 0 {
        next_u64(&mut cursor).map(|value| Some(value))
    } else if flags & UPDATE_FLAG_SNAPSHOT_CLEAR != 0 {
        Some(None)
    } else {
        None
    };
    let master = if flags & UPDATE_FLAG_MASTER_SET != 0 {
        next_u64(&mut cursor).map(|value| Some(ReplicaId::from(value)))
    } else if flags & UPDATE_FLAG_MASTER_CLEAR != 0 {
        Some(None)
    } else {
        None
    };

    Some((
        partition_id,
        PartitionManifestUpdate {
            durable_index,
            applied_index,
            applied_global,
            snapshot_index,
            master,
        },
    ))
}

/// Persistent journal for partition manifest updates.
pub struct PartitionManifestStore {
    writer: ManifestLogWriter,
    base_dir: PathBuf,
    stream_id: u64,
    max_record_len: usize,
}

impl std::fmt::Debug for PartitionManifestStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PartitionManifestStore")
            .field("base_dir", &self.base_dir)
            .field("stream_id", &self.stream_id)
            .finish()
    }
}

impl PartitionManifestStore {
    /// Create a manifest update store rooted at `base_dir/stream_id`.
    pub fn new(
        base_dir: impl AsRef<Path>,
        stream_id: u64,
        mut config: ManifestLogWriterConfig,
    ) -> RaftResult<Self> {
        if !config.enabled {
            config.enabled = true;
        }
        let writer = ManifestLogWriter::new(base_dir.as_ref().to_path_buf(), stream_id, config)
            .map_err(map_aof_err)?;
        Ok(Self {
            writer,
            base_dir: base_dir.as_ref().to_path_buf(),
            stream_id,
            max_record_len: MAX_MANIFEST_RECORD_LEN,
        })
    }

    /// Append an update and commit/rotate the manifest log as needed.
    pub fn append_update(
        &mut self,
        manifest: &PartitionManifest,
        update: &PartitionManifestUpdate,
    ) -> RaftResult<()> {
        if update.is_empty() {
            return Ok(());
        }
        let blob = encode_update_blob(manifest.partition_id(), update);
        let payload = ManifestRecordPayload::CustomEvent {
            schema_id: SCHEMA_SEGMENTED_MANIFEST_UPDATE,
            blob,
        };
        let segment_id = SegmentId::new(manifest.partition_id());
        let logical_offset = update
            .durable_index
            .or(update.applied_index)
            .unwrap_or_else(|| manifest.durable_index());
        self.writer
            .append_now(segment_id, logical_offset, payload)
            .map_err(map_aof_err)?;
        self.writer.commit().map_err(map_aof_err)?;
        self.writer
            .maybe_rotate(self.max_record_len)
            .map_err(map_aof_err)?;
        Ok(())
    }

    /// Read back all stored manifest updates (primarily for recovery/testing).
    pub fn load_updates(&self) -> RaftResult<Vec<(PartitionId, PartitionManifestUpdate)>> {
        let reader =
            ManifestLogReader::open(self.base_dir.clone(), self.stream_id).map_err(map_aof_err)?;
        let mut updates = Vec::new();
        for record in reader.iter() {
            let record = record.map_err(map_aof_err)?;
            if let ManifestRecord {
                payload: ManifestRecordPayload::CustomEvent { schema_id, blob },
                ..
            } = record
            {
                if schema_id == SCHEMA_SEGMENTED_MANIFEST_UPDATE {
                    if let Some(parsed) = decode_update_blob(&blob) {
                        updates.push(parsed);
                    }
                }
            }
        }
        Ok(updates)
    }

    /// Reconstruct partition manifests by replaying persisted updates.
    pub fn load_manifests(&self) -> RaftResult<HashMap<PartitionId, PartitionManifest>> {
        let mut manifests = HashMap::new();
        for (partition_id, update) in self.load_updates()? {
            let entry = manifests
                .entry(partition_id)
                .or_insert_with(|| PartitionManifest::new(partition_id));
            entry.apply_update(&update);
        }
        Ok(manifests)
    }

    /// Access the base directory where manifest chunks reside.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Stream identifier used for chunk naming.
    pub fn stream_id(&self) -> u64 {
        self.stream_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bop_aof::manifest::ManifestLogWriterConfig;
    use tempfile::TempDir;

    fn writer_config() -> ManifestLogWriterConfig {
        ManifestLogWriterConfig {
            chunk_capacity_bytes: 16 * 1024,
            rotation_period: std::time::Duration::from_secs(1),
            growth_step_bytes: 16 * 1024,
            enabled: true,
        }
    }

    fn manifest_update() -> PartitionManifestUpdate {
        PartitionManifestUpdate {
            durable_index: Some(5),
            applied_index: Some(3),
            applied_global: Some(42),
            snapshot_index: Some(Some(2)),
            master: Some(Some(ReplicaId::from(7u64))),
        }
    }

    #[test]
    fn append_and_replay_manifest_updates() {
        let dir = TempDir::new().expect("tempdir");
        let mut store = PartitionManifestStore::new(dir.path(), 1, writer_config()).expect("store");

        let mut manifest = PartitionManifest::new(11);
        manifest.record_durable(5, 42);
        manifest.record_applied(3, 42);
        let update = manifest_update();
        store
            .append_update(&manifest, &update)
            .expect("append update");

        let updates = store.load_updates().expect("load updates");
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].0, 11);
        assert_eq!(updates[0].1, update);
    }

    #[test]
    fn skip_empty_updates() {
        let dir = TempDir::new().expect("tempdir");
        let mut store = PartitionManifestStore::new(dir.path(), 2, writer_config()).expect("store");
        let manifest = PartitionManifest::new(5);
        let update = PartitionManifestUpdate::default();
        store
            .append_update(&manifest, &update)
            .expect("append empty");
        let updates = store.load_updates().expect("load updates");
        assert!(updates.is_empty());
    }

    #[test]
    fn load_manifests_reconstructs_state() {
        let dir = TempDir::new().expect("tempdir");
        let mut store = PartitionManifestStore::new(dir.path(), 3, writer_config()).expect("store");

        let mut manifest = PartitionManifest::new(17);
        manifest.record_durable(4, 10);
        manifest.record_applied(3, 10);
        manifest.record_snapshot(3);
        let update = PartitionManifestUpdate {
            durable_index: Some(6),
            applied_index: Some(5),
            applied_global: Some(12),
            snapshot_index: Some(Some(4)),
            master: Some(Some(ReplicaId::from(3u64))),
        };
        store
            .append_update(&manifest, &update)
            .expect("append update");

        let manifests = store.load_manifests().expect("load manifests");
        assert_eq!(manifests.len(), 1);
        let recovered = manifests.get(&17).expect("manifest present");
        assert_eq!(recovered.durable_index(), 6);
        assert_eq!(recovered.applied_index(), 5);
        assert_eq!(recovered.applied_global(), 12);
        assert_eq!(recovered.snapshot_index(), Some(4));
        assert_eq!(recovered.master(), Some(ReplicaId::from(3u64)));
    }
}
