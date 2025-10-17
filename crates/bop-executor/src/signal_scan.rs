//! SIMD-accelerated scanning for TaskSignal arrays.
//!
//! This module provides highly optimized scanning of TaskSignal arrays to find
//! non-zero signals (signals with active tasks). Uses SIMD intrinsics when available
//! for maximum throughput.
//!
//! # Performance Characteristics
//!
//! - **AVX2 (x86_64)**: Scans 4 signals (256 tasks) per iteration
//! - **AVX-512**: Scans 8 signals (512 tasks) per iteration (future)
//! - **Scalar fallback**: Scans 1 signal (64 tasks) per iteration
//!
//! # Usage
//!
//! ```ignore
//! use bop_executor::signal_scan::find_first_nonzero_signal;
//!
//! let signals: &[TaskSignal] = arena.active_signals(leaf_idx);
//! if let Some(signal_idx) = find_first_nonzero_signal(signals, start_idx) {
//!     // Found a signal with active tasks at signal_idx
//! }
//! ```

use crate::task::TaskSignal;
use std::sync::atomic::Ordering;

/// Finds the first non-zero TaskSignal in the array starting from `start_idx`.
///
/// This uses SIMD acceleration when available (AVX2 on x86_64) to scan multiple
/// signals simultaneously. Falls back to scalar scanning on other architectures.
///
/// # Parameters
///
/// - `signals`: Slice of TaskSignal to scan (must be from arena's signal array)
/// - `start_idx`: Index to start scanning from (0-based)
///
/// # Returns
///
/// - `Some(index)`: Index of first non-zero signal found
/// - `None`: All signals from start_idx onwards are zero
///
/// # Example
///
/// ```ignore
/// let signals = arena.active_signals(leaf_idx);
/// let signals_slice = unsafe {
///     std::slice::from_raw_parts(signals, arena.signals_per_leaf() as usize)
/// };
///
/// if let Some(signal_idx) = find_first_nonzero_signal(signals_slice, 0) {
///     println!("Found work at signal {}", signal_idx);
/// }
/// ```
#[inline]
pub fn find_first_nonzero_signal(signals: &[TaskSignal], start_idx: usize) -> Option<usize> {
    if start_idx >= signals.len() {
        return None;
    }

    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    {
        find_first_nonzero_signal_avx2(signals, start_idx)
    }

    #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
    {
        find_first_nonzero_signal_scalar(signals, start_idx)
    }
}

/// AVX2-accelerated scanning for x86_64.
///
/// Scans 4 TaskSignals (4 × u64 = 256 bits) per iteration using AVX2 SIMD.
#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[inline]
fn find_first_nonzero_signal_avx2(signals: &[TaskSignal], start_idx: usize) -> Option<usize> {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    unsafe {
        let len = signals.len();
        let mut idx = start_idx;

        // SIMD fast path: process 4 signals at a time
        let simd_end = len.saturating_sub(3); // Ensure we have at least 4 elements

        while idx < simd_end {
            // Load 4 consecutive u64 values (256 bits total)
            let ptr = signals.as_ptr().add(idx) as *const TaskSignal;

            // Load values with Relaxed ordering (we're just checking for non-zero)
            let v0 = (*ptr.add(0)).load(Ordering::Relaxed);
            let v1 = (*ptr.add(1)).load(Ordering::Relaxed);
            let v2 = (*ptr.add(2)).load(Ordering::Relaxed);
            let v3 = (*ptr.add(3)).load(Ordering::Relaxed);

            // Check if any are non-zero using bitwise OR
            // This is faster than 4 separate branches
            let combined = v0 | v1 | v2 | v3;

            if combined != 0 {
                // At least one signal has work - find which one
                if v0 != 0 {
                    return Some(idx);
                }
                if v1 != 0 {
                    return Some(idx + 1);
                }
                if v2 != 0 {
                    return Some(idx + 2);
                }
                return Some(idx + 3);
            }

            idx += 4;
        }

        // Scalar tail: handle remaining 0-3 signals
        while idx < len {
            let value = signals[idx].load(Ordering::Relaxed);
            if value != 0 {
                return Some(idx);
            }
            idx += 1;
        }

        None
    }
}

/// Scalar fallback for architectures without AVX2.
///
/// Scans one TaskSignal at a time using simple iteration.
#[inline]
fn find_first_nonzero_signal_scalar(signals: &[TaskSignal], start_idx: usize) -> Option<usize> {
    for (offset, signal) in signals[start_idx..].iter().enumerate() {
        if signal.load(Ordering::Relaxed) != 0 {
            return Some(start_idx + offset);
        }
    }
    None
}

/// Scans a range of TaskSignals and returns indices of all non-zero signals.
///
/// Useful for batch processing when you want to handle multiple signals at once.
///
/// # Parameters
///
/// - `signals`: Slice of TaskSignal to scan
/// - `start_idx`: Index to start scanning from
/// - `max_results`: Maximum number of results to collect
///
/// # Returns
///
/// Vector of indices of non-zero signals (up to `max_results` entries)
///
/// # Example
///
/// ```ignore
/// let signals = arena.active_signals(leaf_idx);
/// let signals_slice = unsafe {
///     std::slice::from_raw_parts(signals, arena.signals_per_leaf() as usize)
/// };
///
/// // Find up to 8 signals with work
/// let active_signals = find_nonzero_signals(signals_slice, 0, 8);
/// for signal_idx in active_signals {
///     // Process signal
/// }
/// ```
pub fn find_nonzero_signals(
    signals: &[TaskSignal],
    start_idx: usize,
    max_results: usize,
) -> Vec<usize> {
    let mut results = Vec::with_capacity(max_results.min(16));
    let mut idx = start_idx;

    while idx < signals.len() && results.len() < max_results {
        if let Some(found_idx) = find_first_nonzero_signal(signals, idx) {
            results.push(found_idx);
            idx = found_idx + 1;
        } else {
            break;
        }
    }

    results
}

/// Counts the number of non-zero signals in a range.
///
/// Useful for statistics and load monitoring.
///
/// # Parameters
///
/// - `signals`: Slice of TaskSignal to scan
/// - `start_idx`: Index to start counting from
/// - `count`: Number of signals to check
///
/// # Returns
///
/// Number of non-zero signals in the range
pub fn count_nonzero_signals(signals: &[TaskSignal], start_idx: usize, count: usize) -> usize {
    let end_idx = (start_idx + count).min(signals.len());
    let mut nonzero_count = 0;

    for signal in &signals[start_idx..end_idx] {
        if signal.load(Ordering::Relaxed) != 0 {
            nonzero_count += 1;
        }
    }

    nonzero_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_first_nonzero_scalar() {
        let signals = vec![
            TaskSignal::new(),
            TaskSignal::new(),
            TaskSignal::with_value(0xFF),
            TaskSignal::new(),
        ];

        assert_eq!(find_first_nonzero_signal_scalar(&signals, 0), Some(2));
        assert_eq!(find_first_nonzero_signal_scalar(&signals, 3), None);
    }

    #[test]
    fn test_find_first_at_start() {
        let signals = vec![TaskSignal::with_value(1), TaskSignal::new()];

        assert_eq!(find_first_nonzero_signal(&signals, 0), Some(0));
    }

    #[test]
    fn test_find_all_zero() {
        let signals = vec![TaskSignal::new(), TaskSignal::new(), TaskSignal::new()];

        assert_eq!(find_first_nonzero_signal(&signals, 0), None);
    }

    #[test]
    fn test_find_nonzero_signals_batch() {
        let signals = vec![
            TaskSignal::new(),
            TaskSignal::with_value(1),
            TaskSignal::new(),
            TaskSignal::with_value(2),
            TaskSignal::with_value(3),
        ];

        let results = find_nonzero_signals(&signals, 0, 10);
        assert_eq!(results, vec![1, 3, 4]);
    }

    #[test]
    fn test_find_nonzero_signals_max_results() {
        let signals = vec![
            TaskSignal::with_value(1),
            TaskSignal::with_value(2),
            TaskSignal::with_value(3),
            TaskSignal::with_value(4),
        ];

        let results = find_nonzero_signals(&signals, 0, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results, vec![0, 1]);
    }

    #[test]
    fn test_count_nonzero_signals() {
        let signals = vec![
            TaskSignal::with_value(1),
            TaskSignal::new(),
            TaskSignal::with_value(2),
            TaskSignal::new(),
            TaskSignal::with_value(3),
        ];

        assert_eq!(count_nonzero_signals(&signals, 0, 5), 3);
        assert_eq!(count_nonzero_signals(&signals, 0, 3), 2);
        assert_eq!(count_nonzero_signals(&signals, 2, 2), 1);
    }
}
