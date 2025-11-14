use std::cell::UnsafeCell;

use rand::RngCore;

const RND_MULTIPLIER: u64 = 0x5DEECE66D;
const RND_ADDEND: u64 = 0xB;
const RND_MASK: u64 = (1 << 48) - 1;

pub struct Random {
    seed: u64,
}

fn now_nanos() -> u64 {
    // Return the current time as the number of nanoseconds since the Unix epoch as a u64
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time went backwards");
    now.as_nanos() as u64
}


thread_local! {
    static THREAD_RND: UnsafeCell<Random> = UnsafeCell::new(Random { seed: now_nanos() });
}

pub fn random_u64() -> u64 {
    THREAD_RND.with(|r| unsafe { &mut *r.get() }.next())
}

pub fn random_usize() -> usize {
    THREAD_RND.with(|r| unsafe { &mut *r.get() }.next()) as usize
}

impl Random {
    pub fn seed(&self) -> u64 {
        self.seed
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
}

