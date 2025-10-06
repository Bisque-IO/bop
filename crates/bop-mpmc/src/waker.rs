use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crossbeam_utils::CachePadded;

/// Summary-driven, counting waker:
/// - `summary`: 1 bit per word (word has any signals set)
/// - `permits`: accumulated wake tokens (no lost wakeups)
/// - `sleepers`: approximate parked threads (to avoid over-notify)
#[repr(align(64))]
pub struct SignalWaker {
    /// Bit i set => word i may be non-zero (64 words → 4096 queues).
    summary: CachePadded<AtomicU64>,

    /// Counting semaphore: how many threads should be awake (permits).
    permits: CachePadded<AtomicU64>,

    /// Approximate number of parked threads (best-effort).
    sleepers: CachePadded<AtomicUsize>,
    m: Mutex<()>,
    cv: Condvar,
}

impl SignalWaker {
    pub fn new() -> Self {
        Self {
            summary: CachePadded::new(AtomicU64::new(0)),
            permits: CachePadded::new(AtomicU64::new(0)),
            sleepers: CachePadded::new(AtomicUsize::new(0)),
            m: Mutex::new(()),
            cv: Condvar::new(),
        }
    }

    // -------------------- PRODUCER-SIDE API --------------------

    /// Mark a specific signal at `index` (0..63) as active (transition 0→1 yields one permit).
    #[inline]
    pub fn mark_active(&self, index: u64) {
        let mask = 1u64 << index;
        let prev = self.summary.fetch_or(mask, Ordering::Relaxed);
        if prev & mask == 0 {
            self.release(1);
        }
    }

    /// Batch: publish multiple words; wake exactly the number of *newly* active words.
    /// `mask` has 1s for words you know became non-zero (e.g., from old==0 checks).
    #[inline]
    pub fn mark_active_mask(&self, mask: u64) {
        if mask == 0 {
            return;
        }
        let prev = self.summary.fetch_or(mask, Ordering::Relaxed);
        let newly = (!prev) & mask;
        let k = newly.count_ones() as usize;
        if k > 0 {
            self.release(k);
        }
    }

    // -------------------- CONSUMER-SIDE API --------------------

    /// Snapshot current summary (hint for which words to try first).
    #[inline]
    pub fn snapshot_summary(&self) -> u64 {
        self.summary.load(Ordering::Relaxed)
    }

    /// After draining a word, attempt to clear its summary bit if the word is now zero.
    /// Call this lazily; it's okay to leave false positives.
    #[inline]
    pub fn try_clear_word_bit_if_empty(&self, w: u64, signal: &AtomicU64) {
        if signal.load(Ordering::Relaxed) == 0 {
            self.summary.fetch_and(!(1u64 << w), Ordering::Relaxed);
        }
    }

    // -------------------- PERMIT / PARKING --------------------

    /// Fast non-blocking acquire; returns true if a permit was consumed.
    #[inline]
    pub fn try_acquire(&self) -> bool {
        self.permits
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |p| p.checked_sub(1))
            .is_ok()
    }

    /// Blocking acquire: park only if no permit is available.
    pub fn acquire(&self) {
        if self.try_acquire() {
            return;
        }
        let mut g = self.m.lock().expect("waker mutex poisoned");
        self.sleepers.fetch_add(1, Ordering::Relaxed);
        loop {
            if self.try_acquire() {
                self.sleepers.fetch_sub(1, Ordering::Relaxed);
                return;
            }
            g = self.cv.wait(g).expect("waker condvar wait poisoned");
        }
    }

    /// Acquire with timeout; returns true if acquired, false on timeout.
    pub fn acquire_timeout(&self, timeout: Duration) -> bool {
        if self.try_acquire() {
            return true;
        }
        let start = std::time::Instant::now();
        let mut g = self.m.lock().expect("waker mutex poisoned");
        self.sleepers.fetch_add(1, Ordering::Relaxed);
        while start.elapsed() < timeout {
            if self.try_acquire() {
                self.sleepers.fetch_sub(1, Ordering::Relaxed);
                return true;
            }
            let left = timeout.saturating_sub(start.elapsed());
            let (gg, res) = self
                .cv
                .wait_timeout(g, left)
                .expect("waker condvar wait poisoned");
            g = gg;
            if res.timed_out() {
                break;
            }
        }
        self.sleepers.fetch_sub(1, Ordering::Relaxed);
        false
    }

    /// Add `n` permits and wake up to `n` sleepers (no broadcast).
    #[inline]
    pub fn release(&self, n: usize) {
        if n == 0 {
            return;
        }
        self.permits.fetch_add(n as u64, Ordering::Release);
        let to_wake = n.min(self.sleepers.load(Ordering::Relaxed));
        for _ in 0..to_wake {
            self.cv.notify_one();
        }
    }

    // -------------------- INSPECTION --------------------

    #[inline]
    pub fn summary_bits(&self) -> u64 {
        self.summary.load(Ordering::Acquire)
    }
    #[inline]
    pub fn permits(&self) -> u64 {
        self.permits.load(Ordering::Acquire)
    }
    #[inline]
    pub fn sleepers(&self) -> usize {
        self.sleepers.load(Ordering::Relaxed)
    }
}

impl Default for SignalWaker {
    fn default() -> Self {
        Self::new()
    }
}
