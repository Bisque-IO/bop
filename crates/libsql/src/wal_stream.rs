//! WAL Stream Protocol
//!
//! Efficient streaming of WAL frames from master to followers.
//! Frames are pushed as they're committed, not pulled on demand.
//!
//! # Protocol
//!
//! ```text
//! Batch := Header Frame*
//!
//! Header (16 bytes):
//!   db_id:        u64    - Database ID
//!   start_frame:  u64    - First frame number in batch
//!                          (low 48 bits = frame_no, high 16 bits = frame_count)
//!
//! Frame (8 + page_size bytes):
//!   page_no:      u32    - Page number
//!   db_size:      u32    - DB size in pages (non-zero = commit frame)
//!   data:         [u8]   - Page data
//! ```
//!
//! # Flow
//!
//! ```text
//! Master:
//!   commit -> encode_batch() -> send to followers
//!
//! Follower:
//!   receive -> decode_batch() -> insert into gap buffer
//!                             -> VirtualWal can read immediately
//! ```

use crate::raft_log::BranchRef;
use crate::virtual_wal::WalFrame;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Maximum frames per batch (fits in 16 bits with some headroom)
pub const MAX_FRAMES_PER_BATCH: usize = 4096;

/// Batch header size
pub const BATCH_HEADER_SIZE: usize = 16;

/// Encode a batch of frames for streaming.
///
/// Returns the encoded bytes. Caller is responsible for framing/transport.
#[inline]
pub fn encode_batch(
    db_id: u64,
    start_frame: u64,
    frames: &[WalFrame],
) -> Vec<u8> {
    debug_assert!(frames.len() <= MAX_FRAMES_PER_BATCH);
    debug_assert!(start_frame <= 0x0000_FFFF_FFFF_FFFF); // 48 bits max

    let frame_count = frames.len() as u64;
    let page_size = frames.first().map(|f| f.data.len()).unwrap_or(0);

    // Pack frame_count into high 16 bits, start_frame in low 48 bits
    let packed_frame_info = (frame_count << 48) | (start_frame & 0x0000_FFFF_FFFF_FFFF);

    let total_size = BATCH_HEADER_SIZE + frames.len() * (8 + page_size);
    let mut buf = Vec::with_capacity(total_size);

    // Header
    buf.extend_from_slice(&db_id.to_le_bytes());
    buf.extend_from_slice(&packed_frame_info.to_le_bytes());

    // Frames
    for frame in frames {
        buf.extend_from_slice(&frame.page_no.to_le_bytes());
        buf.extend_from_slice(&frame.db_size.to_le_bytes());
        buf.extend_from_slice(&frame.data);
    }

    buf
}

/// Decoded batch header
#[derive(Debug, Clone, Copy)]
pub struct BatchHeader {
    pub db_id: u64,
    pub start_frame: u64,
    pub frame_count: u16,
}

/// Decode batch header from bytes.
///
/// Returns header and remaining bytes for frame parsing.
#[inline]
pub fn decode_header(buf: &[u8]) -> Option<(BatchHeader, &[u8])> {
    if buf.len() < BATCH_HEADER_SIZE {
        return None;
    }

    let db_id = u64::from_le_bytes(buf[0..8].try_into().unwrap());
    let packed = u64::from_le_bytes(buf[8..16].try_into().unwrap());

    let frame_count = (packed >> 48) as u16;
    let start_frame = packed & 0x0000_FFFF_FFFF_FFFF;

    Some((
        BatchHeader {
            db_id,
            start_frame,
            frame_count,
        },
        &buf[BATCH_HEADER_SIZE..],
    ))
}

/// Iterator over frames in a batch
pub struct FrameIterator<'a> {
    data: &'a [u8],
    page_size: usize,
    remaining: u16,
    current_frame: u64,
}

impl<'a> FrameIterator<'a> {
    /// Create iterator from batch data (after header)
    #[inline]
    pub fn new(data: &'a [u8], page_size: usize, frame_count: u16, start_frame: u64) -> Self {
        Self {
            data,
            page_size,
            remaining: frame_count,
            current_frame: start_frame,
        }
    }
}

impl<'a> Iterator for FrameIterator<'a> {
    type Item = (u64, WalFrame);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let frame_size = 8 + self.page_size;
        if self.data.len() < frame_size {
            return None;
        }

        let page_no = u32::from_le_bytes(self.data[0..4].try_into().unwrap());
        let db_size = u32::from_le_bytes(self.data[4..8].try_into().unwrap());
        let data = self.data[8..8 + self.page_size].to_vec();

        let frame_no = self.current_frame;
        self.current_frame += 1;
        self.remaining -= 1;
        self.data = &self.data[frame_size..];

        Some((
            frame_no,
            WalFrame {
                page_no,
                db_size,
                data,
            },
        ))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining as usize, Some(self.remaining as usize))
    }
}

impl ExactSizeIterator for FrameIterator<'_> {}

/// Decode a complete batch into frames.
///
/// Returns (db_id, start_frame, frames) or None if invalid.
#[inline]
pub fn decode_batch(buf: &[u8], page_size: usize) -> Option<(u64, u64, Vec<(u64, WalFrame)>)> {
    let (header, data) = decode_header(buf)?;
    let frames: Vec<_> =
        FrameIterator::new(data, page_size, header.frame_count, header.start_frame).collect();

    if frames.len() != header.frame_count as usize {
        return None;
    }

    Some((header.db_id, header.start_frame, frames))
}

/// Gap buffer for follower WAL state.
///
/// Stores frames received from master stream that aren't yet in remote storage.
/// Thread-safe for concurrent reads (SQLite) and writes (stream receiver).
pub struct GapBuffer {
    branch: BranchRef,
    page_size: u32,
    /// Frame data: frame_no -> (page_no, data)
    frames: dashmap::DashMap<u64, Arc<WalFrame>>,
    /// Page index: page_no -> latest frame_no
    page_index: dashmap::DashMap<u32, u64>,
    /// Highest frame number received
    max_frame: AtomicU64,
    /// Lowest frame number still in buffer (frames below this are in remote)
    min_frame: AtomicU64,
}

impl GapBuffer {
    /// Create a new gap buffer
    pub fn new(branch: BranchRef, page_size: u32) -> Self {
        Self {
            branch,
            page_size,
            frames: dashmap::DashMap::new(),
            page_index: dashmap::DashMap::new(),
            max_frame: AtomicU64::new(0),
            min_frame: AtomicU64::new(u64::MAX),
        }
    }

    /// Insert frames from a decoded batch
    #[inline]
    pub fn insert_batch(&self, frames: impl IntoIterator<Item = (u64, WalFrame)>) {
        for (frame_no, frame) in frames {
            let page_no = frame.page_no;

            // Update page index (only if this is newer)
            self.page_index
                .entry(page_no)
                .and_modify(|existing| {
                    if frame_no > *existing {
                        *existing = frame_no;
                    }
                })
                .or_insert(frame_no);

            // Store frame
            self.frames.insert(frame_no, Arc::new(frame));

            // Update bounds
            self.max_frame.fetch_max(frame_no, Ordering::Release);
            self.min_frame.fetch_min(frame_no, Ordering::Release);
        }
    }

    /// Find the latest frame containing a page
    #[inline]
    pub fn find_frame(&self, page_no: u32) -> Option<u64> {
        self.page_index.get(&page_no).map(|r| *r)
    }

    /// Read frame data
    #[inline]
    pub fn read_frame(&self, frame_no: u64) -> Option<Arc<WalFrame>> {
        self.frames.get(&frame_no).map(|r| r.clone())
    }

    /// Get the highest frame number in the buffer
    #[inline]
    pub fn max_frame(&self) -> u64 {
        self.max_frame.load(Ordering::Acquire)
    }

    /// Get the lowest frame number in the buffer
    #[inline]
    pub fn min_frame(&self) -> u64 {
        let min = self.min_frame.load(Ordering::Acquire);
        if min == u64::MAX { 0 } else { min }
    }

    /// Trim frames that are now in remote storage
    ///
    /// Call this after remote storage confirms durability up to `durable_frame`.
    pub fn trim_to(&self, durable_frame: u64) {
        let current_min = self.min_frame.load(Ordering::Acquire);
        if current_min > durable_frame || current_min == u64::MAX {
            return;
        }

        // Remove frames <= durable_frame
        let mut pages_to_check = Vec::new();
        for frame_no in current_min..=durable_frame {
            if let Some((_, frame)) = self.frames.remove(&frame_no) {
                pages_to_check.push(frame.page_no);
            }
        }

        // Update page index - remove entries pointing to trimmed frames
        for page_no in pages_to_check {
            self.page_index.remove_if(&page_no, |_, frame_no| *frame_no <= durable_frame);
        }

        // Update min_frame
        self.min_frame.store(durable_frame + 1, Ordering::Release);
    }

    /// Get branch reference
    pub fn branch(&self) -> BranchRef {
        self.branch
    }

    /// Get page size
    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    /// Number of frames currently buffered
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

/// Stream position for reconnection
#[derive(Debug, Clone, Copy, Default)]
pub struct StreamPosition {
    /// Last frame number received
    pub last_frame: u64,
    /// Last frame number confirmed durable in remote
    pub remote_durable: u64,
}

impl StreamPosition {
    /// Create position for initial connection (start from remote durable point)
    pub fn initial(remote_durable: u64) -> Self {
        Self {
            last_frame: remote_durable,
            remote_durable,
        }
    }

    /// The gap size (frames we have that aren't in remote yet)
    pub fn gap_size(&self) -> u64 {
        self.last_frame.saturating_sub(self.remote_durable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_batch() {
        let frames = vec![
            WalFrame {
                page_no: 1,
                db_size: 0,
                data: vec![0xAA; 64],
            },
            WalFrame {
                page_no: 2,
                db_size: 0,
                data: vec![0xBB; 64],
            },
            WalFrame {
                page_no: 3,
                db_size: 3, // commit frame
                data: vec![0xCC; 64],
            },
        ];

        let encoded = encode_batch(123, 100, &frames);

        // Decode header
        let (header, data) = decode_header(&encoded).unwrap();
        assert_eq!(header.db_id, 123);
        assert_eq!(header.start_frame, 100);
        assert_eq!(header.frame_count, 3);

        // Decode full batch
        let (db_id, start_frame, decoded) = decode_batch(&encoded, 64).unwrap();
        assert_eq!(db_id, 123);
        assert_eq!(start_frame, 100);
        assert_eq!(decoded.len(), 3);

        assert_eq!(decoded[0].0, 100);
        assert_eq!(decoded[0].1.page_no, 1);
        assert_eq!(decoded[1].0, 101);
        assert_eq!(decoded[1].1.page_no, 2);
        assert_eq!(decoded[2].0, 102);
        assert_eq!(decoded[2].1.page_no, 3);
        assert_eq!(decoded[2].1.db_size, 3);
    }

    #[test]
    fn test_gap_buffer() {
        let branch = BranchRef::new(1, 1);
        let buffer = GapBuffer::new(branch, 64);

        // Insert some frames
        buffer.insert_batch(vec![
            (10, WalFrame { page_no: 1, db_size: 0, data: vec![0; 64] }),
            (11, WalFrame { page_no: 2, db_size: 0, data: vec![0; 64] }),
            (12, WalFrame { page_no: 1, db_size: 0, data: vec![1; 64] }), // page 1 updated
        ]);

        assert_eq!(buffer.min_frame(), 10);
        assert_eq!(buffer.max_frame(), 12);
        assert_eq!(buffer.len(), 3);

        // Page 1 should point to frame 12 (latest)
        assert_eq!(buffer.find_frame(1), Some(12));
        assert_eq!(buffer.find_frame(2), Some(11));

        // Trim up to frame 10
        buffer.trim_to(10);
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.min_frame(), 11);

        // Page 1 should still be found (frame 12 still exists)
        assert_eq!(buffer.find_frame(1), Some(12));
    }

    #[test]
    fn test_frame_iterator_exact_size() {
        let data = vec![
            // Frame 1
            1u8, 0, 0, 0, // page_no
            0, 0, 0, 0,   // db_size
            0xAA, 0xAA,   // data (2 bytes)
            // Frame 2
            2, 0, 0, 0,
            0, 0, 0, 0,
            0xBB, 0xBB,
        ];

        let iter = FrameIterator::new(&data, 2, 2, 100);
        assert_eq!(iter.len(), 2);

        let frames: Vec<_> = iter.collect();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].0, 100);
        assert_eq!(frames[1].0, 101);
    }
}
