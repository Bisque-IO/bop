use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::slot::Slot;
use crate::waker::Waker;

pub const CACHE_LINE_SIZE: usize = 64;
pub const SIGNAL_CAPACITY: u64 = 64;
pub const SIGNAL_MASK: u64 = SIGNAL_CAPACITY - 1;
const SIGNAL_GROUP_WIDTH: usize = 8;
pub const SLOTS_PER_GROUP: usize = SIGNAL_GROUP_WIDTH * SIGNAL_CAPACITY as usize;

#[derive(Clone)]
pub struct Signal {
    inner: Arc<SignalInner>,
}

struct SignalInner {
    value: AtomicU64,
    _pad: [u8; 120],
}

impl Signal {
    pub fn new() -> Self {
        Self::with_value(0)
    }

    pub fn with_value(value: u64) -> Self {
        Self {
            inner: Arc::new(SignalInner {
                value: AtomicU64::new(value),
                _pad: [0; 120],
            }),
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
    pub fn set(&self, index: u64) -> SignalSetResult {
        let bit = 1u64 << index;
        let previous = self.inner.value.fetch_or(bit, Ordering::AcqRel);
        SignalSetResult {
            was_empty: previous == 0,
            was_set: (previous & bit) == 0,
        }
    }

    #[inline(always)]
    pub fn acquire(&self, index: u64) -> bool {
        let bit = 1u64 << index;
        let previous = self.inner.value.fetch_and(!bit, Ordering::AcqRel);
        (previous & bit) == bit
    }

    #[inline(always)]
    pub fn is_set(&self, index: u64) -> bool {
        let bit = 1u64 << index;
        (self.load(Ordering::Relaxed) & bit) != 0
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SignalSetResult {
    pub was_empty: bool,
    pub was_set: bool,
}

pub fn signal_nearest(value: u64, signal_index: u64) -> u64 {
    let up = (value >> signal_index).trailing_zeros() as u64;
    let down = reverse_bits(value << (63 - signal_index)).trailing_zeros() as u64;

    if up < down {
        signal_index.wrapping_add(up)
    } else if down < up {
        signal_index.wrapping_sub(down)
    } else {
        signal_index
    }
}

pub fn signal_nearest_branchless(value: u64, signal_index: u64) -> u64 {
    let up = (value >> signal_index).trailing_zeros() as u64;
    let down = reverse_bits(value << (63 - signal_index)).trailing_zeros() as u64;

    let up_lt_down = (up < down) as u64;
    let down_lt_up = (down < up) as u64;
    let equal = 1 - (up_lt_down | down_lt_up);

    let up_mask = 0u64.wrapping_sub(up_lt_down);
    let down_mask = 0u64.wrapping_sub(down_lt_up);
    let same_mask = 0u64.wrapping_sub(equal);

    let up_result = signal_index.wrapping_add(up);
    let down_result = signal_index.wrapping_sub(down);
    let same_result = signal_index;

    (up_result & up_mask) | (down_result & down_mask) | (same_result & same_mask)
}

fn reverse_bits(value: u64) -> u64 {
    value.reverse_bits()
}

#[allow(dead_code)]
pub struct SignalGroup {
    signals: [Signal; SIGNAL_GROUP_WIDTH],
    freelist: [AtomicU64; SIGNAL_GROUP_WIDTH],
    waker: Arc<Waker>,
    counter: AtomicU64,
    #[allow(dead_code)]
    slots: Vec<Arc<Slot>>,
    initialized: AtomicBool,
}

impl SignalGroup {
    pub fn new() -> Self {
        let waker = Arc::new(Waker::new());
        let signals = std::array::from_fn(|_| Signal::new());
        let freelist = std::array::from_fn(|_| AtomicU64::new(u64::MAX));
        let slots = Vec::with_capacity(SLOTS_PER_GROUP);

        let mut group = Self {
            signals,
            freelist,
            waker,
            counter: AtomicU64::new(0),
            slots,
            initialized: AtomicBool::new(false),
        };
        group.initialize_slots();
        group
    }

    fn initialize_slots(&mut self) {
        if self.initialized.swap(true, Ordering::Relaxed) {
            return;
        }

        let mut slots = Vec::with_capacity(SLOTS_PER_GROUP);
        for signal in &self.signals {
            for bit in 0..SIGNAL_CAPACITY {
                slots.push(Arc::new(Slot::new(self.waker.clone(), signal.clone(), bit)));
            }
        }
        self.slots = slots;
    }

    pub fn signals(&self) -> &[Signal] {
        &self.signals
    }

    pub fn waker(&self) -> &Arc<Waker> {
        &self.waker
    }

    pub fn fast_check(&self) -> bool {
        self.signals
            .iter()
            .any(|signal| signal.load(Ordering::Relaxed) != 0)
    }

    pub fn reserve(&self) -> Option<Arc<Slot>> {
        // TODO: Implement freelist-aware reservation.
        self.slots.get(0).cloned()
    }
}

impl Default for SignalGroup {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branchless_matches_branchy() {
        let samples = [
            0u64,
            1u64,
            0x8000_0000_0000_0000,
            0xFFFF_FFFF_FFFF_FFFF,
            0x00FF_00FF_00FF_00FF,
            0x0F0F_0F0F_0F0F_0F0F,
        ];

        for &value in &samples {
            for idx in 0..64 {
                let branchy = signal_nearest(value, idx);
                let branchless = signal_nearest_branchless(value, idx);
                assert_eq!(branchless, branchy, "value={value:#x} idx={idx}");
            }
        }
    }
}
