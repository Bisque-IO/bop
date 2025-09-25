use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

/// Key identifying a cached page entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageCacheKey {
    pub store_id: u64,
    pub generation: u64,
    pub page_id: u64,
}

impl Hash for PageCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.store_id);
        state.write_u64(self.generation);
        state.write_u64(self.page_id);
    }
}

#[derive(Debug, Default)]
pub struct GlobalPageCache {
    inner: Mutex<HashMap<PageCacheKey, Arc<Vec<u8>>>>,
}

impl GlobalPageCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, key: PageCacheKey, data: Arc<Vec<u8>>) -> Option<Arc<Vec<u8>>> {
        let mut guard = self.inner.lock().expect("cache mutex poisoned");
        guard.insert(key, data)
    }

    pub fn get(&self, key: &PageCacheKey) -> Option<Arc<Vec<u8>>> {
        let guard = self.inner.lock().expect("cache mutex poisoned");
        guard.get(key).cloned()
    }

    pub fn evict_generation(&self, store_id: u64, generation: u64) {
        let mut guard = self.inner.lock().expect("cache mutex poisoned");
        guard.retain(|k, _| !(k.store_id == store_id && k.generation == generation));
    }

    pub fn clear_store(&self, store_id: u64) {
        let mut guard = self.inner.lock().expect("cache mutex poisoned");
        guard.retain(|k, _| k.store_id != store_id);
    }

    pub fn len(&self) -> usize {
        self.inner.lock().expect("cache mutex poisoned").len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(store: u64, generation: u64, page: u64) -> PageCacheKey {
        PageCacheKey {
            store_id: store,
            generation,
            page_id: page,
        }
    }

    #[test]
    fn insert_and_get_page() {
        let cache = GlobalPageCache::new();
        let data = Arc::new(vec![1u8; 16]);
        cache.insert(key(1, 7, 10), Arc::clone(&data));
        let fetched = cache.get(&key(1, 7, 10)).expect("cache hit");
        assert_eq!(&*fetched, &*data);
    }

    #[test]
    fn evict_generation_removes_matches() {
        let cache = GlobalPageCache::new();
        cache.insert(key(2, 1, 1), Arc::new(vec![0u8; 4]));
        cache.insert(key(2, 2, 1), Arc::new(vec![0u8; 4]));
        cache.insert(key(3, 1, 1), Arc::new(vec![0u8; 4]));
        cache.evict_generation(2, 1);
        assert!(cache.get(&key(2, 1, 1)).is_none());
        assert!(cache.get(&key(2, 2, 1)).is_some());
        assert!(cache.get(&key(3, 1, 1)).is_some());
    }

    #[test]
    fn clear_store_prunes_everything() {
        let cache = GlobalPageCache::new();
        cache.insert(key(5, 1, 1), Arc::new(vec![0u8; 4]));
        cache.insert(key(5, 2, 2), Arc::new(vec![0u8; 4]));
        cache.insert(key(6, 1, 1), Arc::new(vec![0u8; 4]));
        cache.clear_store(5);
        assert_eq!(cache.len(), 1);
        assert!(cache.get(&key(6, 1, 1)).is_some());
    }
}
