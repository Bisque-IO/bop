use std::marker::PhantomData;
use std::{mem, ptr};
use crate::alloc::Allocator;
use crate::hash::Hasher;

pub trait Map {
    type Key: Hasher;
    type Value;

    fn capacity(&self) -> u64;

    fn length(&self) -> u64;

    fn contains(&self, key: Self::Key) -> bool {
        self.get(key).is_some()
    }

    fn get(&self, key: Self::Key) -> Option<Self::Value>;
}

pub trait MapBuilder: Map {
    type Allocator: Allocator;

    fn set(&mut self, key: Self::Key, value: Self::Value) -> Option<Self::Value>;

    fn delete(&mut self, key: Self::Key) -> Option<Self::Value>;
}

const LOAD_FACTOR: f64 = 0.85;
const DIB_BIT_SIZE: u64 = 16;
const HASH_BIT_SIZE: u64 = 64 - DIB_BIT_SIZE;
const MAX_HASH: u64 = !0u64 >> DIB_BIT_SIZE;
const MAX_DIB: u64 = !0u64 >> HASH_BIT_SIZE;

pub struct FlatMap<'a, K: Hasher, V> {
    cap: u32,
    length: u32,
    entries: *mut FlatMapEntry<'a, K, V>,
    _phantom: PhantomData<&'a (K, V)>,
}

impl<'a, K: Hasher, V> FlatMap<'a, K, V> {
    pub fn new() {

    }
}

pub struct FlatMapBuilder<'a, K: Hasher, V, A: Allocator> {
    inner: *mut FlatMap<'a, K, V>,
    mask: u64,
    grow_at: u64,
    shrink_at: u64,
    _phantom: PhantomData<&'a (K, V, A)>,
}

#[repr(C)]
struct FlatMapEntry<'a, K: Hasher, V> {
    // bitfield { hash:48 dib:16 }
    hdib: u64,
    key: K,
    value: V,
    _phantom: PhantomData<&'a (K, V)>,
}

impl<'a, K: Hasher, V> FlatMapEntry<'a, K, V> {
    #[inline]
    pub fn new(hdib: u64, key: K, value: V) -> Self {
        return Self { hdib: hdib.to_le(), key, value, _phantom: PhantomData };
    }

    #[inline]
    pub fn dib(&self) -> u64 {
        (self.hdib & MAX_DIB) as u64
    }

    #[inline]
    pub fn hash(&self) -> u64 {
        u64::from_le(self.hdib) >> DIB_BIT_SIZE
    }

    #[inline]
    pub fn set_dib(&mut self, dib: u64) {
        self.hdib = u64::from_le(self.hdib) >> DIB_BIT_SIZE << DIB_BIT_SIZE | dib & MAX_DIB
    }

    #[inline]
    pub fn set_hash(&mut self, hash: u64) {
        self.hdib = hash << DIB_BIT_SIZE | u64::from_le(self.hdib) & MAX_DIB
    }
}

impl<'a, K: Hasher, V> Clone for FlatMapEntry<'a, K, V> {
    #[inline(always)]
    fn clone(&self) -> Self {
        unsafe {
            FlatMapEntry {
                hdib: self.hdib,
                key: ptr::read(&self.key),
                value: ptr::read(&self.value),
                _phantom: PhantomData,
            }
        }
    }
}

#[inline(always)]
fn make_hdib(hash: u64, dib: u64) -> u64 {
    return hash << DIB_BIT_SIZE | dib & MAX_DIB;
}

impl<'a, K: Hasher, V> FlatMap<'a, K, V> {
    #[inline(always)]
    fn mask(&self) -> u64 {
        self.capacity() - 1
    }
}

impl<'a, K: Hasher, V> Map for FlatMap<'a, K, V> {
    type Key = K;
    type Value = V;

    #[inline(always)]
    fn capacity(&self) -> u64 {
        u32::from_le(self.cap) as u64
    }

    #[inline(always)]
    fn length(&self) -> u64 {
        u32::from_le(self.length) as u64
    }

    fn get(&self, key: Self::Key) -> Option<Self::Value> {
        let hash = key.hash_wap() >> DIB_BIT_SIZE;
        let mask = self.mask();
        let mut i = hash & mask;

        loop {
            let offset = mem::size_of::<FlatMapEntry<'a, K, V>>() * i as usize;
            let cursor = unsafe { &mut *((self.entries as *mut u8).offset(offset as isize) as *mut FlatMapEntry<'a, K, V>) };
            if cursor.dib() == 0 {
                return None;
            }
            if hash == cursor.hash() && key == cursor.key {
                return Some(unsafe { ptr::read(&cursor.value as *const V) });
            }
            i = (i + 1) & mask;
        }
    }
}

impl<'a, K: Hasher, V, A: Allocator> Map for FlatMapBuilder<'a, K, V, A> {
    type Key = K;
    type Value = V;

    #[inline(always)]
    fn capacity(&self) -> u64 {
        unsafe { (&*self.inner).capacity() }
    }

    #[inline(always)]
    fn length(&self) -> u64 {
        unsafe { (&*self.inner).length() }
    }

    fn get(&self, key: Self::Key) -> Option<Self::Value> {
        unsafe { (&*self.inner).get(key) }
    }
}

impl<'a, K: Hasher, V, A: Allocator> MapBuilder for FlatMapBuilder<'a, K, V, A> {
    type Allocator = A;

    fn set(&mut self, key: Self::Key, value: Self::Value) -> Option<Self::Value> {
        if self.length() >= self.grow_at {
            self.resize(self.capacity() * 2);
        }
        self.do_set(key.hash_wap(), key, value)
    }

    fn delete(&mut self, key: Self::Key) -> Option<Self::Value> {
        self.do_delete(key)
    }
}

impl<'a, K: Hasher, V, A: Allocator> FlatMapBuilder<'a, K, V, A> {
    pub fn new(inner: *mut FlatMap<'a, K, V>) -> Self {
        Self {
            inner,
            mask: 0,
            grow_at: 0,
            shrink_at: 0,
            _phantom: PhantomData
        }
    }

    #[inline(always)]
    fn mask(&self) -> u64 {
        self.mask
    }

    fn resize(&mut self, _new_size: u64) {}

    #[inline(always)]
    fn set_length(&mut self, new_length: u64) {
        unsafe { (&mut *self.inner).length = (new_length as u32).to_le(); }
    }

    #[inline(always)]
    fn entry_at(&self, at: u64) -> &mut FlatMapEntry<'a, K, V> {
        unsafe {
            let inner = &*(self.inner);
            &mut *((inner.entries as *mut u8).offset(
                mem::size_of::<FlatMapEntry<'a, K, V>>() as isize * at as isize
            ) as *mut FlatMapEntry<'a, K, V>)
        }
    }

    fn do_set(&mut self, hash: u64, key: K, value: V) -> Option<V> {
        let mut e = FlatMapEntry::new(make_hdib(hash, 1), key, value);
        let mut i = e.hash() & self.mask();

        loop {
            let cursor = self.entry_at(i);
            if cursor.dib() == 0 {
                unsafe { *(cursor as *mut FlatMapEntry<'a, K, V>) = e; }
                self.set_length(self.length() + 1);
                return None;
            }
            if e.hash() == cursor.hash() && e.key == cursor.key {
                let prev: V = unsafe { ptr::read(&cursor.value as *const V) };
                unsafe { *(cursor as *mut FlatMapEntry<'a, K, V>) = e; }
                return Some(prev);
            }
            if cursor.dib() < e.dib() {
                unsafe {
                    let next = ptr::read(cursor);
                    ptr::write(
                        cursor as *mut FlatMapEntry<'a, K, V>,
                        e.clone(),
                    );
                    ptr::write(&mut e as *mut FlatMapEntry<'a, K, V>, next);
                }
            }
            i = (i + 1) & self.mask();
            e.set_dib(e.dib() + 1);
        }
    }

    fn do_delete(&mut self, key: K) -> Option<V> {
        if self.length() == 0 {
            return None;
        }

        let hash = key.hash_wap() >> DIB_BIT_SIZE;
        let mask = self.mask();
        let mut i = hash & mask;

        loop {
            let cursor = self.entry_at(i);
            if cursor.dib() == 0 {
                return None;
            }
            if hash == cursor.hash() && key == cursor.key {
                let prev: V = unsafe { ptr::read(&cursor.value as *const V) };
                cursor.set_dib(0);
                self.remove(i);
                return Some(prev);
            }
            i = (i + 1) & mask;
        }
    }

    fn remove(&mut self, mut index: u64) {
        'main_loop:
        loop {
            let pi = index;
            index = (index + 1) & self.mask();
            let cursor = self.entry_at(index);
            if cursor.dib() <= 1 {
                unsafe {
                    ptr::write(cursor as *mut FlatMapEntry<'a, K, V>, mem::zeroed());
                }
                break 'main_loop;
            }
            unsafe {
                let pi_entry = self.entry_at(pi);
                ptr::write(
                    pi_entry as *mut FlatMapEntry<'a, K, V>,
                    cursor.clone());
                pi_entry.set_dib(pi_entry.dib() - 1);
            }
        }
        self.set_length(self.length() - 1);
    }
}

#[cfg(test)]
mod tests {
    // use std::alloc::GlobalAlloc;
    // use std::ptr;
    // use crate::appender::Appender;
    // use crate::map::FlatMapBuilder;

    #[test]
    fn simple() {
        // let mut builder: FlatMapBuilder<'static, i32, i32, Appender> = FlatMapBuilder::new(ptr::null_mut());
    }
}