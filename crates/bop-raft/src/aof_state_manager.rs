use std::convert::TryInto;
use std::path::PathBuf;
use std::time::Duration;

use bop_aof::{
    Aof, AofConfig, AofManager, AofManagerConfig, error::AofError, reader::SegmentCursor,
};

use crate::buffer::Buffer;
use crate::config::{ClusterConfig, ClusterConfigView};
use crate::error::{RaftError, RaftResult};
use crate::state::{ServerState, ServerStateView};
use crate::traits::StateManagerInterface;
use crate::types::ServerId;

use bop_sys::{
    bop_raft_cluster_config_deserialize, bop_raft_cluster_config_serialize,
    bop_raft_srv_state_deserialize, bop_raft_srv_state_serialize,
};

const ENTRY_MAGIC: &[u8; 4] = b"BRSM";
const ENTRY_VERSION: u8 = 1;
const ENTRY_RESERVED_SIZE: usize = 2;
const ENTRY_HEADER_SIZE: usize = 4 + 1 + 1 + ENTRY_RESERVED_SIZE + 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    State = 1,
    Config = 2,
}

impl EntryKind {
    fn as_u8(self) -> u8 {
        match self {
            EntryKind::State => 1,
            EntryKind::Config => 2,
        }
    }

    fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(EntryKind::State),
            2 => Some(EntryKind::Config),
            _ => None,
        }
    }
}

/// Builder-style configuration for the AOF-backed state manager.
pub struct AofStateManagerConfig {
    pub server_id: ServerId,
    pub aof: AofConfig,
    pub manager: AofManagerConfig,
    pub append_timeout: Duration,
    pub verify_checksums: bool,
}

impl AofStateManagerConfig {
    pub fn new(root_dir: impl Into<PathBuf>, server_id: ServerId) -> Self {
        let mut aof = AofConfig::default();
        aof.root_dir = root_dir.into();
        Self {
            server_id,
            aof,
            manager: AofManagerConfig::default(),
            append_timeout: Duration::from_secs(5),
            verify_checksums: true,
        }
    }

    pub fn with_aof_config(mut self, config: AofConfig) -> Self {
        self.aof = config;
        self
    }

    pub fn with_manager_config(mut self, config: AofManagerConfig) -> Self {
        self.manager = config;
        self
    }

    pub fn append_timeout(mut self, timeout: Duration) -> Self {
        self.append_timeout = timeout;
        self
    }

    pub fn verify_checksums(mut self, verify: bool) -> Self {
        self.verify_checksums = verify;
        self
    }
}

/// State manager that persists metadata using bop-aof.
pub struct AofStateManager {
    server_id: ServerId,
    _manager: AofManager,
    aof: Aof,
    append_timeout: Duration,
    last_state_bytes: Option<Vec<u8>>,
    last_config_bytes: Option<Vec<u8>>,
}

impl AofStateManager {
    pub fn new(config: AofStateManagerConfig) -> RaftResult<Self> {
        let AofStateManagerConfig {
            server_id,
            aof,
            manager,
            append_timeout,
            verify_checksums,
        } = config;

        let manager = AofManager::with_config(manager).map_err(map_aof_err)?;
        let handle = manager.handle();
        let aof = Aof::new(handle, aof).map_err(map_aof_err)?;
        let (last_state_bytes, last_config_bytes) = Self::recover_records(&aof, verify_checksums)?;

        Ok(Self {
            server_id,
            _manager: manager,
            aof,
            append_timeout,
            last_state_bytes,
            last_config_bytes,
        })
    }

    fn serialize_state(state: ServerStateView<'_>) -> RaftResult<Vec<u8>> {
        let raw = unsafe { bop_raft_srv_state_serialize(state.as_ptr() as *mut _) };
        let buffer = unsafe { Buffer::from_raw(raw)? };
        Ok(buffer.to_vec())
    }

    fn serialize_config(config: ClusterConfigView<'_>) -> RaftResult<Vec<u8>> {
        let raw = unsafe { bop_raft_cluster_config_serialize(config.as_ptr() as *mut _) };
        let buffer = unsafe { Buffer::from_raw(raw)? };
        Ok(buffer.to_vec())
    }

    fn deserialize_state(bytes: &[u8]) -> RaftResult<ServerState> {
        let mut buffer = Buffer::from_bytes(bytes)?;
        let raw = unsafe { bop_raft_srv_state_deserialize(buffer.as_ptr()) };
        if raw.is_null() {
            return Err(RaftError::StateMachineError(
                "Failed to deserialize server state from AOF".to_string(),
            ));
        }
        unsafe { ServerState::from_raw(raw) }
    }

    fn deserialize_config(bytes: &[u8]) -> RaftResult<ClusterConfig> {
        let mut buffer = Buffer::from_bytes(bytes)?;
        let raw = unsafe { bop_raft_cluster_config_deserialize(buffer.as_ptr()) };
        if raw.is_null() {
            return Err(RaftError::StateMachineError(
                "Failed to deserialize cluster configuration from AOF".to_string(),
            ));
        }
        unsafe { ClusterConfig::from_raw(raw) }
    }

    fn append_entry(&mut self, kind: EntryKind, data: Vec<u8>) -> RaftResult<Vec<u8>> {
        let payload = encode_entry(kind, &data);
        let record_id = self
            .aof
            .append_record_with_timeout(&payload, self.append_timeout)
            .map_err(map_aof_err)?;
        self.aof.flush_until(record_id).map_err(map_aof_err)?;
        Ok(data)
    }

    fn recover_records(
        aof: &Aof,
        verify_checksums: bool,
    ) -> RaftResult<(Option<Vec<u8>>, Option<Vec<u8>>)> {
        let sealed = match aof.seal_active() {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(AofError::WouldBlock(_)) => false,
            Err(err) => return Err(map_aof_err(err)),
        };

        if !sealed {
            if let Err(err) = aof.force_rollover() {
                if !matches!(err, AofError::WouldBlock(_)) {
                    return Err(map_aof_err(err));
                }
            }
        }

        let mut snapshots = aof.segment_snapshot();
        snapshots.sort_by_key(|snapshot| snapshot.segment_id.as_u64());

        let mut last_state = None;
        let mut last_config = None;

        for snapshot in snapshots {
            if !snapshot.sealed {
                continue;
            }
            let reader = aof.open_reader(snapshot.segment_id).map_err(map_aof_err)?;
            let mut cursor = SegmentCursor::new(reader);
            Self::replay_cursor(
                &mut cursor,
                verify_checksums,
                &mut last_state,
                &mut last_config,
            )?;
        }

        Ok((last_state, last_config))
    }

    fn replay_cursor(
        cursor: &mut SegmentCursor,
        _verify_checksums: bool,
        last_state: &mut Option<Vec<u8>>,
        last_config: &mut Option<Vec<u8>>,
    ) -> RaftResult<()> {
        while let Some(record) = cursor.next().map_err(map_aof_err)? {
            let entry = decode_entry(record.payload())?;
            match entry.kind {
                EntryKind::State => *last_state = Some(entry.data),
                EntryKind::Config => *last_config = Some(entry.data),
            }
        }

        Ok(())
    }
}

impl StateManagerInterface for AofStateManager {
    fn load_state(&self) -> RaftResult<Option<ServerState>> {
        self.last_state_bytes
            .as_ref()
            .map(|bytes| Self::deserialize_state(bytes))
            .transpose()
    }

    fn save_state(&mut self, state: ServerStateView<'_>) -> RaftResult<()> {
        let data = Self::serialize_state(state)?;
        let data = self.append_entry(EntryKind::State, data)?;
        self.last_state_bytes = Some(data);
        Ok(())
    }

    fn load_config(&self) -> RaftResult<Option<ClusterConfig>> {
        self.last_config_bytes
            .as_ref()
            .map(|bytes| Self::deserialize_config(bytes))
            .transpose()
    }

    fn save_config(&mut self, config: ClusterConfigView<'_>) -> RaftResult<()> {
        let data = Self::serialize_config(config)?;
        let data = self.append_entry(EntryKind::Config, data)?;
        self.last_config_bytes = Some(data);
        Ok(())
    }

    fn server_id(&self) -> ServerId {
        self.server_id
    }
}

struct DecodedEntry {
    kind: EntryKind,
    data: Vec<u8>,
}

fn encode_entry(kind: EntryKind, data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(ENTRY_HEADER_SIZE + data.len());
    buf.extend_from_slice(ENTRY_MAGIC);
    buf.push(ENTRY_VERSION);
    buf.push(kind.as_u8());
    buf.extend_from_slice(&[0u8; ENTRY_RESERVED_SIZE]);
    buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
    buf.extend_from_slice(data);
    buf
}

fn decode_entry(payload: &[u8]) -> RaftResult<DecodedEntry> {
    if payload.len() < ENTRY_HEADER_SIZE {
        return Err(RaftError::StateMachineError(
            "AOF state manager record truncated".to_string(),
        ));
    }

    if &payload[0..4] != ENTRY_MAGIC {
        return Err(RaftError::StateMachineError(
            "Unexpected magic for AOF state manager record".to_string(),
        ));
    }

    if payload[4] != ENTRY_VERSION {
        return Err(RaftError::StateMachineError(format!(
            "Unsupported AOF state manager record version: {}",
            payload[4]
        )));
    }

    let kind = EntryKind::from_u8(payload[5]).ok_or_else(|| {
        RaftError::StateMachineError(format!(
            "Unknown AOF state manager record kind: {}",
            payload[5]
        ))
    })?;

    let declared_len = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
    if ENTRY_HEADER_SIZE + declared_len > payload.len() {
        return Err(RaftError::StateMachineError(
            "AOF state manager record payload truncated".to_string(),
        ));
    }

    let data = payload[ENTRY_HEADER_SIZE..ENTRY_HEADER_SIZE + declared_len].to_vec();
    Ok(DecodedEntry { kind, data })
}

fn map_aof_err(err: AofError) -> RaftError {
    RaftError::StateMachineError(format!("AOF error: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Term;
    use std::path::Path;
    use tempfile::TempDir;

    fn tempdir() -> RaftResult<TempDir> {
        TempDir::new().map_err(|err| RaftError::StateMachineError(format!("tempdir error: {err}")))
    }

    fn manager_config(root: &Path) -> AofStateManagerConfig {
        let mut aof_cfg = AofConfig::default();
        aof_cfg.root_dir = root.to_path_buf();
        let manager_cfg = AofManagerConfig::default().without_tier2();
        AofStateManagerConfig {
            server_id: ServerId(9),
            aof: aof_cfg,
            manager: manager_cfg,
            append_timeout: Duration::from_millis(250),
            verify_checksums: true,
        }
    }

    fn make_state_ptr(
        term: u64,
        voted_for: i32,
        election_allowed: bool,
        catching_up: bool,
        receiving_snapshot: bool,
    ) -> RaftResult<*mut bop_sys::bop_raft_srv_state> {
        let mut bytes = Vec::with_capacity(16);
        bytes.push(2); // version
        bytes.extend_from_slice(&term.to_le_bytes());
        bytes.extend_from_slice(&voted_for.to_le_bytes());
        bytes.push(if election_allowed { 1 } else { 0 });
        bytes.push(if catching_up { 1 } else { 0 });
        bytes.push(if receiving_snapshot { 1 } else { 0 });

        let mut buffer = Buffer::from_bytes(&bytes)?;
        let ptr = unsafe { bop_sys::bop_raft_srv_state_deserialize(buffer.as_ptr()) };
        if ptr.is_null() {
            Err(RaftError::StateMachineError(
                "failed to deserialize test server state".to_string(),
            ))
        } else {
            Ok(ptr)
        }
    }

    #[test]
    fn persists_and_recovers_latest_state_and_config() -> RaftResult<()> {
        let temp = tempdir()?;
        let storage_root = temp.path().join("state");

        let mut manager = AofStateManager::new(manager_config(&storage_root))?;
        assert!(manager.load_state()?.is_none());
        assert!(manager.load_config()?.is_none());

        let state1 = make_state_ptr(2, 7, true, false, false)?;
        let view1 = unsafe { ServerStateView::new(state1).expect("state view") };
        manager.save_state(view1)?;
        unsafe {
            bop_sys::bop_raft_srv_state_delete(state1);
        }

        let state2 = make_state_ptr(9, 11, false, true, true)?;
        let view2 = unsafe { ServerStateView::new(state2).expect("state view") };
        manager.save_state(view2)?;
        unsafe {
            bop_sys::bop_raft_srv_state_delete(state2);
        }

        let config1 = ClusterConfig::new()?;
        manager.save_config(config1.view())?;
        drop(config1);

        let config2 = ClusterConfig::new()?;
        manager.save_config(config2.view())?;
        drop(config2);

        let current = manager.load_state()?.expect("state after save");
        assert_eq!(current.term(), Term(9));
        assert_eq!(current.voted_for(), ServerId(11));
        assert!(!current.is_election_timer_allowed());
        assert!(current.is_catching_up());
        assert!(current.is_receiving_snapshot());

        let current_cfg = manager.load_config()?.expect("config after save");
        assert_eq!(current_cfg.servers_size(), 0);

        drop(manager);

        let reopened = AofStateManager::new(manager_config(&storage_root))?;
        let recovered = reopened.load_state()?.expect("recovered state");
        assert_eq!(recovered.term(), Term(9));
        assert_eq!(recovered.voted_for(), ServerId(11));
        assert!(!recovered.is_election_timer_allowed());
        assert!(recovered.is_catching_up());
        assert!(recovered.is_receiving_snapshot());

        let recovered_cfg = reopened.load_config()?.expect("recovered config");
        assert_eq!(recovered_cfg.servers_size(), 0);

        Ok(())
    }
}
