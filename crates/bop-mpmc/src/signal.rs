use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use crossbeam_utils::CachePadded;

use crate::SignalWaker;

pub const CACHE_LINE_SIZE: usize = 64;
pub const SIGNAL_CAPACITY: u64 = 64;
pub const SIGNAL_MASK: u64 = SIGNAL_CAPACITY - 1;
const SIGNAL_GROUP_WIDTH: usize = 8;
pub const SLOTS_PER_GROUP: usize = SIGNAL_GROUP_WIDTH * SIGNAL_CAPACITY as usize;

#[derive(Clone)]
pub struct Signal {
    inner: Arc<CachePadded<SignalInner>>,
}

impl Signal {
    #[inline(always)]
    pub fn index(&self) -> u64 {
        return self.inner.index;
    }

    #[inline(always)]
    pub fn value(&self) -> &AtomicU64 {
        &self.inner.value
    }
}

struct SignalInner {
    pub index: u64,
    value: AtomicU64,
}

impl Signal {
    pub fn new() -> Self {
        Self::with_value(0, 0)
    }

    pub fn with_index(index: u64) -> Self {
        Self::with_value(index, 0)
    }

    pub fn with_value(index: u64, value: u64) -> Self {
        Self {
            inner: Arc::new(CachePadded::new(SignalInner {
                index,
                value: AtomicU64::new(value),
            })),
        }
    }

    #[inline(always)]
    pub fn load(&self, ordering: Ordering) -> u64 {
        self.inner.value.load(ordering)
    }

    #[inline(always)]
    pub fn size(&self) -> u64 {
        self.load(Ordering::Relaxed).count_ones() as u64
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.load(Ordering::Relaxed).count_ones() == 0
    }

    #[inline(always)]
    pub fn set(&self, index: u64) -> (bool, bool) {
        crate::bits::set(&self.inner.value, index)
    }

    #[inline(always)]
    pub fn set_with_bit(&self, bit: u64) -> u64 {
        crate::bits::set_with_bit(&self.inner.value, bit)
    }

    #[inline(always)]
    pub fn acquire(&self, index: u64) -> bool {
        crate::bits::acquire(&self.inner.value, index)
    }

    #[inline(always)]
    pub fn try_acquire(&self, index: u64) -> (u64, u64, bool) {
        crate::bits::try_acquire(&self.inner.value, index)
    }

    #[inline(always)]
    pub fn is_set(&self, index: u64) -> bool {
        crate::bits::is_set(&self.inner.value, index)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SignalSetResult {
    pub was_empty: bool,
    pub was_set: bool,
}

pub const IDLE: u8 = 0;
pub const SCHEDULED: u8 = 1;
pub const EXECUTING: u8 = 2;

///
pub struct SignalGate {
    flags: AtomicU8,
    bit_index: u64,
    signal: Signal,
    waker: Arc<SignalWaker>,
}

impl SignalGate {
    pub fn new(bit_index: u64, signal: Signal, waker: Arc<SignalWaker>) -> Self {
        Self {
            flags: AtomicU8::new(IDLE),
            bit_index,
            signal,
            waker,
        }
    }

    #[inline(always)]
    pub fn schedule(&self) -> bool {
        if (self.flags.load(Ordering::Acquire) & SCHEDULED) != IDLE {
            return false;
        }

        let previous_flags = self.flags.fetch_or(SCHEDULED, Ordering::Release);
        let scheduled_nor_executing = (previous_flags & (SCHEDULED | EXECUTING)) == IDLE;

        if scheduled_nor_executing {
            let (was_empty, was_set) = self.signal.set(self.bit_index);
            if was_empty && was_set {
                self.waker.mark_active(self.signal.index());
            }
            true
        } else {
            false
        }
    }

    #[inline(always)]
    pub fn begin(&self) {
        self.flags.store(EXECUTING, Ordering::Release);
    }

    #[inline(always)]
    pub fn finish(&self) {
        let after_flags = self.flags.fetch_sub(EXECUTING, Ordering::AcqRel);
        if after_flags & SCHEDULED != IDLE {
            self.signal.set(self.bit_index);
        }
    }

    #[inline(always)]
    pub fn finish_and_schedule(&self) {
        self.flags.store(SCHEDULED, Ordering::Release);
        let (was_empty, was_set) = self.signal.set(self.bit_index);
        if was_empty && was_set {
            self.waker.mark_active(self.signal.index());
            // self.waker.increment();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
