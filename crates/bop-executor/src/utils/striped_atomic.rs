use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU64, Ordering};
use std::boxed::Box;
use std::vec::Vec;

use crate::CachePadded;
use std::{
	future::{Future, IntoFuture}
	,
	task::Wake
	,
};

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

    #[test]
    fn striped_atomic_u64_saturating_sub_does_not_underflow() {
        let counter = StripedAtomicU64::new(2);
        let idx = counter.stripe_for(1);
        counter.fetch_add_index(idx, 2, Ordering::Relaxed);
        let previous = counter.saturating_sub_index(idx, 10, Ordering::AcqRel);
        assert_eq!(previous, 2);
        assert_eq!(counter.get(idx, Ordering::Acquire), 0);
        let zero_prev = counter.saturating_sub_index(idx, 1, Ordering::AcqRel);
        assert_eq!(zero_prev, 0);
    }

    #[test]
    fn stripe_for_covers_all_indices() {
        let counter = StripedAtomicU64::new(8);
        let mut seen = [false; 8];
        for hint in 0..64 {
            let idx = counter.stripe_for(hint);
            seen[idx] = true;
        }
        assert!(seen.iter().all(|present| *present));
    }
}
