//! Segment metadata types for AOF segment tracking.
//!
//! This module contains the essential data structures for tracking segments
//! in the AOF segment index. The actual segment management is now handled
//! directly by the AOF implementation.

use crate::aof::record::SegmentMetadata;
use serde::{Deserialize, Serialize};

/// Status of a segment in the storage hierarchy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SegmentStatus {
    /// Currently being written to (tail segment)
    Active,
    /// Finalized and available locally
    Finalized,
    /// Archived to remote storage but cached locally
    ArchivedCached,
    /// Only available in remote storage
    ArchivedRemote,
}

impl SegmentStatus {
    /// Check if segment is archived (either cached or remote-only)
    pub fn is_archived(&self) -> bool {
        matches!(
            self,
            SegmentStatus::ArchivedCached | SegmentStatus::ArchivedRemote
        )
    }
}

/// Complete segment entry tracking all storage tiers and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentEntry {
    /// Base ID of the segment
    pub base_id: u64,
    /// Last record ID in the segment
    pub last_id: u64,
    /// Number of records in the segment
    pub record_count: u64,

    /// Timestamp when segment was created
    pub created_at: u64,
    /// Timestamp when segment was finalized (no more writes)
    pub finalized_at: Option<u64>,
    /// Timestamp when segment was archived to remote storage
    pub archived_at: Option<u64>,

    /// CRC64-NVME checksum of uncompressed segment data
    pub uncompressed_checksum: u64,
    /// CRC64-NVME checksum of compressed segment data (if compressed)
    pub compressed_checksum: Option<u64>,

    /// Size of original uncompressed segment
    pub original_size: u64,
    /// Size of compressed segment (if compressed)
    pub compressed_size: Option<u64>,
    /// Compression ratio achieved (compressed/original)
    pub compression_ratio: Option<f32>,

    /// Current status in storage hierarchy
    pub status: SegmentStatus,
    /// Local filesystem path (if available locally)
    pub local_path: Option<String>,
    /// Remote archive path/key (if archived)
    pub archive_key: Option<String>,
}

impl SegmentEntry {
    /// Create new segment entry for active segment
    pub fn new_active(
        metadata: &SegmentMetadata,
        local_path: String,
        uncompressed_checksum: u64,
    ) -> Self {
        Self {
            base_id: metadata.base_id,
            last_id: metadata.last_id,
            record_count: metadata.record_count,
            created_at: metadata.created_at,
            finalized_at: None,
            archived_at: None,
            uncompressed_checksum,
            compressed_checksum: None,
            original_size: metadata.size,
            compressed_size: None,
            compression_ratio: None,
            status: SegmentStatus::Active,
            local_path: Some(local_path),
            archive_key: None,
        }
    }

    /// Create new segment entry for finalized segment
    pub fn new_finalized(
        metadata: &SegmentMetadata,
        local_path: String,
        uncompressed_checksum: u64,
        finalized_at: u64,
    ) -> Self {
        Self {
            base_id: metadata.base_id,
            last_id: metadata.last_id,
            record_count: metadata.record_count,
            created_at: metadata.created_at,
            finalized_at: Some(finalized_at),
            archived_at: None,
            uncompressed_checksum,
            compressed_checksum: None,
            original_size: metadata.size,
            compressed_size: None,
            compression_ratio: None,
            status: SegmentStatus::Finalized,
            local_path: Some(local_path),
            archive_key: None,
        }
    }

    /// Check if segment is archived (either cached or remote-only)
    pub fn is_archived(&self) -> bool {
        self.status.is_archived()
    }

    /// Check if this segment contains the given record ID
    pub fn contains_record(&self, record_id: u64) -> bool {
        record_id >= self.base_id && record_id <= self.last_id
    }

    /// Check if this segment overlaps with the given timestamp
    pub fn overlaps_timestamp(&self, timestamp: u64) -> bool {
        // Check if the timestamp falls within the segment's time range
        if timestamp >= self.created_at {
            // If segment is finalized, check if timestamp is before finalization
            if let Some(finalized_at) = self.finalized_at {
                timestamp <= finalized_at
            } else {
                // Segment is still active, so it contains the timestamp
                true
            }
        } else {
            false
        }
    }

    /// Finalize the segment (mark as no longer accepting writes)
    pub fn finalize(&mut self, finalized_at: u64) {
        self.finalized_at = Some(finalized_at);
        if self.status == SegmentStatus::Active {
            self.status = SegmentStatus::Finalized;
        }
    }

    /// Archive the segment to remote storage
    pub fn archive(
        &mut self,
        archive_key: String,
        archived_at: u64,
        compressed_size: Option<u64>,
        compressed_checksum: Option<u64>,
    ) {
        self.archive_key = Some(archive_key);
        self.archived_at = Some(archived_at);
        self.compressed_size = compressed_size;
        self.compressed_checksum = compressed_checksum;

        if let Some(compressed) = compressed_size {
            self.compression_ratio = Some(compressed as f32 / self.original_size as f32);
        }

        self.status = SegmentStatus::ArchivedCached;
    }
}
