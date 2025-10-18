use core::fmt;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU64, Ordering};
use std::boxed::Box;
use std::vec::Vec;

/// Pads and aligns a value to the length of a cache line.
///
/// In concurrent programming, sometimes it is desirable to make sure commonly accessed pieces of
/// data are not placed into the same cache line. Updating an atomic value invalidates the whole
/// cache line it belongs to, which makes the next access to the same cache line slower for other
/// CPU cores. Use `CachePadded` to ensure updating one piece of data doesn't invalidate other
/// cached data.
///
/// # Size and alignment
///
/// Cache lines are assumed to be N bytes long, depending on the architecture:
///
/// * On x86-64, aarch64, and powerpc64, N = 128.
/// * On arm, mips, mips64, sparc, and hexagon, N = 32.
/// * On m68k, N = 16.
/// * On s390x, N = 256.
/// * On all others, N = 64.
///
/// Note that N is just a reasonable guess and is not guaranteed to match the actual cache line
/// length of the machine the program is running on. On modern Intel architectures, spatial
/// prefetcher is pulling pairs of 64-byte cache lines at a time, so we pessimistically assume that
/// cache lines are 128 bytes long.
///
/// The size of `CachePadded<T>` is the smallest multiple of N bytes large enough to accommodate
/// a value of type `T`.
///
/// The alignment of `CachePadded<T>` is the maximum of N bytes and the alignment of `T`.
#[derive(Clone, Copy, Default, Hash, PartialEq, Eq)]
#[cfg_attr(
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "arm64ec",
        target_arch = "powerpc64",
    ),
    repr(align(128))
)]
#[cfg_attr(
    any(
        target_arch = "arm",
        target_arch = "mips",
        target_arch = "mips32r6",
        target_arch = "mips64",
        target_arch = "mips64r6",
        target_arch = "sparc",
        target_arch = "hexagon",
    ),
    repr(align(32))
)]
#[cfg_attr(target_arch = "m68k", repr(align(16)))]
#[cfg_attr(target_arch = "s390x", repr(align(256)))]
#[cfg_attr(
    not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "arm64ec",
        target_arch = "powerpc64",
        target_arch = "arm",
        target_arch = "mips",
        target_arch = "mips32r6",
        target_arch = "mips64",
        target_arch = "mips64r6",
        target_arch = "sparc",
        target_arch = "hexagon",
        target_arch = "m68k",
        target_arch = "s390x",
    )),
    repr(align(64))
)]
pub struct CachePadded<T> {
    value: T,
}

unsafe impl<T: Send> Send for CachePadded<T> {}
unsafe impl<T: Sync> Sync for CachePadded<T> {}

impl<T> CachePadded<T> {
    /// Pads and aligns a value to the length of a cache line.
    pub const fn new(t: T) -> CachePadded<T> {
        CachePadded::<T> { value: t }
    }

    /// Returns the inner value.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T> Deref for CachePadded<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for CachePadded<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T: fmt::Debug> fmt::Debug for CachePadded<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CachePadded")
            .field("value", &self.value)
            .finish()
    }
}

impl<T> From<T> for CachePadded<T> {
    fn from(t: T) -> Self {
        CachePadded::new(t)
    }
}

impl<T: fmt::Display> fmt::Display for CachePadded<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.value, f)
    }
}

/// Stripe-sharded `AtomicU64` counters backed by cache-line padded lanes.
///
/// This helper spreads contention across multiple atomics. The number of
/// stripes must be a power-of-two so that callsites can cheaply select a lane
/// using a simple mask.
#[derive(Debug)]
pub struct StripedAtomicU64 {
    stripes: Box<[CachePadded<AtomicU64>]>,
    mask: usize,
}

impl StripedAtomicU64 {
    pub const DEFAULT_STRIPES: usize = 32;

    /// Creates a new striped counter with the provided number of lanes.
    ///
    /// # Panics
    ///
    /// Panics if `stripe_count` is zero or not a power of two.
    pub fn new(stripe_count: usize) -> Self {
        assert!(stripe_count > 0, "stripe count must be non-zero");
        assert!(
            stripe_count.is_power_of_two(),
            "stripe count must be a power of two"
        );

        let mut lanes = Vec::with_capacity(stripe_count);
        for _ in 0..stripe_count {
            lanes.push(CachePadded::new(AtomicU64::new(0)));
        }

        Self {
            stripes: lanes.into_boxed_slice(),
            mask: stripe_count - 1,
        }
    }

    /// Returns the number of stripes.
    #[inline(always)]
    pub fn stripes(&self) -> usize {
        self.stripes.len()
    }

    /// Returns the stripe index for a hint (typically a hash or thread id).
    #[inline(always)]
    pub fn stripe_for(&self, hint: usize) -> usize {
        hint & self.mask
    }

    /// Returns a snapshot of the stripe value at `index` using the provided ordering.
    #[inline(always)]
    pub fn get(&self, index: usize, ordering: Ordering) -> u64 {
        debug_assert!(index < self.stripes.len());
        self.stripes[index].load(ordering)
    }

    /// Increments the stripe chosen by `hint` by one using relaxed ordering.
    #[inline(always)]
    pub fn increment(&self, hint: usize) {
        self.fetch_add_hint(hint, 1, Ordering::Relaxed);
    }

    /// Decrements the stripe chosen by `hint` by one using relaxed ordering.
    #[inline(always)]
    pub fn decrement(&self, hint: usize) {
        self.fetch_sub_hint(hint, 1, Ordering::Relaxed);
    }

    /// Adds `value` to the stripe chosen by `hint`, returning the previous value.
    #[inline(always)]
    pub fn fetch_add_hint(&self, hint: usize, value: u64, ordering: Ordering) -> u64 {
        let idx = self.stripe_for(hint);
        self.fetch_add_index(idx, value, ordering)
    }

    /// Subtracts `value` from the stripe chosen by `hint`, returning the previous value.
    #[inline(always)]
    pub fn fetch_sub_hint(&self, hint: usize, value: u64, ordering: Ordering) -> u64 {
        let idx = self.stripe_for(hint);
        self.fetch_sub_index(idx, value, ordering)
    }

    /// Subtracts `value` from the stripe chosen by `hint`, saturating at zero.
    #[inline(always)]
    pub fn saturating_sub_hint(&self, hint: usize, value: u64, ordering: Ordering) -> u64 {
        let idx = self.stripe_for(hint);
        self.saturating_sub_index(idx, value, ordering)
    }

    /// Adds `value` to the stripe at `index`, returning the previous value.
    #[inline(always)]
    pub fn fetch_add_index(&self, index: usize, value: u64, ordering: Ordering) -> u64 {
        debug_assert!(index < self.stripes.len());
        self.stripes[index].fetch_add(value, ordering)
    }

    /// Subtracts `value` from the stripe at `index`, returning the previous value.
    #[inline(always)]
    pub fn fetch_sub_index(&self, index: usize, value: u64, ordering: Ordering) -> u64 {
        debug_assert!(index < self.stripes.len());
        self.stripes[index].fetch_sub(value, ordering)
    }

    /// Subtracts `value` from the stripe at `index`, saturating at zero.
    #[inline(always)]
    pub fn saturating_sub_index(&self, index: usize, value: u64, ordering: Ordering) -> u64 {
        debug_assert!(index < self.stripes.len());
        let stripe = &self.stripes[index];
        let mut current = stripe.load(Ordering::Relaxed);
        if current == 0 || value == 0 {
            return current;
        }
        loop {
            let next = current.saturating_sub(value);
            match stripe.compare_exchange(current, next, ordering, Ordering::Relaxed) {
                Ok(_) => return current,
                Err(observed) => {
                    if observed == 0 {
                        return 0;
                    }
                    current = observed;
                }
            }
        }
    }

    /// Returns the sum of all stripes using relaxed loads.
    #[inline]
    pub fn total(&self) -> u64 {
        self.stripes
            .iter()
            .map(|stripe| stripe.load(Ordering::Relaxed))
            .sum()
    }

    /// Returns a vector containing a relaxed snapshot of all stripes.
    #[inline]
    pub fn snapshot(&self) -> Vec<u64> {
        self.stripes
            .iter()
            .map(|stripe| stripe.load(Ordering::Relaxed))
            .collect()
    }

    /// Clears every stripe using the provided ordering and returns the aggregated total.
    #[inline]
    pub fn clear(&self, ordering: Ordering) -> u64 {
        self.stripes
            .iter()
            .map(|stripe| stripe.swap(0, ordering))
            .sum()
    }
}

impl Default for StripedAtomicU64 {
    fn default() -> Self {
        Self::new(Self::DEFAULT_STRIPES)
    }
}

#[cfg(test)]
mod tests {
    use super::StripedAtomicU64;
    use core::sync::atomic::Ordering;

    #[test]
    fn striped_atomic_u64_basic_accounting() {
        let counter = StripedAtomicU64::new(8);
        for i in 0..64 {
            counter.increment(i);
        }

        assert_eq!(counter.total(), 64);

        let snapshot = counter.snapshot();
        assert_eq!(snapshot.len(), 8);
        assert!(snapshot.iter().all(|value| *value == 8));

        let cleared = counter.clear(Ordering::AcqRel);
        assert_eq!(cleared, 64);
        assert_eq!(counter.total(), 0);
    }

    #[test]
    fn striped_atomic_u64_fetch_add_and_sub() {
        let counter = StripedAtomicU64::new(4);
        let idx = counter.stripe_for(7);
        counter.fetch_add_index(idx, 10, Ordering::AcqRel);
        counter.fetch_sub_index(idx, 4, Ordering::AcqRel);
        assert_eq!(counter.get(idx, Ordering::Acquire), 6);
    }
}
