use std::collections::{HashMap, hash_map::Entry};

use crate::error::AofResult;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::config::SegmentId;

use super::tier0::InstanceId;

/// Tracks durability state for a specific segment.
///
/// Records how many bytes have been requested for persistence
/// versus how many bytes are actually durable (flushed to storage).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DurabilityEntry {
    /// Total bytes requested to be durable
    pub requested_bytes: u32,
    /// Bytes confirmed as durable (flushed)
    pub durable_bytes: u32,
}

/// Tracks durability state across all segments for an AOF instance.
///
/// Maintains a cursor of durability progress, tracking which bytes have
/// been requested for persistence and which have been confirmed as durable.
/// Provides atomic updates with rollback capability for persistence failures.
///
/// ## Thread Safety
///
/// All operations are thread-safe using internal locking. Multiple threads
/// can concurrently update durability state for different segments.
///
/// ## Durability Guarantees
///
/// - `requested_bytes`: Monotonically increasing, tracks flush requests
/// - `durable_bytes`: Never exceeds requested_bytes, tracks confirmed flushes
/// - Updates are atomic with rollback on persistence failures
pub struct DurabilityCursor {
    /// Instance identifier for logging and debugging
    instance_id: InstanceId,
    /// Protected durability state for all segments
    state: Mutex<HashMap<SegmentId, DurabilityEntry>>,
}

impl DurabilityCursor {
    pub fn new(instance_id: InstanceId) -> Self {
        debug!(
            instance = instance_id.get(),
            "initialized durability cursor"
        );
        Self {
            instance_id,
            state: Mutex::new(HashMap::new()),
        }
    }

    pub fn seed_entry(&self, segment_id: SegmentId, requested_bytes: u32, durable_bytes: u32) {
        let capped = durable_bytes.min(requested_bytes);
        let mut state = self.state.lock();
        state.insert(
            segment_id,
            DurabilityEntry {
                requested_bytes,
                durable_bytes: capped,
            },
        );
        debug!(
            instance = self.instance_id.get(),
            segment = segment_id.as_u64(),
            requested = requested_bytes,
            durable = capped,
            "seeded durability cursor entry"
        );
    }

    pub fn record_request(&self, segment_id: SegmentId, requested_bytes: u32) {
        let mut state = self.state.lock();
        let entry = state
            .entry(segment_id)
            .or_insert_with(DurabilityEntry::default);
        if requested_bytes > entry.requested_bytes {
            entry.requested_bytes = requested_bytes;
        }
    }

    pub fn record_flush(&self, segment_id: SegmentId, requested_bytes: u32, durable_bytes: u32) {
        let mut state = self.state.lock();
        let entry = state
            .entry(segment_id)
            .or_insert_with(DurabilityEntry::default);
        if requested_bytes > entry.requested_bytes {
            entry.requested_bytes = requested_bytes;
        }
        let capped = durable_bytes.min(entry.requested_bytes);
        if capped > entry.durable_bytes {
            entry.durable_bytes = capped;
        }
    }

    pub(crate) fn update_flush_with_snapshot<F>(
        &self,
        segment_id: SegmentId,
        requested_bytes: u32,
        durable_bytes: u32,
        mut persist: F,
    ) -> AofResult<()>
    where
        F: FnMut(&[(SegmentId, DurabilityEntry)]) -> AofResult<()>,
    {
        let (snapshot, previous, existed_before) = {
            let mut state = self.state.lock();
            let entry_slot = state.entry(segment_id);
            let (prev, existed_before) = match entry_slot {
                Entry::Occupied(mut occupied) => {
                    let prev = *occupied.get();
                    let mut updated = prev;
                    if requested_bytes > updated.requested_bytes {
                        updated.requested_bytes = requested_bytes;
                    }
                    let capped = durable_bytes.min(updated.requested_bytes);
                    if capped > updated.durable_bytes {
                        updated.durable_bytes = capped;
                    }
                    occupied.insert(updated);
                    (prev, true)
                }
                Entry::Vacant(vacant) => {
                    let mut updated = DurabilityEntry::default();
                    if requested_bytes > updated.requested_bytes {
                        updated.requested_bytes = requested_bytes;
                    }
                    updated.durable_bytes = durable_bytes.min(updated.requested_bytes);
                    vacant.insert(updated);
                    (DurabilityEntry::default(), false)
                }
            };
            let snapshot = state
                .iter()
                .map(|(segment_id, entry)| (*segment_id, *entry))
                .collect::<Vec<_>>();
            (snapshot, prev, existed_before)
        };
        if let Err(err) = persist(&snapshot) {
            let mut state = self.state.lock();
            match state.entry(segment_id) {
                Entry::Occupied(mut occupied) => {
                    if existed_before {
                        occupied.insert(previous);
                    } else {
                        occupied.remove();
                    }
                }
                Entry::Vacant(vacant) => {
                    if existed_before {
                        vacant.insert(previous);
                    }
                }
            }
            return Err(err);
        }
        Ok(())
    }

    pub fn entry(&self, segment_id: SegmentId) -> Option<DurabilityEntry> {
        self.state.lock().get(&segment_id).copied()
    }

    pub fn snapshot(&self) -> Vec<(SegmentId, DurabilityEntry)> {
        self.state
            .lock()
            .iter()
            .map(|(segment_id, entry)| (*segment_id, *entry))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_entry_sets_requested_and_durable_bytes() {
        let cursor = DurabilityCursor::new(InstanceId::new(7));
        let segment = SegmentId::new(3);
        cursor.seed_entry(segment, 512, 256);
        let entry = cursor.entry(segment).expect("entry after seed");
        assert_eq!(entry.requested_bytes, 512);
        assert_eq!(entry.durable_bytes, 256);
    }
    #[test]
    fn update_flush_with_snapshot_reverts_on_failure() {
        use crate::error::AofError;
        let cursor = DurabilityCursor::new(InstanceId::new(5));
        let segment = SegmentId::new(11);
        let result = cursor.update_flush_with_snapshot(segment, 256, 128, |_| {
            Err(AofError::other("persist failure"))
        });
        assert!(result.is_err());
        assert!(cursor.entry(segment).is_none());
    }

    #[test]
    fn record_updates_preserve_monotonicity() {
        let cursor = DurabilityCursor::new(InstanceId::new(9));
        let segment = SegmentId::new(11);
        cursor.seed_entry(segment, 256, 128);
        cursor.record_request(segment, 128);
        let seeded = cursor.entry(segment).expect("seeded entry");
        assert_eq!(seeded.requested_bytes, 256);
        assert_eq!(seeded.durable_bytes, 128);

        cursor.record_request(segment, 1024);
        cursor.record_flush(segment, 2048, 4096);
        let flushed = cursor.entry(segment).expect("flushed entry");
        assert_eq!(flushed.requested_bytes, 2048);
        assert_eq!(flushed.durable_bytes, 2048);
    }
}
