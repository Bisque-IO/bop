use crate::bits;
use crate::runtime::signal::{SIGNAL_CAPACITY, SIGNAL_MASK};
use rand::RngCore;

const RND_MULTIPLIER: u64 = 0x5DEECE66D;
const RND_ADDEND: u64 = 0xB;
const RND_MASK: u64 = (1 << 48) - 1;

pub struct Selector {
    pub owned: i32,
    pub value: u64,
    pub block: u64,
    pub signal: u64,
    pub bit: u64,
    pub bit_count: u64,
    pub misses: u64,
    pub contention: u64,
    pub seed: u64,
    pub cached: *mut (),
}

impl Selector {
    pub fn new() -> Self {
        let seed: u64 = rand::rng().next_u64();
        let mut selector = Self {
            owned: 0,
            value: 0,
            block: 0,
            signal: 0,
            bit: 0,
            bit_count: 0,
            misses: 0,
            contention: 0,
            seed,
            cached: std::ptr::null_mut(),
        };
        selector.next_map();
        selector
    }

    pub fn next(&mut self) -> u64 {
        let old_seed = self.seed;
        let next_seed = (old_seed
            .wrapping_mul(RND_MULTIPLIER)
            .wrapping_add(RND_ADDEND))
            & RND_MASK;
        self.seed = next_seed;
        next_seed >> 16
    }

    pub fn next_map(&mut self) -> u64 {
        self.signal = self.next();
        self.bit = self.next() & SIGNAL_MASK;
        self.bit_count = 1;
        self.signal
    }

    pub fn next_select(&mut self) -> u64 {
        if self.bit_count >= SIGNAL_CAPACITY {
            self.next_map();
            return self.bit;
        }
        self.signal = self.next();
        self.bit = self.bit.wrapping_add(1);
        self.bit_count += 1;
        self.bit & SIGNAL_MASK
    }

    // pub fn next_map(&mut self) -> u64 {
    //     self.signal = self.signal + 1;
    //     self.bit = 0;
    //     self.bit_count = 1;
    //     self.signal
    // }
    //
    // pub fn next_select(&mut self) -> u64 {
    //     if self.bit_count >= SIGNAL_CAPACITY {
    //         self.next_map();
    //         return self.bit;
    //     }
    //     self.bit = self.bit.wrapping_add(1);
    //     self.bit_count += 1;
    //     self.bit_count
    // }

    /// Reset statistics counters
    pub fn reset_stats(&mut self) {
        self.misses = 0;
        self.contention = 0;
    }

    /// Get the miss rate (0.0 to 1.0)
    pub fn miss_rate(&self) -> f64 {
        let total = self.misses + self.contention + 1;
        self.misses as f64 / total as f64
    }

    /// Get the contention rate (0.0 to 1.0)
    pub fn contention_rate(&self) -> f64 {
        let total = self.misses + self.contention + 1;
        self.contention as f64 / total as f64
    }
}
