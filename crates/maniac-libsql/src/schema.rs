
/// Database ID type
pub type DatabaseId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BranchId(pub u32);

impl From<u32> for BranchId {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

/// Table Names
pub const DB_INFO_TABLE: &str = "databases";
pub const BRANCHES_TABLE: &str = "branches";
pub const SEGMENT_VERSIONS_TABLE: &str = "segment_versions";
pub const CHUNK_REFS_TABLE: &str = "chunk_refs";
pub const CHUNK_REF_COUNTS_TABLE: &str = "chunk_ref_counts";
pub const CHUNKS_TABLE: &str = "chunks";
pub const SYS_META_TABLE: &str = "sys_meta";
pub const WAL_MAP_TABLE: &str = "wal_map";
pub const CHECKPOINTS_TABLE: &str = "checkpoints";
pub const RAFT_LOG_TABLE: &str = "raft_log";
pub const RAFT_SEGMENTS_TABLE: &str = "raft_segments";
pub const RAFT_STATE_TABLE: &str = "raft_state";
pub const EVOLUTIONS_TABLE: &str = "evolutions";

#[derive(Debug, Clone)]
pub struct DatabaseInfo {
    pub name: String,
    pub created_at: u64,
    pub page_size: u32,
    pub chunk_size: u32,
}

impl DatabaseInfo {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let name_bytes = self.name.as_bytes();
        bytes.extend_from_slice(&(name_bytes.len() as u32).to_be_bytes());
        bytes.extend_from_slice(name_bytes);
        bytes.extend_from_slice(&self.created_at.to_be_bytes());
        bytes.extend_from_slice(&self.page_size.to_be_bytes());
        bytes.extend_from_slice(&self.chunk_size.to_be_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let mut offset = 0;
        if bytes.len() < 4 {
            anyhow::bail!("Invalid ID");
        }
        let name_len = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
        offset += 4;
        if bytes.len() < offset + name_len + 16 {
            anyhow::bail!("Invalid DB Info");
        }
        let name = String::from_utf8(bytes[offset..offset + name_len].to_vec())?;
        offset += name_len;
        let created_at = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;
        let page_size = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?);
        offset += 4;
        let chunk_size = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?);

        Ok(Self {
            name,
            created_at,
            page_size,
            chunk_size,
        })
    }
}

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub database_id: DatabaseId,
    pub name: String,
    pub parent_branch_id: Option<u32>,
    pub fork_at_version: u64,
    pub current_head_version: u64,
}

impl BranchInfo {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.database_id.to_be_bytes());
        let name_bytes = self.name.as_bytes();
        bytes.extend_from_slice(&(name_bytes.len() as u32).to_be_bytes());
        bytes.extend_from_slice(name_bytes);
        match self.parent_branch_id {
            Some(pid) => {
                bytes.push(1);
                bytes.extend_from_slice(&(pid as u32).to_be_bytes());
            }
            None => bytes.push(0),
        }
        bytes.extend_from_slice(&self.fork_at_version.to_be_bytes());
        bytes.extend_from_slice(&self.current_head_version.to_be_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let mut offset = 0;
        if bytes.len() < 8 {
            anyhow::bail!("Invalid Branch Info");
        }
        let database_id = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        if bytes.len() < offset + 4 {
            anyhow::bail!("Invalid Branch Name Len");
        }
        let name_len = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
        offset += 4;

        if bytes.len() < offset + name_len + 1 {
            anyhow::bail!("Incomplete Branch Info");
        }
        let name = String::from_utf8(bytes[offset..offset + name_len].to_vec())?;
        offset += name_len;

        let has_parent = bytes[offset] != 0;
        offset += 1;

        let parent_branch_id = if has_parent {
            if bytes.len() < offset + 4 {
                anyhow::bail!("Missing Parent ID");
            }
            let pid = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?);
            offset += 4;
            Some(pid)
        } else {
            None
        };

        if bytes.len() < offset + 16 {
            anyhow::bail!("Missing versions");
        }
        let fork_at_version = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;
        let current_head_version = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);

        Ok(Self {
            database_id,
            name,
            parent_branch_id,
            fork_at_version,
            current_head_version,
        })
    }
}

pub type ChunkId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundChunk {
    pub chunk_id: ChunkId,
}

#[derive(Debug, Clone)]
pub struct WalSegment {
    pub start_frame: u64,
    pub end_frame: u64,
    pub s3_key: String,
    pub timestamp: u64,
}

impl WalSegment {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.start_frame.to_be_bytes());
        bytes.extend_from_slice(&self.end_frame.to_be_bytes());
        let key_bytes = self.s3_key.as_bytes();
        bytes.extend_from_slice(&(key_bytes.len() as u32).to_be_bytes());
        bytes.extend_from_slice(key_bytes);
        bytes.extend_from_slice(&self.timestamp.to_be_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let mut offset = 0;
        if bytes.len() < 16 {
            anyhow::bail!("Invalid WalSegment");
        }
        let start_frame = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;
        let end_frame = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        if bytes.len() < offset + 4 {
            anyhow::bail!("Invalid key len");
        }
        let key_len = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
        offset += 4;

        if bytes.len() < offset + key_len + 8 {
            anyhow::bail!("Incomplete WalSegment");
        }
        let s3_key = String::from_utf8(bytes[offset..offset + key_len].to_vec())?;
        offset += key_len;

        let timestamp = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);

        Ok(Self {
            start_frame,
            end_frame,
            s3_key,
            timestamp,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CheckpointInfo {
    pub last_applied_frame: u64,
    pub base_layer_version: u64,
}

impl CheckpointInfo {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.last_applied_frame.to_be_bytes());
        bytes.extend_from_slice(&self.base_layer_version.to_be_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() != 16 {
            anyhow::bail!("Invalid CheckpointInfo");
        }
        let last_applied_frame = u64::from_be_bytes(bytes[0..8].try_into()?);
        let base_layer_version = u64::from_be_bytes(bytes[8..16].try_into()?);
        Ok(Self {
            last_applied_frame,
            base_layer_version,
        })
    }
}

pub struct SegmentVersionKey {
    pub database_id: u64,
    pub branch_id: u32,
    pub segment_id: u32,
    pub version_inverted: u64,
}

impl SegmentVersionKey {
    pub fn to_bytes(&self) -> [u8; 24] {
        let mut buf = [0u8; 24];
        let mut offset = 0;
        buf[offset..offset + 8].copy_from_slice(&self.database_id.to_be_bytes());
        offset += 8;
        buf[offset..offset + 4].copy_from_slice(&self.branch_id.to_be_bytes());
        offset += 4;
        buf[offset..offset + 4].copy_from_slice(&self.segment_id.to_be_bytes());
        offset += 4;
        buf[offset..offset + 8].copy_from_slice(&self.version_inverted.to_be_bytes());
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 24 {
            return None;
        }
        let mut offset = 0;
        let database_id = u64::from_be_bytes(bytes[offset..offset + 8].try_into().unwrap());
        offset += 8;
        let branch_id = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap());
        offset += 4;
        let segment_id = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap());
        offset += 4;
        let version_inverted = u64::from_be_bytes(bytes[offset..offset + 8].try_into().unwrap());

        Some(Self {
            database_id,
            branch_id,
            segment_id,
            version_inverted,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WalMapKey {
    pub database_id: u64,
    pub branch_id: u32,
    pub start_frame: u64,
}

impl WalMapKey {
    pub fn to_bytes(&self) -> [u8; 20] {
        let mut buf = [0u8; 20];
        buf[0..8].copy_from_slice(&self.database_id.to_be_bytes());
        buf[8..12].copy_from_slice(&self.branch_id.to_be_bytes());
        buf[12..20].copy_from_slice(&self.start_frame.to_be_bytes());
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 20 {
            return None;
        }
        let database_id = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        let branch_id = u32::from_be_bytes(bytes[8..12].try_into().unwrap());
        let start_frame = u64::from_be_bytes(bytes[12..20].try_into().unwrap());
        Some(Self {
            database_id,
            branch_id,
            start_frame,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BranchKey {
    pub database_id: u64,
    pub branch_id: u32,
}

impl BranchKey {
    pub fn to_bytes(&self) -> [u8; 12] {
        let mut buf = [0u8; 12];
        buf[0..8].copy_from_slice(&self.database_id.to_be_bytes());
        buf[8..12].copy_from_slice(&self.branch_id.to_be_bytes());
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 12 {
            return None;
        }
        let database_id = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        let branch_id = u32::from_be_bytes(bytes[8..12].try_into().unwrap());
        Some(Self {
            database_id,
            branch_id,
        })
    }
}

// ============================================================================
// Raft Log Types
// ============================================================================

/// Key for raft log entries.
/// Format: [db_id (8)] [branch_id (4)] [log_index (8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RaftLogKey {
    pub database_id: u64,
    pub branch_id: u32,
    pub log_index: u64,
}

impl RaftLogKey {
    pub fn to_bytes(&self) -> [u8; 20] {
        let mut buf = [0u8; 20];
        buf[0..8].copy_from_slice(&self.database_id.to_be_bytes());
        buf[8..12].copy_from_slice(&self.branch_id.to_be_bytes());
        buf[12..20].copy_from_slice(&self.log_index.to_be_bytes());
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 20 {
            return None;
        }
        let database_id = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        let branch_id = u32::from_be_bytes(bytes[8..12].try_into().unwrap());
        let log_index = u64::from_be_bytes(bytes[12..20].try_into().unwrap());
        Some(Self {
            database_id,
            branch_id,
            log_index,
        })
    }
}

/// Types of entries that can appear in the raft log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RaftEntryType {
    /// WAL metadata update - indicates new WAL segment available in S3.
    WalMetadata = 1,
    /// SQLite session changeset - can be used to reconstruct WAL.
    Changeset = 2,
    /// Checkpoint complete - WAL has been applied to base layer.
    Checkpoint = 3,
    /// Branch created.
    BranchCreated = 4,
}

impl RaftEntryType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::WalMetadata),
            2 => Some(Self::Changeset),
            3 => Some(Self::Checkpoint),
            4 => Some(Self::BranchCreated),
            _ => None,
        }
    }
}

/// Metadata for a raft log entry stored in libmdbx.
///
/// The raft log is replicated across nodes and contains:
/// - Metadata updates (WAL segment info, checkpoints)
/// - Pointers to changeset data in segment files
///
/// The actual changeset data lives in segmented files (like WAL),
/// not in libmdbx. This struct contains only lightweight metadata.
#[derive(Debug, Clone)]
pub struct RaftLogEntry {
    /// Raft term when this entry was created.
    pub term: u64,
    /// Type of this entry.
    pub entry_type: RaftEntryType,
    /// WAL frame range this entry covers (if applicable).
    pub start_frame: u64,
    pub end_frame: u64,
    /// Timestamp when this entry was created.
    pub timestamp: u64,
    /// For small metadata (WAL info, checkpoint), inline payload.
    /// For changesets, this is empty - data lives in segment files.
    pub inline_payload: Vec<u8>,
}

impl RaftLogEntry {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(37 + self.inline_payload.len());
        bytes.extend_from_slice(&self.term.to_be_bytes());
        bytes.push(self.entry_type as u8);
        bytes.extend_from_slice(&self.start_frame.to_be_bytes());
        bytes.extend_from_slice(&self.end_frame.to_be_bytes());
        bytes.extend_from_slice(&self.timestamp.to_be_bytes());
        bytes.extend_from_slice(&(self.inline_payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&self.inline_payload);
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() < 37 {
            anyhow::bail!("RaftLogEntry too short");
        }

        let mut offset = 0;
        let term = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        let entry_type = RaftEntryType::from_u8(bytes[offset])
            .ok_or_else(|| anyhow::anyhow!("Invalid entry type"))?;
        offset += 1;

        let start_frame = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        let end_frame = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        let timestamp = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        let payload_len = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
        offset += 4;

        if bytes.len() < offset + payload_len {
            anyhow::bail!("RaftLogEntry payload truncated");
        }

        let inline_payload = bytes[offset..offset + payload_len].to_vec();

        Ok(Self {
            term,
            entry_type,
            start_frame,
            end_frame,
            timestamp,
            inline_payload,
        })
    }

    /// Create a WAL metadata entry (inline payload with S3 key).
    pub fn wal_metadata(term: u64, segment: &WalSegment) -> Self {
        Self {
            term,
            entry_type: RaftEntryType::WalMetadata,
            start_frame: segment.start_frame,
            end_frame: segment.end_frame,
            timestamp: segment.timestamp,
            inline_payload: segment.s3_key.as_bytes().to_vec(),
        }
    }

    /// Create a changeset entry (no inline payload - data in segment file).
    pub fn changeset(term: u64, start_frame: u64, end_frame: u64, timestamp: u64) -> Self {
        Self {
            term,
            entry_type: RaftEntryType::Changeset,
            start_frame,
            end_frame,
            timestamp,
            inline_payload: Vec::new(),
        }
    }

    /// Create a checkpoint entry (inline payload with checkpoint info).
    pub fn checkpoint(term: u64, info: &CheckpointInfo, timestamp: u64) -> Self {
        Self {
            term,
            entry_type: RaftEntryType::Checkpoint,
            start_frame: info.last_applied_frame,
            end_frame: info.last_applied_frame,
            timestamp,
            inline_payload: info.to_bytes(),
        }
    }
}

/// Metadata for a raft log segment file.
///
/// Raft log entries with changeset data are stored in segment files.
/// This struct tracks the segment file metadata.
#[derive(Debug, Clone)]
pub struct RaftSegment {
    /// First log index in this segment.
    pub start_index: u64,
    /// Last log index in this segment.
    pub end_index: u64,
    /// File path or S3 key for this segment.
    pub path: String,
    /// Total size in bytes.
    pub size_bytes: u64,
    /// Timestamp when this segment was created.
    pub timestamp: u64,
}

impl RaftSegment {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.start_index.to_be_bytes());
        bytes.extend_from_slice(&self.end_index.to_be_bytes());
        let path_bytes = self.path.as_bytes();
        bytes.extend_from_slice(&(path_bytes.len() as u32).to_be_bytes());
        bytes.extend_from_slice(path_bytes);
        bytes.extend_from_slice(&self.size_bytes.to_be_bytes());
        bytes.extend_from_slice(&self.timestamp.to_be_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() < 20 {
            anyhow::bail!("RaftSegment too short");
        }

        let mut offset = 0;
        let start_index = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        let end_index = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        let path_len = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
        offset += 4;

        if bytes.len() < offset + path_len + 16 {
            anyhow::bail!("RaftSegment truncated");
        }

        let path = String::from_utf8(bytes[offset..offset + path_len].to_vec())?;
        offset += path_len;

        let size_bytes = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        let timestamp = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);

        Ok(Self {
            start_index,
            end_index,
            path,
            size_bytes,
            timestamp,
        })
    }
}

/// Key for raft segment metadata.
/// Format: [db_id (8)] [branch_id (4)] [start_index (8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RaftSegmentKey {
    pub database_id: u64,
    pub branch_id: u32,
    pub start_index: u64,
}

impl RaftSegmentKey {
    pub fn to_bytes(&self) -> [u8; 20] {
        let mut buf = [0u8; 20];
        buf[0..8].copy_from_slice(&self.database_id.to_be_bytes());
        buf[8..12].copy_from_slice(&self.branch_id.to_be_bytes());
        buf[12..20].copy_from_slice(&self.start_index.to_be_bytes());
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 20 {
            return None;
        }
        let database_id = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        let branch_id = u32::from_be_bytes(bytes[8..12].try_into().unwrap());
        let start_index = u64::from_be_bytes(bytes[12..20].try_into().unwrap());
        Some(Self {
            database_id,
            branch_id,
            start_index,
        })
    }
}

/// Raft log state for a branch.
#[derive(Debug, Clone)]
pub struct RaftState {
    /// Current term.
    pub current_term: u64,
    /// Last log index that has been committed.
    pub commit_index: u64,
    /// Last log index that has been applied to state machine.
    pub last_applied: u64,
    /// Voted for in current term (node ID or 0 if none).
    pub voted_for: u64,
}

impl RaftState {
    pub fn new() -> Self {
        Self {
            current_term: 0,
            commit_index: 0,
            last_applied: 0,
            voted_for: 0,
        }
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        let mut buf = [0u8; 32];
        buf[0..8].copy_from_slice(&self.current_term.to_be_bytes());
        buf[8..16].copy_from_slice(&self.commit_index.to_be_bytes());
        buf[16..24].copy_from_slice(&self.last_applied.to_be_bytes());
        buf[24..32].copy_from_slice(&self.voted_for.to_be_bytes());
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() != 32 {
            anyhow::bail!("Invalid RaftState");
        }
        Ok(Self {
            current_term: u64::from_be_bytes(bytes[0..8].try_into()?),
            commit_index: u64::from_be_bytes(bytes[8..16].try_into()?),
            last_applied: u64::from_be_bytes(bytes[16..24].try_into()?),
            voted_for: u64::from_be_bytes(bytes[24..32].try_into()?),
        })
    }
}

impl Default for RaftState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Schema Evolution Types
// ============================================================================

/// Unique identifier for a schema evolution.
pub type EvolutionId = u64;

/// State of a schema evolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EvolutionState {
    /// Evolution created, target branch forked from source.
    Created = 1,
    /// DDL changes being applied to target branch.
    ApplyingDdl = 2,
    /// DDL applied, replaying changesets from source branch.
    ReplayingChangesets = 3,
    /// Changesets caught up, ready for cutover.
    ReadyForCutover = 4,
    /// Source branch writes stopped, final sync in progress.
    Cutover = 5,
    /// Evolution complete, target branch is now the active branch.
    Completed = 6,
    /// Evolution failed and was rolled back.
    Failed = 7,
    /// Evolution was cancelled.
    Cancelled = 8,
}

impl EvolutionState {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Created),
            2 => Some(Self::ApplyingDdl),
            3 => Some(Self::ReplayingChangesets),
            4 => Some(Self::ReadyForCutover),
            5 => Some(Self::Cutover),
            6 => Some(Self::Completed),
            7 => Some(Self::Failed),
            8 => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Check if evolution is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Check if evolution is still active.
    pub fn is_active(&self) -> bool {
        !self.is_terminal()
    }
}

/// Key for evolution records.
/// Format: [db_id (8)] [evolution_id (8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EvolutionKey {
    pub database_id: u64,
    pub evolution_id: EvolutionId,
}

impl EvolutionKey {
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        buf[0..8].copy_from_slice(&self.database_id.to_be_bytes());
        buf[8..16].copy_from_slice(&self.evolution_id.to_be_bytes());
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 16 {
            return None;
        }
        let database_id = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        let evolution_id = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
        Some(Self {
            database_id,
            evolution_id,
        })
    }
}

/// A single DDL statement in the evolution.
#[derive(Debug, Clone)]
pub struct EvolutionStep {
    /// Order of this step in the evolution.
    pub step_number: u32,
    /// The DDL SQL statement.
    pub ddl_sql: String,
    /// Whether this step has been applied.
    pub applied: bool,
    /// Timestamp when this step was applied (0 if not yet).
    pub applied_at: u64,
    /// Error message if this step failed.
    pub error: Option<String>,
}

impl EvolutionStep {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.step_number.to_be_bytes());
        let sql_bytes = self.ddl_sql.as_bytes();
        bytes.extend_from_slice(&(sql_bytes.len() as u32).to_be_bytes());
        bytes.extend_from_slice(sql_bytes);
        bytes.push(if self.applied { 1 } else { 0 });
        bytes.extend_from_slice(&self.applied_at.to_be_bytes());
        match &self.error {
            Some(err) => {
                bytes.push(1);
                let err_bytes = err.as_bytes();
                bytes.extend_from_slice(&(err_bytes.len() as u32).to_be_bytes());
                bytes.extend_from_slice(err_bytes);
            }
            None => bytes.push(0),
        }
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() < 4 {
            anyhow::bail!("EvolutionStep too short");
        }
        let mut offset = 0;

        let step_number = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?);
        offset += 4;

        if bytes.len() < offset + 4 {
            anyhow::bail!("EvolutionStep: missing sql len");
        }
        let sql_len = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
        offset += 4;

        if bytes.len() < offset + sql_len + 9 {
            anyhow::bail!("EvolutionStep: incomplete");
        }
        let ddl_sql = String::from_utf8(bytes[offset..offset + sql_len].to_vec())?;
        offset += sql_len;

        let applied = bytes[offset] != 0;
        offset += 1;

        let applied_at = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        let has_error = bytes[offset] != 0;
        offset += 1;

        let error = if has_error {
            if bytes.len() < offset + 4 {
                anyhow::bail!("EvolutionStep: missing error len");
            }
            let err_len = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
            offset += 4;
            if bytes.len() < offset + err_len {
                anyhow::bail!("EvolutionStep: error truncated");
            }
            Some(String::from_utf8(bytes[offset..offset + err_len].to_vec())?)
        } else {
            None
        };

        Ok(Self {
            step_number,
            ddl_sql,
            applied,
            applied_at,
            error,
        })
    }
}

/// Schema evolution record.
///
/// Tracks a zero-downtime schema evolution from a source branch to a target branch.
///
/// # Evolution Flow
/// 1. **Created**: Fork target branch from source at current version
/// 2. **ApplyingDdl**: Apply DDL changes to target branch
/// 3. **ReplayingChangesets**: Replay changesets from source that occurred since fork
/// 4. **ReadyForCutover**: Caught up, waiting for cutover command
/// 5. **Cutover**: Source writes stopped, final sync
/// 6. **Completed**: Target is now the active branch
#[derive(Debug, Clone)]
pub struct Evolution {
    /// Unique ID for this evolution.
    pub id: EvolutionId,
    /// Human-readable name/description.
    pub name: String,
    /// Source branch (where we forked from).
    pub source_branch_id: u32,
    /// Target branch (where DDL changes are applied).
    pub target_branch_id: u32,
    /// Current state of the evolution.
    pub state: EvolutionState,
    /// Version of source branch when we forked.
    pub fork_version: u64,
    /// Last raft log index from source that we've replayed.
    pub replayed_log_index: u64,
    /// Timestamp when evolution was created.
    pub created_at: u64,
    /// Timestamp when evolution was last updated.
    pub updated_at: u64,
    /// Timestamp when evolution completed (0 if not yet).
    pub completed_at: u64,
    /// Error message if evolution failed.
    pub error: Option<String>,
    /// DDL steps to apply.
    pub steps: Vec<EvolutionStep>,
}

impl Evolution {
    pub fn new(
        id: EvolutionId,
        name: String,
        source_branch_id: u32,
        target_branch_id: u32,
        fork_version: u64,
        steps: Vec<EvolutionStep>,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            id,
            name,
            source_branch_id,
            target_branch_id,
            state: EvolutionState::Created,
            fork_version,
            replayed_log_index: 0,
            created_at: now,
            updated_at: now,
            completed_at: 0,
            error: None,
            steps,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&self.id.to_be_bytes());

        let name_bytes = self.name.as_bytes();
        bytes.extend_from_slice(&(name_bytes.len() as u32).to_be_bytes());
        bytes.extend_from_slice(name_bytes);

        bytes.extend_from_slice(&self.source_branch_id.to_be_bytes());
        bytes.extend_from_slice(&self.target_branch_id.to_be_bytes());
        bytes.push(self.state as u8);
        bytes.extend_from_slice(&self.fork_version.to_be_bytes());
        bytes.extend_from_slice(&self.replayed_log_index.to_be_bytes());
        bytes.extend_from_slice(&self.created_at.to_be_bytes());
        bytes.extend_from_slice(&self.updated_at.to_be_bytes());
        bytes.extend_from_slice(&self.completed_at.to_be_bytes());

        match &self.error {
            Some(err) => {
                bytes.push(1);
                let err_bytes = err.as_bytes();
                bytes.extend_from_slice(&(err_bytes.len() as u32).to_be_bytes());
                bytes.extend_from_slice(err_bytes);
            }
            None => bytes.push(0),
        }

        // Serialize steps
        bytes.extend_from_slice(&(self.steps.len() as u32).to_be_bytes());
        for step in &self.steps {
            let step_bytes = step.to_bytes();
            bytes.extend_from_slice(&(step_bytes.len() as u32).to_be_bytes());
            bytes.extend_from_slice(&step_bytes);
        }

        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() < 8 {
            anyhow::bail!("Evolution too short");
        }
        let mut offset = 0;

        let id = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        if bytes.len() < offset + 4 {
            anyhow::bail!("Evolution: missing name len");
        }
        let name_len = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
        offset += 4;

        if bytes.len() < offset + name_len {
            anyhow::bail!("Evolution: name truncated");
        }
        let name = String::from_utf8(bytes[offset..offset + name_len].to_vec())?;
        offset += name_len;

        if bytes.len() < offset + 57 {
            anyhow::bail!("Evolution: incomplete fixed fields");
        }

        let source_branch_id = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?);
        offset += 4;

        let target_branch_id = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?);
        offset += 4;

        let state = EvolutionState::from_u8(bytes[offset])
            .ok_or_else(|| anyhow::anyhow!("Invalid evolution state"))?;
        offset += 1;

        let fork_version = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        let replayed_log_index = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        let created_at = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        let updated_at = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        let completed_at = u64::from_be_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        let has_error = bytes[offset] != 0;
        offset += 1;

        let error = if has_error {
            if bytes.len() < offset + 4 {
                anyhow::bail!("Evolution: missing error len");
            }
            let err_len = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
            offset += 4;
            if bytes.len() < offset + err_len {
                anyhow::bail!("Evolution: error truncated");
            }
            let err = String::from_utf8(bytes[offset..offset + err_len].to_vec())?;
            offset += err_len;
            Some(err)
        } else {
            None
        };

        if bytes.len() < offset + 4 {
            anyhow::bail!("Evolution: missing steps count");
        }
        let steps_count = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
        offset += 4;

        let mut steps = Vec::with_capacity(steps_count);
        for _ in 0..steps_count {
            if bytes.len() < offset + 4 {
                anyhow::bail!("Evolution: missing step len");
            }
            let step_len = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
            offset += 4;

            if bytes.len() < offset + step_len {
                anyhow::bail!("Evolution: step truncated");
            }
            let step = EvolutionStep::from_bytes(&bytes[offset..offset + step_len])?;
            offset += step_len;
            steps.push(step);
        }

        Ok(Self {
            id,
            name,
            source_branch_id,
            target_branch_id,
            state,
            fork_version,
            replayed_log_index,
            created_at,
            updated_at,
            completed_at,
            error,
            steps,
        })
    }
}
