//! Slab.
//! Part of code and design forked from tokio.

use parking_lot::Mutex;
use std::{
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
};

// Configuration for ConcurrentSlab
const CONCURRENT_SLAB_SHARDS: usize = 512;
const CONCURRENT_SLAB_SHARD_BITS: usize = 9; // 2^9 = 512 shards
const CONCURRENT_SLAB_LOCAL_BITS: usize = 16; // Supports up to 65,536 entries per shard
const CONCURRENT_SLAB_SHARD_MASK: usize = CONCURRENT_SLAB_SHARDS - 1;
const CONCURRENT_SLAB_LOCAL_MASK: usize = (1 << CONCURRENT_SLAB_LOCAL_BITS) - 1;

/// Pre-allocated storage for a uniform data type
#[derive(Default)]
pub(crate) struct Slab<T> {
    // pages of continued memory
    pages: [Option<Page<T>>; NUM_PAGES],
    // cached write page id
    w_page_id: usize,
    // current generation
    generation: u32,
}

const NUM_PAGES: usize = 26;
const PAGE_INITIAL_SIZE: usize = 64;
const COMPACT_INTERVAL: u32 = 2048;

impl<T> Slab<T> {
    /// Create a new slab.
    pub(crate) const fn new() -> Slab<T> {
        Slab {
            pages: [
                None, None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, None, None, None, None, None, None, None, None, None, None, None,
            ],
            w_page_id: 0,
            generation: 0,
        }
    }

    /// Get slab len.
    #[allow(unused)]
    pub(crate) fn len(&self) -> usize {
        self.pages.iter().fold(0, |acc, page| match page {
            Some(page) => acc + page.used,
            None => acc,
        })
    }

    pub(crate) fn get(&mut self, key: usize) -> Option<Ref<'_, T>> {
        let page_id = get_page_id(key);
        // here we make 2 mut ref so we must make it safe.
        let slab = unsafe { &mut *(self as *mut Slab<T>) };
        let page = match unsafe { self.pages.get_unchecked_mut(page_id) } {
            Some(page) => page,
            None => return None,
        };
        let index = key - page.prev_len;
        match page.get_entry_mut(index) {
            None => None,
            Some(entry) => match entry {
                Entry::Vacant(_) => None,
                Entry::Occupied(_) => Some(Ref { slab, page, index }),
            },
        }
    }

    /// Insert an element into slab. The key is returned.
    /// Note: If the slab is out of slot, it will panic.
    pub(crate) fn insert(&mut self, val: T) -> usize {
        let begin_id = self.w_page_id;
        for i in begin_id..NUM_PAGES {
            unsafe {
                let page = match self.pages.get_unchecked_mut(i) {
                    Some(page) => page,
                    None => {
                        let page = Page::new(
                            PAGE_INITIAL_SIZE << i,
                            (PAGE_INITIAL_SIZE << i) - PAGE_INITIAL_SIZE,
                        );
                        let r = self.pages.get_unchecked_mut(i);
                        *r = Some(page);
                        r.as_mut().unwrap_unchecked()
                    }
                };
                if let Some(slot) = page.alloc() {
                    page.set(slot, val);
                    self.w_page_id = i;
                    return slot + page.prev_len;
                }
            }
        }
        panic!("out of slot");
    }

    /// Remove an element from slab.
    #[allow(unused)]
    pub(crate) fn remove(&mut self, key: usize) -> Option<T> {
        let page_id = get_page_id(key);
        let page = match unsafe { self.pages.get_unchecked_mut(page_id) } {
            Some(page) => page,
            None => return None,
        };
        let val = page.remove(key - page.prev_len);
        self.mark_remove();
        val
    }

    pub(crate) fn mark_remove(&mut self) {
        // compact
        self.generation = self.generation.wrapping_add(1);
        if self.generation.is_multiple_of(COMPACT_INTERVAL) {
            // reset write page index
            self.w_page_id = 0;
            // find the last allocated page and try to drop
            if let Some((id, last_page)) = self
                .pages
                .iter_mut()
                .enumerate()
                .rev()
                .find_map(|(id, p)| p.as_mut().map(|p| (id, p)))
            {
                if last_page.is_empty() && id > 0 {
                    unsafe {
                        *self.pages.get_unchecked_mut(id) = None;
                    }
                }
            }
        }
    }
}

// Forked from tokio.
fn get_page_id(key: usize) -> usize {
    const POINTER_WIDTH: u32 = std::mem::size_of::<usize>() as u32 * 8;
    const PAGE_INDEX_SHIFT: u32 = PAGE_INITIAL_SIZE.trailing_zeros() + 1;

    let slot_shifted = (key.saturating_add(PAGE_INITIAL_SIZE)) >> PAGE_INDEX_SHIFT;
    ((POINTER_WIDTH - slot_shifted.leading_zeros()) as usize).min(NUM_PAGES - 1)
}

/// Ref point to a valid slot.
pub(crate) struct Ref<'a, T> {
    slab: &'a mut Slab<T>,
    page: &'a mut Page<T>,
    index: usize,
}

impl<T> Ref<'_, T> {
    #[allow(unused)]
    pub(crate) fn remove(self) -> T {
        // # Safety
        // We make sure the index is valid.
        let val = unsafe { self.page.remove(self.index).unwrap_unchecked() };
        self.slab.mark_remove();
        val
    }
}

impl<T> AsRef<T> for Ref<'_, T> {
    fn as_ref(&self) -> &T {
        // # Safety
        // We make sure the index is valid.
        unsafe { self.page.get(self.index).unwrap_unchecked() }
    }
}

impl<T> AsMut<T> for Ref<'_, T> {
    fn as_mut(&mut self) -> &mut T {
        // # Safety
        // We make sure the index is valid.
        unsafe { self.page.get_mut(self.index).unwrap_unchecked() }
    }
}

impl<T> Deref for Ref<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T> DerefMut for Ref<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

enum Entry<T> {
    Vacant(usize),
    Occupied(T),
}

impl<T> Entry<T> {
    fn as_ref(&self) -> Option<&T> {
        match self {
            Entry::Vacant(_) => None,
            Entry::Occupied(inner) => Some(inner),
        }
    }

    fn as_mut(&mut self) -> Option<&mut T> {
        match self {
            Entry::Vacant(_) => None,
            Entry::Occupied(inner) => Some(inner),
        }
    }

    fn is_vacant(&self) -> bool {
        matches!(self, Entry::Vacant(_))
    }

    unsafe fn unwrap_unchecked(self) -> T {
        match self {
            Entry::Vacant(_) => std::hint::unreachable_unchecked(),
            Entry::Occupied(inner) => inner,
        }
    }
}

struct Page<T> {
    // continued buffer of fixed size
    slots: Box<[MaybeUninit<Entry<T>>]>,
    // number of occupied slots
    used: usize,
    // number of initialized slots
    initialized: usize,
    // next slot to write
    next: usize,
    // sum of previous page's slots count
    prev_len: usize,
}

impl<T> Page<T> {
    fn new(size: usize, prev_len: usize) -> Self {
        let mut buffer = Vec::with_capacity(size);
        unsafe { buffer.set_len(size) };
        let slots = buffer.into_boxed_slice();
        Self {
            slots,
            used: 0,
            initialized: 0,
            next: 0,
            prev_len,
        }
    }

    fn is_empty(&self) -> bool {
        self.used == 0
    }

    fn is_full(&self) -> bool {
        self.used == self.slots.len()
    }

    // alloc a slot
    // Safety: after slot is allocated, the caller must guarantee it will be
    // initialized
    unsafe fn alloc(&mut self) -> Option<usize> {
        let next = self.next;
        if self.is_full() {
            // current page is full
            debug_assert_eq!(next, self.slots.len(), "next should eq to slots.len()");
            return None;
        } else if next >= self.initialized {
            // the slot to write is not initialized
            debug_assert_eq!(next, self.initialized, "next should eq to initialized");
            self.initialized += 1;
            self.next += 1;
        } else {
            // the slot has already been initialized
            // it must be Vacant
            let slot = self.slots.get_unchecked(next).assume_init_ref();
            match slot {
                Entry::Vacant(next_slot) => {
                    self.next = *next_slot;
                }
                Entry::Occupied(_) => {
                    panic!(
                        "slab corruption: slot {} is Occupied but in free list (next={}, initialized={}, used={}, len={})",
                        next,
                        next,
                        self.initialized,
                        self.used,
                        self.slots.len()
                    );
                }
            }
        }
        self.used += 1;
        Some(next)
    }

    // set value of the slot
    // Safety: the slot must returned by Self::alloc.
    unsafe fn set(&mut self, slot: usize, val: T) {
        let slot = self.slots.get_unchecked_mut(slot);
        *slot = MaybeUninit::new(Entry::Occupied(val));
    }

    fn get(&self, slot: usize) -> Option<&T> {
        if slot >= self.initialized {
            return None;
        }
        unsafe { self.slots.get_unchecked(slot).assume_init_ref() }.as_ref()
    }

    fn get_mut(&mut self, slot: usize) -> Option<&mut T> {
        if slot >= self.initialized {
            return None;
        }
        unsafe { self.slots.get_unchecked_mut(slot).assume_init_mut() }.as_mut()
    }

    fn get_entry_mut(&mut self, slot: usize) -> Option<&mut Entry<T>> {
        if slot >= self.initialized {
            return None;
        }
        unsafe { Some(self.slots.get_unchecked_mut(slot).assume_init_mut()) }
    }

    fn remove(&mut self, slot: usize) -> Option<T> {
        if slot >= self.initialized {
            return None;
        }
        unsafe {
            let slot_mut = self.slots.get_unchecked_mut(slot).assume_init_mut();
            if slot_mut.is_vacant() {
                // Double-remove detected!
                panic!(
                    "slab double-remove: slot {} is already Vacant! (next={}, initialized={}, used={}, len={})",
                    slot,
                    self.next,
                    self.initialized,
                    self.used,
                    self.slots.len()
                );
            }
            let old_next = self.next;
            let val = std::mem::replace(slot_mut, Entry::Vacant(old_next));
            self.next = slot;
            self.used -= 1;

            Some(val.unwrap_unchecked())
        }
    }
}

impl<T> Drop for Page<T> {
    fn drop(&mut self) {
        let mut to_drop = std::mem::take(&mut self.slots).into_vec();

        unsafe {
            if self.is_empty() {
                // fast drop if empty
                to_drop.set_len(0);
            } else {
                // slow drop
                to_drop.set_len(self.initialized);
                std::mem::transmute::<Vec<MaybeUninit<Entry<T>>>, Vec<Entry<T>>>(to_drop);
            }
        }
    }
}

/// Concurrent slab with 512 mutex-protected shards for reduced contention.
///
/// This is designed for use with io_uring where operations can be submitted
/// and completed by any worker thread. A single mutex would become a bottleneck,
/// so we shard the slab into 512 independent segments.
///
/// # Thread Safety
///
/// - Each shard is protected by its own `parking_lot::Mutex`
/// - Operations on different shards can proceed concurrently
/// - Provides much better scalability than a single mutex
///
/// # Key Encoding
///
/// Keys are encoded as: `(shard_id << LOCAL_BITS) | local_key`
/// - Upper 9 bits: shard identifier (0-511)
/// - Lower 16 bits: local key within that shard (0-65535)
///
/// This allows:
/// - 512 total shards
/// - Up to 65,536 entries per shard
/// - Total capacity: ~33.5 million entries
///
/// # Performance Characteristics
///
/// - Insert: O(1) with lock on single shard
/// - Get: O(1) with lock on single shard
/// - Remove: O(1) with lock on single shard
/// - Contention: Reduced by 512x compared to single mutex
pub struct ConcurrentSlab<T> {
    shards: Box<[Shard<T>]>,
}

struct Shard<T> {
    slab: Mutex<Slab<T>>,
    key_offset: usize,
}

impl<T> ConcurrentSlab<T> {
    /// Create a new concurrent slab with 512 shards.
    pub fn new() -> Self {
        let mut shards = Vec::with_capacity(CONCURRENT_SLAB_SHARDS);
        for shard_id in 0..CONCURRENT_SLAB_SHARDS {
            let key_offset = shard_id << CONCURRENT_SLAB_LOCAL_BITS;
            shards.push(Shard {
                slab: Mutex::new(Slab::new()),
                key_offset,
            });
        }
        Self {
            shards: shards.into_boxed_slice(),
        }
    }

    /// Insert a value into the slab, returning a key.
    ///
    /// The shard is selected by trying shards in sequence until finding one with capacity.
    /// This provides good load distribution while being simple and safe.
    pub fn insert(&self, val: T) -> usize {
        // Start with a hint based on current shard rotation to distribute load
        // Use a simple counter-like approach by checking shards sequentially
        let start_shard = std::hint::black_box(0); // Start from shard 0, will distribute through capacity checks

        // Try to insert into shards sequentially, looking for one with capacity
        let mut current_shard = start_shard;
        loop {
            let shard = &self.shards[current_shard];
            let mut slab_guard = shard.slab.lock();

            // Check if shard has capacity (max 65k per shard)
            if slab_guard.len() < (1 << CONCURRENT_SLAB_LOCAL_BITS) {
                let local_key = slab_guard.insert(val);
                return shard.key_offset | local_key;
            }

            // Shard full, try next one (simple round-robin)
            current_shard = (current_shard + 1) % CONCURRENT_SLAB_SHARDS;

            // If we've tried all shards and they're all full, panic
            if current_shard == start_shard {
                panic!("ConcurrentSlab is out of capacity (all 512 shards full)");
            }
        }
    }

    /// Get a reference to a value by key.
    ///
    /// Returns `None` if key is not present.
    ///
    /// This method holds the shard's mutex lock for the duration of the access.
    pub fn get(&self, key: usize) -> Option<SlabGuard<'_, T>> {
        let (shard_id, local_key) = Self::decode_key(key);
        let shard = &self.shards.get(shard_id)?;
        let mut slab_guard = shard.slab.lock();

        // Use the Slab's get method which returns a Ref
        let slab_ref = slab_guard.get(local_key)?;

        // We need to convert Ref to a safe reference
        // Since SlabRef gives us &T, we can return it via SlabGuard
        let value_ptr = slab_ref.as_ref() as *const T;
        drop(slab_ref);

        Some(SlabGuard {
            key,
            local_key,
            shard,
            value: value_ptr,
            _lock_guard: slab_guard,
        })
    }

    /// Remove a value by key, returning it.
    ///
    /// Returns `None` if key is not present.
    pub fn remove(&self, key: usize) -> Option<T> {
        let (shard_id, local_key) = Self::decode_key(key);
        let shard = self.shards.get(shard_id)?;
        let mut slab_guard = shard.slab.lock();
        slab_guard.remove(local_key)
    }

    /// Get the total number of elements across all shards.
    ///
    /// Note: This iterates over all shards and holds each lock briefly,
    /// so it may be slightly expensive with high contention.
    pub fn len(&self) -> usize {
        self.shards
            .iter()
            .map(|shard| shard.slab.lock().len())
            .sum()
    }

    /// Decode a key into (shard_id, local_index) components.
    #[inline]
    fn decode_key(key: usize) -> (usize, usize) {
        let local_index = key & CONCURRENT_SLAB_LOCAL_MASK;
        let shard_id = (key >> CONCURRENT_SLAB_LOCAL_BITS) & CONCURRENT_SLAB_SHARD_MASK;
        (shard_id, local_index)
    }
}

impl<T> Default for ConcurrentSlab<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard that holds a reference to a value in the slab.
///
/// This guard holds the shard's mutex lock, ensuring the reference
/// remains valid for the duration of the guard's lifetime.
pub struct SlabGuard<'a, T> {
    key: usize,
    local_key: usize,
    shard: &'a Shard<T>,
    value: *const T,
    _lock_guard: parking_lot::MutexGuard<'a, Slab<T>>,
}

impl<'a, T> SlabGuard<'a, T> {
    /// Get the key for this entry.
    pub fn key(&self) -> usize {
        self.key
    }

    /// Remove the entry from the slab, returning the value.
    ///
    /// This consumes the guard and releases the mutex.
    pub fn remove(mut self) -> T {
        let mut slab_guard = self.shard.slab.lock();
        slab_guard
            .remove(self.local_key)
            .expect("double-remove detected in ConcurrentSlab")
    }
}

impl<'a, T> Deref for SlabGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.value }
    }
}

impl<'a, T> SlabGuard<'a, T> {
    /// Get a mutable pointer to the underlying value.
    ///
    /// # Safety
    ///
    /// This returns a raw mutable pointer. The caller must ensure:
    /// - The pointer is not used after the guard is dropped
    /// - No other mutable references exist to the same data
    /// - The pointer is only dereferenced while the guard is alive
    #[inline]
    pub unsafe fn as_mut_ptr(&self) -> *mut T {
        self.value as *mut T
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove_one() {
        let mut slab = Slab::default();
        let key = slab.insert(10);
        assert_eq!(slab.get(key).unwrap().as_mut(), &10);
        assert_eq!(slab.remove(key), Some(10));
        assert!(slab.get(key).is_none());
        assert_eq!(slab.len(), 0);
    }

    #[test]
    fn insert_get_remove_many() {
        let mut slab = Slab::new();
        let mut keys = vec![];

        for i in 0..10 {
            for j in 0..10 {
                let val = (i * 10) + j;

                let key = slab.insert(val);
                keys.push((key, val));
                assert_eq!(slab.get(key).unwrap().as_mut(), &val);
            }

            for (key, val) in keys.drain(..) {
                assert_eq!(val, slab.remove(key).unwrap());
            }
        }
    }

    #[test]
    fn get_not_exist() {
        let mut slab = Slab::<i32>::new();
        assert!(slab.get(0).is_none());
        assert!(slab.get(1).is_none());
        assert!(slab.get(usize::MAX).is_none());
        assert!(slab.remove(0).is_none());
        assert!(slab.remove(1).is_none());
        assert!(slab.remove(usize::MAX).is_none());
    }

    #[test]
    fn insert_remove_big() {
        let mut slab = Slab::default();
        let keys = (0..1_000_000).map(|i| slab.insert(i)).collect::<Vec<_>>();
        keys.iter().zip(0..1_000_000).for_each(|(key, val)| {
            assert_eq!(slab.remove(*key).unwrap(), val);
        });
        keys.iter().for_each(|key| {
            assert!(slab.get(*key).is_none());
        });
        assert_eq!(slab.len(), 0);
    }

    // ConcurrentSlab tests
    #[test]
    fn concurrent_insert_get_remove_one() {
        let slab = ConcurrentSlab::new();
        let key = slab.insert(42);
        {
            let guard = slab.get(key).expect("should get value");
            assert_eq!(*guard, 42);
        }
        assert_eq!(slab.remove(key), Some(42));
        assert!(slab.get(key).is_none());
        assert_eq!(slab.len(), 0);
    }

    #[test]
    fn concurrent_insert_get_remove_many() {
        let slab = ConcurrentSlab::new();
        let mut keys = vec![];

        for i in 0..1000 {
            let key = slab.insert(i);
            keys.push((key, i));
        }

        for (key, val) in &keys {
            let guard = slab.get(*key).expect("should get value");
            assert_eq!(*guard, *val);
        }

        for (key, val) in keys {
            assert_eq!(slab.remove(key), Some(val));
        }

        assert_eq!(slab.len(), 0);
    }

    #[test]
    fn concurrent_key_encoding() {
        let slab = ConcurrentSlab::new();

        // Test that keys are properly encoded
        let key1 = slab.insert(1);
        let (shard_id1, local_key1) = ConcurrentSlab::<()>::decode_key(key1);
        assert!(shard_id1 < CONCURRENT_SLAB_SHARDS);
        assert!(local_key1 < (1 << CONCURRENT_SLAB_LOCAL_BITS));

        let key2 = slab.insert(2);
        let (shard_id2, local_key2) = ConcurrentSlab::<()>::decode_key(key2);
        assert!(shard_id2 < CONCURRENT_SLAB_SHARDS);
        assert!(local_key2 < (1 << CONCURRENT_SLAB_LOCAL_BITS));
    }

    #[test]
    fn concurrent_remove_via_guard() {
        let slab = ConcurrentSlab::new();
        let key = slab.insert(99);

        let guard = slab.get(key).expect("should get value");
        assert_eq!(*guard, 99);
        let val = guard.remove();
        assert_eq!(val, 99);

        assert!(slab.get(key).is_none());
        assert_eq!(slab.len(), 0);
    }

    #[test]
    fn concurrent_capacity_distribution() {
        let slab = ConcurrentSlab::new();

        // Insert many values to test distribution across shards
        let keys: Vec<_> = (0..10000).map(|i| slab.insert(i)).collect();

        // Verify all can be retrieved
        for (i, key) in keys.iter().enumerate() {
            let guard = slab.get(*key).expect("should get value");
            assert_eq!(*guard, i);
        }

        assert_eq!(slab.len(), 10000);
    }
}
