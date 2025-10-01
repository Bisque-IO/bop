use std::collections::VecDeque;
use std::fmt;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use tracing::{debug, info, instrument, trace};

static NEXT_CACHE_OBJECT_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate a globally unique identifier that callers can use to namespace cache entries.
pub fn allocate_cache_object_id() -> u64 {
    NEXT_CACHE_OBJECT_ID.fetch_add(1, Ordering::SeqCst)
}

/// Logical namespace associated with cached pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageCacheNamespace {
    Manifest,
    LibsqlWal,
    StoragePod,
}

/// Unified key used for caching pages across the storage stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageCacheKey {
    pub namespace: PageCacheNamespace,
    pub object_id: u64,
    pub page_no: u64,
}

impl PageCacheKey {
    pub fn manifest(object_id: u64, page_no: u64) -> Self {
        Self {
            namespace: PageCacheNamespace::Manifest,
            object_id,
            page_no,
        }
    }

    pub fn libsql_wal(object_id: u64, page_no: u64) -> Self {
        Self {
            namespace: PageCacheNamespace::LibsqlWal,
            object_id,
            page_no,
        }
    }
    pub fn storage_pod(object_id: u64, page_no: u64) -> Self {
        Self {
            namespace: PageCacheNamespace::StoragePod,
            object_id,
            page_no,
        }
    }
}

/// Configuration parameters for the global page cache.
#[derive(Debug, Clone)]
pub struct PageCacheConfig {
    /// Optional ceiling for cached payloads. When `None`, the cache grows without eviction.
    pub capacity_bytes: Option<usize>,
}

impl Default for PageCacheConfig {
    fn default() -> Self {
        Self {
            capacity_bytes: None,
        }
    }
}

#[derive(Debug)]
struct EvictionState<K> {
    total_bytes: usize,
    order: VecDeque<EvictionEntry<K>>,
}

impl<K> Default for EvictionState<K> {
    fn default() -> Self {
        Self {
            total_bytes: 0,
            order: VecDeque::new(),
        }
    }
}

#[derive(Debug)]
struct EvictionEntry<K> {
    key: K,
    stamp: u64,
}

/// Observer hooks invoked as cache events occur. Callers can use this to persist pages,
/// build warm caches, or collect external diagnostics.
///
/// # Re-entrancy
/// Callbacks must not synchronously call back into the cache while they are executing. The
/// cache defers observer invocations until internal bookkeeping locks are released, but
/// re-entrant access can still create deadlocks if observers attempt to acquire cache locks
/// held by other threads.
///
/// # T10a: Checkpoint Hooks
/// The on_checkpoint_start and on_checkpoint_end hooks allow the checkpoint executor
/// to coordinate with the PageCache, for example to pin pages or adjust eviction policies.
pub trait PageCacheObserver<K>: Send + Sync + 'static {
    fn on_insert(&self, _key: &K, _bytes: &[u8]) {}
    fn on_hit(&self, _key: &K) {}
    fn on_miss(&self, _key: &K) {}
    fn on_evict(&self, _key: &K, _bytes: &[u8]) {}

    /// Called when a checkpoint job starts processing chunks.
    ///
    /// # Parameters
    /// - `job_id`: Unique identifier for the checkpoint job
    /// - `generation`: The generation being written
    fn on_checkpoint_start(&self, _job_id: u64, _generation: u64) {}

    /// Called when a checkpoint job completes (success or failure).
    ///
    /// # Parameters
    /// - `job_id`: Unique identifier for the checkpoint job
    /// - `success`: Whether the checkpoint completed successfully
    fn on_checkpoint_end(&self, _job_id: u64, _success: bool) {}
}

/// A cached page frame backed by reference-counted bytes.
#[derive(Debug)]
pub struct PageFrame {
    bytes: Arc<[u8]>,
    last_access: AtomicU64,
}

impl PageFrame {
    fn new(bytes: Arc<[u8]>, stamp: u64) -> Self {
        Self {
            last_access: AtomicU64::new(stamp),
            bytes,
        }
    }

    /// Length of the backing payload.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Materialize a read-only view of the bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    fn touch(&self, stamp: u64) {
        self.last_access.store(stamp, Ordering::Relaxed);
    }

    fn last_access(&self) -> u64 {
        self.last_access.load(Ordering::Relaxed)
    }
}

/// Global, concurrent page cache backed by DashMap.
pub struct PageCache<K>
where
    K: Eq + Hash + Clone,
{
    config: PageCacheConfig,
    frames: DashMap<K, Arc<PageFrame>>,
    eviction: Mutex<EvictionState<K>>,
    clock: AtomicU64,
    observer: Option<Arc<dyn PageCacheObserver<K>>>,
    hits: AtomicU64,
    misses: AtomicU64,
    insertions: AtomicU64,
    evictions: AtomicU64,
}

impl<K> PageCache<K>
where
    K: Eq + Hash + Clone + 'static,
{
    /// Construct a cache using the provided configuration.
    #[instrument(skip(config), fields(capacity_bytes = ?config.capacity_bytes))]
    pub fn new(config: PageCacheConfig) -> Self {
        info!(capacity_bytes = ?config.capacity_bytes, "Creating page cache");
        Self::with_observer(config, None)
    }

    /// Construct a cache with the supplied configuration and observer hooks.
    #[instrument(skip(config, observer), fields(capacity_bytes = ?config.capacity_bytes, has_observer = observer.is_some()))]
    pub fn with_observer(
        config: PageCacheConfig,
        observer: Option<Arc<dyn PageCacheObserver<K>>>,
    ) -> Self {
        info!(
            capacity_bytes = ?config.capacity_bytes,
            has_observer = observer.is_some(),
            "Creating page cache with observer"
        );
        Self {
            config,
            frames: DashMap::new(),
            eviction: Mutex::new(EvictionState::default()),
            clock: AtomicU64::new(0),
            observer,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            insertions: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }

    /// Insert or replace a page in the cache.
    pub fn insert(&self, key: K, payload: Arc<[u8]>) -> Arc<PageFrame> {
        let stamp = self.next_stamp();
        let frame = Arc::new(PageFrame::new(payload, stamp));
        let frame_len = frame.len();

        let mut eviction = self
            .eviction
            .lock()
            .expect("page cache eviction state poisoned");

        let is_replacement = self.frames.contains_key(&key);
        if let Some(previous) = self.frames.insert(key.clone(), frame.clone()) {
            eviction.total_bytes = eviction
                .total_bytes
                .saturating_sub(previous.len())
                .saturating_add(frame_len);
            trace!(frame_len, old_len = previous.len(), "Replaced page in cache");
        } else {
            eviction.total_bytes = eviction.total_bytes.saturating_add(frame_len);
            trace!(frame_len, "Inserted new page into cache");
        }

        eviction.order.push_back(EvictionEntry {
            key: key.clone(),
            stamp,
        });
        drop(eviction);

        self.insertions.fetch_add(1, Ordering::Relaxed);
        if let Some(observer) = &self.observer {
            observer.on_insert(&key, frame.as_slice());
        }

        self.enforce_capacity();
        frame
    }

    /// Retrieve a page from the cache, refreshing its recency metadata.
    pub fn get(&self, key: &K) -> Option<Arc<PageFrame>> {
        let stamp = self.next_stamp();
        if let Some(guard) = self.frames.get(key) {
            let frame = guard.value().clone();
            frame.touch(stamp);
            drop(guard);

            if self.config.capacity_bytes.is_some() {
                let mut eviction = self
                    .eviction
                    .lock()
                    .expect("page cache eviction state poisoned");
                eviction.order.push_back(EvictionEntry {
                    key: key.clone(),
                    stamp,
                });
            }

            self.hits.fetch_add(1, Ordering::Relaxed);
            if let Some(observer) = &self.observer {
                observer.on_hit(key);
            }
            Some(frame)
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            if let Some(observer) = &self.observer {
                observer.on_miss(key);
            }
            None
        }
    }

    /// Remove a page from the cache.
    pub fn remove(&self, key: &K) -> Option<Arc<PageFrame>> {
        let mut eviction = self
            .eviction
            .lock()
            .expect("page cache eviction state poisoned");
        let removed = self.frames.remove(key);
        if let Some((_k, frame)) = &removed {
            eviction.total_bytes = eviction.total_bytes.saturating_sub(frame.len());
        }
        removed.map(|(_, frame)| frame)
    }

    /// Number of cached pages.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Total bytes currently accounted for in the cache.
    pub fn total_bytes(&self) -> usize {
        self.eviction
            .lock()
            .expect("page cache eviction state poisoned")
            .total_bytes
    }

    /// Clear all cached entries.
    pub fn clear(&self) {
        self.frames.clear();
        let mut eviction = self
            .eviction
            .lock()
            .expect("page cache eviction state poisoned");
        eviction.total_bytes = 0;
        eviction.order.clear();
    }

    fn enforce_capacity(&self) {
        let Some(limit) = self.config.capacity_bytes else {
            return;
        };

        let mut total_evicted = 0;
        loop {
            let (evicted, needs_more, current_bytes) = {
                let mut eviction = self
                    .eviction
                    .lock()
                    .expect("page cache eviction state poisoned");

                if eviction.total_bytes <= limit {
                    return;
                }

                let mut evicted: Option<(K, Arc<PageFrame>)> = None;

                while let Some(entry) = eviction.order.pop_front() {
                    if let Some(frame_guard) = self.frames.get(&entry.key) {
                        let last_access = frame_guard.value().last_access();
                        drop(frame_guard);

                        if last_access == entry.stamp {
                            if let Some((_k, removed_frame)) = self.frames.remove(&entry.key) {
                                eviction.total_bytes =
                                    eviction.total_bytes.saturating_sub(removed_frame.len());
                                self.evictions.fetch_add(1, Ordering::Relaxed);
                                evicted = Some((entry.key, removed_frame));
                                break;
                            }
                        }
                    }
                }

                let needs_more = eviction.total_bytes > limit && evicted.is_some();
                (evicted, needs_more, eviction.total_bytes)
            };

            if let Some((key, frame)) = evicted {
                total_evicted += 1;
                trace!(size = frame.len(), current_bytes, limit, "Evicted page from cache");
                if let Some(observer) = &self.observer {
                    observer.on_evict(&key, frame.as_slice());
                }
            } else {
                break;
            }

            if !needs_more {
                if total_evicted > 0 {
                    debug!(total_evicted, current_bytes, limit, "Cache capacity enforced");
                }
                break;
            }
        }
    }

    fn next_stamp(&self) -> u64 {
        self.clock.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Capture an instantaneous snapshot of cache metrics.
    pub fn metrics(&self) -> PageCacheMetricsSnapshot {
        PageCacheMetricsSnapshot {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            insertions: self.insertions.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageCacheMetricsSnapshot {
    pub hits: u64,
    pub misses: u64,
    pub insertions: u64,
    pub evictions: u64,
}

impl<K> fmt::Debug for PageCache<K>
where
    K: Eq + Hash + Clone + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PageCache")
            .field("capacity_bytes", &self.config.capacity_bytes)
            .field("len", &self.len())
            .field("total_bytes", &self.total_bytes())
            .field("hits", &self.hits.load(Ordering::Relaxed))
            .field("misses", &self.misses.load(Ordering::Relaxed))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, OnceLock, Weak};
    use std::thread;

    #[test]
    fn insert_and_get_roundtrip() {
        let cache = PageCache::new(PageCacheConfig {
            capacity_bytes: Some(4096),
        });
        let payload: Arc<[u8]> = Arc::from(vec![1, 2, 3, 4]);
        cache.insert("page-a", payload.clone());

        let frame = cache.get(&"page-a").expect("frame not found");
        assert_eq!(frame.as_slice(), payload.as_ref());
        assert!(cache.total_bytes() >= frame.len());
    }

    #[test]
    fn enforces_capacity_limit() {
        let cache = PageCache::new(PageCacheConfig {
            capacity_bytes: Some(256),
        });

        for idx in 0..4 {
            let payload: Arc<[u8]> = Arc::from(vec![idx as u8; 128]);
            cache.insert(idx, payload);
        }

        // Cache should have evicted at least one entry to stay within 256 bytes.
        assert!(cache.len() <= 3);
        assert!(cache.total_bytes() <= 256);
    }

    #[test]
    fn concurrent_access_is_safe() {
        let cache = Arc::new(PageCache::new(PageCacheConfig {
            capacity_bytes: Some(1024 * 8),
        }));

        let mut handles = Vec::new();
        for shard in 0..4 {
            let cache = cache.clone();
            handles.push(thread::spawn(move || {
                for page in 0..64 {
                    let key = format!("{}:{}", shard, page);
                    let payload: Arc<[u8]> = Arc::from(vec![page as u8; 128]);
                    cache.insert(key.clone(), payload);
                    let _ = cache.get(&key);
                }
            }));
        }

        for handle in handles {
            handle.join().expect("worker panicked");
        }

        assert!(cache.total_bytes() <= 1024 * 8);
    }

    #[test]
    fn observer_reentry_does_not_deadlock() {
        use std::sync::atomic::{AtomicBool, Ordering};

        struct ReentrantObserver {
            cache: OnceLock<Weak<PageCache<&'static str>>>,
            invoked: AtomicBool,
        }

        impl ReentrantObserver {
            fn new() -> Self {
                Self {
                    cache: OnceLock::new(),
                    invoked: AtomicBool::new(false),
                }
            }

            fn attach(&self, cache: &Arc<PageCache<&'static str>>) {
                let _ = self.cache.set(Arc::downgrade(cache));
            }
        }

        impl PageCacheObserver<&'static str> for ReentrantObserver {
            fn on_evict(&self, _key: &&'static str, _bytes: &[u8]) {
                self.invoked.store(true, Ordering::SeqCst);
                if let Some(cache) = self.cache.get().and_then(|weak| weak.upgrade()) {
                    let _ = cache.total_bytes();
                }
            }
        }

        let observer = Arc::new(ReentrantObserver::new());
        let cache = Arc::new(PageCache::with_observer(
            PageCacheConfig {
                capacity_bytes: Some(4),
            },
            Some(observer.clone() as Arc<dyn PageCacheObserver<&'static str>>),
        ));
        observer.attach(&cache);

        cache.insert("page-a", Arc::from(vec![1u8; 4]));
        cache.insert("page-b", Arc::from(vec![2u8; 4]));

        assert!(observer.invoked.load(Ordering::SeqCst));
    }

    #[test]
    fn metrics_and_observer_are_updated() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        #[derive(Default)]
        struct Recorder {
            inserts: AtomicUsize,
            hits: AtomicUsize,
            misses: AtomicUsize,
            evictions: AtomicUsize,
        }

        impl PageCacheObserver<&'static str> for Recorder {
            fn on_insert(&self, _key: &&'static str, _bytes: &[u8]) {
                self.inserts.fetch_add(1, Ordering::Relaxed);
            }

            fn on_hit(&self, _key: &&'static str) {
                self.hits.fetch_add(1, Ordering::Relaxed);
            }

            fn on_miss(&self, _key: &&'static str) {
                self.misses.fetch_add(1, Ordering::Relaxed);
            }

            fn on_evict(&self, _key: &&'static str, _bytes: &[u8]) {
                self.evictions.fetch_add(1, Ordering::Relaxed);
            }
        }

        let observer = Arc::new(Recorder::default());
        let cache = PageCache::with_observer(
            PageCacheConfig {
                capacity_bytes: Some(4),
            },
            Some(observer.clone()),
        );

        let payload: Arc<[u8]> = Arc::from(vec![1u8; 4]);
        cache.insert("page-a", payload.clone());
        assert!(cache.get(&"page-a").is_some());
        assert!(cache.get(&"page-b").is_none());
        cache.insert("page-c", Arc::from(vec![2u8; 4]));

        let metrics = cache.metrics();
        assert_eq!(metrics.insertions, 2);
        assert_eq!(metrics.hits, 1);
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.evictions, 1);

        assert_eq!(observer.inserts.load(Ordering::Relaxed), 2);
        assert_eq!(observer.hits.load(Ordering::Relaxed), 1);
        assert_eq!(observer.misses.load(Ordering::Relaxed), 1);
        assert_eq!(observer.evictions.load(Ordering::Relaxed), 1);
    }
}
