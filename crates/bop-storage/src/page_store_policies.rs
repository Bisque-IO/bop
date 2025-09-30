//! Page store policies for checkpoint coordination and cache management.
//!
//! This module implements T10 from the checkpointing plan: PageStore lease
//! wiring and cache population policies.
//!
//! # Architecture
//!
//! - **CheckpointObserver**: Tracks checkpoint lifecycle and registers leases
//! - **FetchOptions**: Controls cache behavior during page fetches
//! - **PinningPolicy**: Manages page pinning to prevent eviction

use std::sync::Arc;

use crate::checkpoint::LeaseMap;
use crate::manifest::{ChunkId, DbId, Generation, JobId};
use crate::page_cache::{PageCacheKey, PageCacheObserver};

/// Options for fetching pages from storage.
///
/// # T10c: Cache Policies
#[derive(Debug, Clone)]
pub struct FetchOptions {
    /// Pin the page in cache after fetch (prevents eviction).
    pub pin_in_cache: bool,

    /// Fetch hint for prefetching adjacent pages.
    pub prefetch_hint: PrefetchHint,

    /// Bypass cache and fetch directly from remote (cold scan).
    pub bypass_cache: bool,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            pin_in_cache: false,
            prefetch_hint: PrefetchHint::None,
            bypass_cache: false,
        }
    }
}

/// Hint for prefetching adjacent pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefetchHint {
    /// No prefetching.
    None,

    /// Prefetch forward (sequential scan).
    Forward { pages: u32 },

    /// Prefetch backward (reverse scan).
    Backward { pages: u32 },

    /// Prefetch both directions (random access).
    Bidirectional { pages: u32 },
}

/// Observer for checkpoint events on the PageCache.
///
/// # T10a: PageCache Observer Checkpoint Hooks
///
/// Tracks checkpoint lifecycle and coordinates with LeaseMap.
pub struct CheckpointObserver {
    db_id: DbId,
    lease_map: Arc<LeaseMap>,
}

impl CheckpointObserver {
    /// Creates a new checkpoint observer.
    pub fn new(db_id: DbId, lease_map: Arc<LeaseMap>) -> Self {
        Self { db_id, lease_map }
    }

    /// Called when a checkpoint starts.
    ///
    /// # T10a: on_checkpoint_start
    ///
    /// Registers leases for all chunks being checkpointed.
    pub fn on_checkpoint_start(
        &self,
        job_id: JobId,
        chunk_ids: &[ChunkId],
        generation: Generation,
    ) {
        for &chunk_id in chunk_ids {
            let _ = self.lease_map.register(
                chunk_id,
                generation,
                job_id,
                std::time::Duration::from_secs(600),
            );
        }
    }

    /// Called when a checkpoint completes successfully.
    ///
    /// # T10a: on_checkpoint_end
    ///
    /// Releases leases for all checkpointed chunks.
    pub fn on_checkpoint_end(&self, chunk_ids: &[ChunkId]) {
        for &chunk_id in chunk_ids {
            self.lease_map.release(chunk_id);
        }
    }

    /// Called when a checkpoint fails or is cancelled.
    pub fn on_checkpoint_failed(&self, chunk_ids: &[ChunkId]) {
        // Release leases
        self.on_checkpoint_end(chunk_ids);
    }

    /// Returns the database ID.
    pub fn db_id(&self) -> DbId {
        self.db_id
    }

    /// Returns a reference to the lease map.
    pub fn lease_map(&self) -> &LeaseMap {
        &self.lease_map
    }
}

impl PageCacheObserver<PageCacheKey> for CheckpointObserver {
    fn on_insert(&self, _key: &PageCacheKey, _bytes: &[u8]) {
        // Could track which pages are cached for checkpoint planning
    }

    fn on_evict(&self, _key: &PageCacheKey, _bytes: &[u8]) {
        // Could update lease map if evicting pages being checkpointed
    }

    /// T10a: Checkpoint start hook implementation
    fn on_checkpoint_start(&self, _job_id: u64, _generation: u64) {
        // Hook is called but lease registration happens per-chunk in orchestrator
        // This could be used for cache-wide policies like disabling eviction
    }

    /// T10a: Checkpoint end hook implementation
    fn on_checkpoint_end(&self, _job_id: u64, success: bool) {
        // Could adjust cache policies based on checkpoint success/failure
        if !success {
            // On failure, could trigger cache cleanup or reset policies
        }
    }
}

/// Pin count tracking for pages.
///
/// # T10c: Pin Management
///
/// Tracks pin counts to prevent eviction of actively-used pages.
pub struct PinTracker {
    db_id: DbId,
    pins: std::sync::Arc<dashmap::DashMap<u32, u32>>, // page_no -> pin_count
}

impl PinTracker {
    /// Creates a new pin tracker for a database.
    pub fn new(db_id: DbId) -> Self {
        Self {
            db_id,
            pins: Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Pins a page (increments pin count).
    pub fn pin(&self, page_no: u32) -> u32 {
        let mut entry = self.pins.entry(page_no).or_insert(0);
        *entry += 1;
        *entry
    }

    /// Unpins a page (decrements pin count).
    pub fn unpin(&self, page_no: u32) -> u32 {
        if let Some(mut entry) = self.pins.get_mut(&page_no) {
            if *entry > 0 {
                *entry -= 1;
            }
            *entry
        } else {
            0
        }
    }

    /// Returns the pin count for a page.
    pub fn pin_count(&self, page_no: u32) -> u32 {
        self.pins.get(&page_no).map(|e| *e).unwrap_or(0)
    }

    /// Checks if a page is pinned.
    pub fn is_pinned(&self, page_no: u32) -> bool {
        self.pin_count(page_no) > 0
    }

    /// Returns the database ID.
    pub fn db_id(&self) -> DbId {
        self.db_id
    }

    /// Returns the total number of pinned pages.
    pub fn pinned_page_count(&self) -> usize {
        self.pins.iter().filter(|e| *e.value() > 0).count()
    }
}

/// RAII guard for page pinning.
///
/// Automatically unpins the page when dropped.
pub struct PinGuard {
    tracker: Arc<PinTracker>,
    page_no: u32,
}

impl PinGuard {
    /// Creates a new pin guard.
    pub fn new(tracker: Arc<PinTracker>, page_no: u32) -> Self {
        tracker.pin(page_no);
        Self { tracker, page_no }
    }

    /// Returns the page number.
    pub fn page_no(&self) -> u32 {
        self.page_no
    }
}

impl Drop for PinGuard {
    fn drop(&mut self) {
        self.tracker.unpin(self.page_no);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_options_default() {
        let options = FetchOptions::default();
        assert!(!options.pin_in_cache);
        assert!(!options.bypass_cache);
        assert_eq!(options.prefetch_hint, PrefetchHint::None);
    }

    #[test]
    fn checkpoint_observer_registers_leases() {
        let lease_map = Arc::new(LeaseMap::new());
        let observer = CheckpointObserver::new(1, lease_map.clone());

        // Start checkpoint
        observer.on_checkpoint_start(100, &[1, 2, 3], 5);

        // Check leases are registered
        assert_eq!(lease_map.active_count(), 3);

        // End checkpoint
        observer.on_checkpoint_end(&[1, 2, 3]);

        // Check leases are released
        assert_eq!(lease_map.active_count(), 0);
    }

    #[test]
    fn pin_tracker_manages_pins() {
        let tracker = Arc::new(PinTracker::new(1));

        assert_eq!(tracker.pin_count(100), 0);
        assert!(!tracker.is_pinned(100));

        tracker.pin(100);
        assert_eq!(tracker.pin_count(100), 1);
        assert!(tracker.is_pinned(100));

        tracker.pin(100);
        assert_eq!(tracker.pin_count(100), 2);

        tracker.unpin(100);
        assert_eq!(tracker.pin_count(100), 1);
        assert!(tracker.is_pinned(100));

        tracker.unpin(100);
        assert_eq!(tracker.pin_count(100), 0);
        assert!(!tracker.is_pinned(100));
    }

    #[test]
    fn pin_guard_auto_unpins() {
        let tracker = Arc::new(PinTracker::new(1));

        {
            let _guard = PinGuard::new(tracker.clone(), 200);
            assert_eq!(tracker.pin_count(200), 1);
        }

        // Guard dropped, should unpin
        assert_eq!(tracker.pin_count(200), 0);
    }

    #[test]
    fn checkpoint_observer_releases_leases() {
        let lease_map = Arc::new(LeaseMap::new());
        let observer = CheckpointObserver::new(1, lease_map.clone());

        let chunk_ids = vec![40, 50, 60];
        observer.on_checkpoint_start(200, &chunk_ids, 10);

        // Verify leases exist
        assert_eq!(lease_map.active_count(), chunk_ids.len());

        observer.on_checkpoint_end(&chunk_ids);

        // Leases should be released
        assert_eq!(lease_map.active_count(), 0);
    }
}
