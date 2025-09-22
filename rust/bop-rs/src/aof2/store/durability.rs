//! Volatile durability cursor tracking per-segment flush progress.
use std::collections::HashMap;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::aof2::config::SegmentId;

use super::tier0::InstanceId;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DurabilityEntry {
    pub requested_bytes: u32,
    pub durable_bytes: u32,
}

pub struct DurabilityCursor {
    instance_id: InstanceId,
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
        entry.durable_bytes = durable_bytes.min(entry.requested_bytes);
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
