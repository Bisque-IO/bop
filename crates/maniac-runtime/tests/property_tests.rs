//! Property and fuzz-style tests covering Section 5 of the signalling plan.
use maniac_executor::runtime::summary::Summary;
use maniac_executor::runtime::waker::{STATUS_SUMMARY_BITS, WorkerWaker};
use rand::{Rng, SeedableRng, rngs::SmallRng};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Randomised sequences of producer/consumer events must keep the permit counter in lock-step
/// with the modelled number of 0→1 transitions minus acquisitions.
#[test]
fn property_signal_waker_permit_consistency() {
    const SEEDS: u64 = 32;
    const STEPS: usize = 512;

    for seed in 0..SEEDS {
        let mut rng = SmallRng::seed_from_u64(0x516E_A100_u64 ^ seed);
        let waker = WorkerWaker::new();
        let signals: Vec<AtomicU64> = (0..STATUS_SUMMARY_BITS as usize)
            .map(|_| AtomicU64::new(0))
            .collect();
        let mut active = vec![false; STATUS_SUMMARY_BITS as usize];
        let mut expected_permits = 0u64;

        for step in 0..STEPS {
            let op = rng.random_range(0..3);
            match op {
                0 => {
                    let idx = rng.random_range(0..STATUS_SUMMARY_BITS as usize);
                    let was_active = active[idx];
                    active[idx] = true;
                    signals[idx].store(1, Ordering::Release);
                    waker.mark_active(idx as u64);
                    if !was_active {
                        expected_permits += 1;
                    }
                }
                1 => {
                    let idx = rng.random_range(0..STATUS_SUMMARY_BITS as usize);
                    active[idx] = false;
                    signals[idx].store(0, Ordering::Release);
                    waker.try_unmark_if_empty(idx as u64, &signals[idx]);
                }
                _ => {
                    if waker.try_acquire() {
                        assert!(
                            expected_permits > 0,
                            "permit underflow: seed {seed} step {step}"
                        );
                        expected_permits -= 1;
                    }
                }
            }

            assert_eq!(
                waker.permits(),
                expected_permits,
                "permit mismatch: seed {seed} step {step} op {op}"
            );
        }

        while waker.try_acquire() {
            assert!(
                expected_permits > 0,
                "unexpected drain underflow for seed {seed}"
            );
            expected_permits -= 1;
        }

        assert_eq!(
            expected_permits, 0,
            "permits left after draining for seed {seed}"
        );
        assert_eq!(
            waker.permits(),
            0,
            "waker permits non-zero after drain for seed {seed}"
        );
    }
}

/// Differential property: Summary notifications plus explicit sync must never lose active
/// leaves, even as partitions resize while random leaves toggle.
#[test]
fn property_partition_hint_accuracy() {
    const SEEDS: u64 = 24;
    const STEPS: usize = 256;

    for seed in 0..SEEDS {
        let mut rng = SmallRng::seed_from_u64(0x5EED_4ACC_u64 ^ seed);
        let leaf_count = rng.random_range(1..=32);
        let signals_per_leaf = rng.random_range(1..=4);
        let worker_count = rng.random_range(1..=leaf_count.max(1).min(8));

        let wakers: Vec<Arc<WorkerWaker>> = (0..worker_count)
            .map(|_| Arc::new(WorkerWaker::new()))
            .collect();
        let worker_count_cell = AtomicUsize::new(worker_count);
        let tree = Summary::new(leaf_count, signals_per_leaf, &wakers, &worker_count_cell);

        let mut leaf_masks = vec![0u64; leaf_count];
        let leaf_words: Vec<AtomicU64> = (0..leaf_count).map(|_| AtomicU64::new(0)).collect();

        for step in 0..STEPS {
            let leaf_idx = rng.random_range(0..leaf_count);
            let signal_idx = rng.random_range(0..signals_per_leaf);
            let bit = 1u64 << signal_idx;

            if rng.random_bool(0.5) {
                tree.mark_signal_active(leaf_idx, signal_idx);
                leaf_masks[leaf_idx] |= bit;
            } else {
                tree.mark_signal_inactive(leaf_idx, signal_idx);
                leaf_masks[leaf_idx] &= !bit;
            }
            leaf_words[leaf_idx].store(leaf_masks[leaf_idx], Ordering::Release);

            for worker_id in 0..worker_count {
                let start = tree.partition_start_for_worker(worker_id, worker_count);
                let end = tree.partition_end_for_worker(worker_id, worker_count);
                let partition_len = end.saturating_sub(start);
                let expected_summary = (start..end).enumerate().fold(0u64, |acc, (local, idx)| {
                    if leaf_masks[idx] != 0 {
                        acc | (1u64 << local)
                    } else {
                        acc
                    }
                });
                let mask = if partition_len >= 64 {
                    u64::MAX
                } else {
                    (1u64 << partition_len) - 1
                };

                let waker = &wakers[worker_id];
                assert_eq!(
                    waker.partition_summary() & mask,
                    expected_summary,
                    "seed {seed} step {step} worker {worker_id} mismatch before sync"
                );
                assert_eq!(
                    waker.partition_summary() & !mask,
                    0,
                    "seed {seed} step {step} worker {worker_id} leaked bits"
                );
                let (_, tasks_bit) = waker.status_bits();
                assert_eq!(
                    tasks_bit,
                    expected_summary != 0,
                    "seed {seed} step {step} worker {worker_id} tasks bit drift"
                );
            }

            let worker_id = rng.random_range(0..worker_count);
            let start = tree.partition_start_for_worker(worker_id, worker_count);
            let end = tree.partition_end_for_worker(worker_id, worker_count);
            let has_work = wakers[worker_id].sync_partition_summary(start, end, &leaf_words);
            let partition_len = end.saturating_sub(start);
            let expected_summary = (start..end).enumerate().fold(0u64, |acc, (local, idx)| {
                if leaf_masks[idx] != 0 {
                    acc | (1u64 << local)
                } else {
                    acc
                }
            });
            let mask = if partition_len >= 64 {
                u64::MAX
            } else {
                (1u64 << partition_len) - 1
            };

            assert_eq!(
                has_work,
                expected_summary != 0,
                "seed {seed} step {step} worker {worker_id} inconsistent has_work"
            );
            assert_eq!(
                wakers[worker_id].partition_summary() & mask,
                expected_summary,
                "seed {seed} step {step} worker {worker_id} mismatch after sync"
            );
            let (_, tasks_bit) = wakers[worker_id].status_bits();
            assert_eq!(
                tasks_bit,
                expected_summary != 0,
                "seed {seed} step {step} worker {worker_id} tasks bit after sync"
            );
        }

        for worker_id in 0..worker_count {
            let start = tree.partition_start_for_worker(worker_id, worker_count);
            let end = tree.partition_end_for_worker(worker_id, worker_count);
            let has_work = wakers[worker_id].sync_partition_summary(start, end, &leaf_words);
            let expected_summary = (start..end).enumerate().fold(0u64, |acc, (local, idx)| {
                if leaf_masks[idx] != 0 {
                    acc | (1u64 << local)
                } else {
                    acc
                }
            });
            assert_eq!(
                has_work,
                expected_summary != 0,
                "seed {seed} final worker {worker_id} has_work mismatch"
            );
        }
    }
}

/// A fuzzier look at reserve-task fairness: every leaf must be revisited within a bounded window,
/// modelling that no partition can be starved while capacity exists.
#[test]
fn property_summary_tree_reserve_fairness() {
    const SEEDS: u64 = 16;

    for seed in 0..SEEDS {
        let mut rng = SmallRng::seed_from_u64(0xfa1f_f055_u64 ^ seed);
        let worker_count = rng.random_range(1..=4);
        let leaf_count = rng.random_range(worker_count..=worker_count * 4);
        let signals_per_leaf = rng.random_range(1..=4);

        let wakers: Vec<Arc<WorkerWaker>> = (0..worker_count)
            .map(|_| Arc::new(WorkerWaker::new()))
            .collect();
        let worker_count_cell = AtomicUsize::new(worker_count);
        let tree = Summary::new(leaf_count, signals_per_leaf, &wakers, &worker_count_cell);

        let window = leaf_count * signals_per_leaf * 2;
        let total_steps = window * 4;
        let mut last_seen = vec![None; leaf_count];
        let mut all_seen = false;

        for step in 0..total_steps {
            if let Some((leaf_idx, signal_idx, bit)) = tree.reserve_task() {
                last_seen[leaf_idx] = Some(step);
                tree.release_task_in_leaf(leaf_idx, signal_idx, bit as usize);
            }

            if !all_seen && last_seen.iter().all(|entry| entry.is_some()) {
                all_seen = true;
            }

            if all_seen {
                for (leaf, seen) in last_seen.iter().enumerate() {
                    let seen = seen.expect("leaf should have been seen once");
                    assert!(
                        step - seen <= window,
                        "leaf {leaf} starved for {} steps (seed {seed})",
                        step - seen
                    );
                }
            }
        }

        assert!(
            all_seen,
            "reserve_task never visited all leaves (seed {seed})"
        );
    }
}
