use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::Utc;
use dashmap::DashMap;
use tokio::runtime::Runtime;

pub mod config;
pub mod error;
pub mod flush;
pub mod fs;
pub mod segment;

pub use config::{
    AofConfig, CompactionPolicy, Compression, FlushConfig, IdStrategy, RecordId, RetentionPolicy,
    SegmentId,
};
pub use fs::{
    create_fixed_size_file, fsync_dir, Layout, SegmentFileName, TempFileGuard, SEGMENT_FILE_EXTENSION,
};

use error::{AofError, AofResult};
use segment::Segment;

pub struct AofManagerConfig {
    pub max_segment_cache: u64,
    pub flush: FlushConfig,
}

impl Default for AofManagerConfig {
    fn default() -> Self {
        Self {
            max_segment_cache: 128,
            flush: FlushConfig::default(),
        }
    }
}

pub struct AofManager {
    rt: Arc<Runtime>,
    flush_map: dashmap::DashMap<u64, Arc<AofAppender>>,
    background_thread: Option<std::thread::JoinHandle<()>>,
}

impl AofManager {}

struct AofAppender {
    inner: Arc<Mutex<AofAppenderInner>>,
}

struct AofAppenderInner {
    aof: Arc<Aof>,
    segment: Arc<Segment>,
    pending: AtomicU64,
}

struct AofState {
    segments: Vec<Arc<Segment>>,
    tail: Option<Arc<Segment>>,
    next_segment_index: u32,
    next_offset: u64,
    record_count: u64,
}

pub struct Aof {
    rt: Arc<Runtime>,
    config: AofConfig,
    layout: Layout,
    state: Mutex<AofState>,
}

impl Aof {
    pub fn new(rt: Arc<Runtime>, config: AofConfig) -> AofResult<Self> {
        let normalized = config.normalized();
        let layout = Layout::new(&normalized);
        layout.ensure()?;
        Ok(Self {
            rt,
            config: normalized,
            layout,
            state: Mutex::new(AofState {
                segments: Vec::new(),
                tail: None,
                next_segment_index: 0,
                next_offset: 0,
                record_count: 0,
            }),
        })
    }

    pub fn config(&self) -> &AofConfig {
        &self.config
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    fn lock_state(&self) -> AofResult<MutexGuard<'_, AofState>> {
        self.state
            .lock()
            .map_err(|_| AofError::internal("aof state poisoned"))
    }

    fn ensure_active_segment(
        &self,
        state: &mut AofState,
        created_at: i64,
    ) -> AofResult<Arc<Segment>> {
        if let Some(segment) = state.tail.clone() {
            return Ok(segment);
        }

        let segment_index = state.next_segment_index;
        let segment_id = SegmentId::new(segment_index as u64);
        let base_offset = state.next_offset;
        let base_record_count = state.record_count;
        let file_name = SegmentFileName::format(segment_id, base_offset, created_at);
        let path = self.layout.segment_path(&file_name);

        let segment = Arc::new(Segment::create_active(
            segment_id,
            base_offset,
            base_record_count,
            created_at,
            &self.config,
            path.as_path(),
        )?);

        state.next_segment_index = segment_index.saturating_add(1);
        state.tail = Some(segment.clone());
        state.segments.push(segment.clone());
        Ok(segment)
    }

    pub fn append_record(&self, payload: &[u8]) -> AofResult<RecordId> {
        if payload.is_empty() {
            return Err(AofError::InvalidState("record payload is empty".to_string()));
        }

        let now = Utc::now();
        let timestamp = now
            .timestamp_nanos_opt()
            .unwrap_or_else(|| now.timestamp() * 1_000_000_000);
        let timestamp_u64 = if timestamp < 0 { 0 } else { timestamp as u64 };

        let mut state = self.lock_state()?;
        let mut segment = self.ensure_active_segment(&mut state, timestamp)?;

        loop {
            match segment.append_record(payload, timestamp_u64) {
                Ok(result) => {
                    let record_id = RecordId::from_parts(segment.id().as_u32(), result.segment_offset);
                    state.next_offset = result.last_offset;
                    state.record_count += 1;

                    if result.is_full {
                        state.tail = None;
                    } else {
                        state.tail = Some(segment.clone());
                    }

                    return Ok(record_id);
                }
                Err(AofError::SegmentFull(_)) => {
                    state.tail = None;
                    segment = self.ensure_active_segment(&mut state, timestamp)?;
                }
                Err(err) => return Err(err),
            }
        }
    }
}

#[cfg(test)]
mod tests {}
