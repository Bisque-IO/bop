use maniac::runtime::summary::Summary;
use maniac::runtime::waker::WorkerWaker;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

#[test]
fn summary_tree_notifies_partition_owner_on_activity() {
    let wakers: Vec<_> = (0..2).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(2));
    let tree = Summary::new(4, 1, &wakers, &worker_count);

    // Leaf 3 belongs to worker 1 (leaves {0,1} -> worker 0, {2,3} -> worker 1).
    assert!(tree.mark_signal_active(3, 0));

    let owner = &wakers[1];
    assert_eq!(
        owner.partition_summary(),
        1 << 1,
        "local index 1 (global leaf 3) should be marked active"
    );
    let (_, tasks_bit) = owner.status_bits();
    assert!(
        tasks_bit,
        "tasks bit must be raised when partition becomes non-empty"
    );

    // Mark another leaf for the same worker and confirm the bitmap tracks both.
    tree.mark_signal_active(2, 0);
    assert_eq!(owner.partition_summary(), (1 << 0) | (1 << 1));

    // Clearing a single leaf keeps the other bit set.
    tree.mark_signal_inactive(3, 0);
    assert_eq!(
        owner.partition_summary(),
        1 << 0,
        "clearing leaf 3 must keep leaf 2 marked"
    );
    let (_, tasks_bit) = owner.status_bits();
    assert!(
        tasks_bit,
        "tasks bit should stay set while any leaf in the partition is active"
    );

    // Clearing the final leaf should drop both partition summary and the status bit.
    tree.mark_signal_inactive(2, 0);
    assert_eq!(owner.partition_summary(), 0);
    let (_, tasks_bit) = owner.status_bits();
    assert!(
        !tasks_bit,
        "tasks bit must clear once the entire partition goes idle"
    );
}

#[test]
fn summary_tree_compute_partition_owner_matches_layout() {
    let wakers: Vec<_> = (0..3).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(3));
    let tree = Summary::new(7, 1, &wakers, &worker_count);

    // Layout with 7 leaves and 3 workers => first worker gets 3 leaves, others 2 each.
    assert_eq!(tree.compute_partition_owner(0, 3), 0);
    assert_eq!(tree.compute_partition_owner(1, 3), 0);
    assert_eq!(tree.compute_partition_owner(2, 3), 0);
    assert_eq!(tree.compute_partition_owner(3, 3), 1);
    assert_eq!(tree.compute_partition_owner(4, 3), 1);
    assert_eq!(tree.compute_partition_owner(5, 3), 2);
    assert_eq!(tree.compute_partition_owner(6, 3), 2);

    // Validate the local-index helper.
    assert_eq!(tree.global_to_local_leaf_idx(0, 0, 3), Some(0));
    assert_eq!(tree.global_to_local_leaf_idx(2, 0, 3), Some(2));
    assert_eq!(tree.global_to_local_leaf_idx(3, 0, 3), None);
    assert_eq!(tree.global_to_local_leaf_idx(3, 1, 3), Some(0));
    assert_eq!(tree.global_to_local_leaf_idx(6, 2, 3), Some(1));
}

#[test]
fn summary_tree_reservations_are_unique_and_release_cleanly() {
    let wakers: Vec<_> = (0..1).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(1));
    let tree = Summary::new(1, 1, &wakers, &worker_count);

    let mut slots = vec![];
    for _ in 0..64 {
        let bit = tree
            .reserve_task_in_leaf(0, 0)
            .expect("should reserve until all bits consumed");
        slots.push(bit);
    }
    assert!(
        tree.reserve_task_in_leaf(0, 0).is_none(),
        "all slots should be exhausted"
    );

    // Release everything and ensure a new reservation succeeds.
    for bit in &slots {
        tree.release_task_in_leaf(0, 0, *bit as usize);
    }
    assert!(
        tree.reserve_task_in_leaf(0, 0).is_some(),
        "reservation should succeed after releasing all slots"
    );
}

#[test]
fn summary_tree_mark_signal_active_is_idempotent() {
    let wakers: Vec<_> = (0..1).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(1));
    let tree = Summary::new(2, 1, &wakers, &worker_count);

    let owner = &wakers[0];
    assert_eq!(owner.permits(), 0);

    assert!(
        tree.mark_signal_active(1, 0),
        "first activation should report the leaf was empty"
    );
    assert_eq!(
        owner.partition_summary(),
        1 << 1,
        "owner partition summary should reflect the active leaf"
    );
    assert_eq!(
        owner.permits(),
        1,
        "partition going from empty→non-empty must release exactly one permit"
    );

    assert!(
        !tree.mark_signal_active(1, 0),
        "re-activating the same signal should report false"
    );
    assert_eq!(
        owner.permits(),
        1,
        "repeated activation should not release extra permits"
    );

    assert!(
        tree.mark_signal_inactive(1, 0),
        "clearing the final signal in the leaf should return true"
    );
    assert_eq!(owner.partition_summary(), 0);
    let (_, tasks_bit) = owner.status_bits();
    assert!(
        !tasks_bit,
        "tasks bit must drop once all signals in the partition are cleared"
    );
}

#[test]
fn summary_tree_mark_signal_inactive_only_when_leaf_empty() {
    let wakers: Vec<_> = (0..1).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(1));
    let tree = Summary::new(1, 2, &wakers, &worker_count);

    assert!(tree.mark_signal_active(0, 0));
    assert!(!tree.mark_signal_active(0, 1), "leaf already non-empty");

    // Clearing one of two active signals should not report empty.
    assert!(
        !tree.mark_signal_inactive(0, 0),
        "leaf should still be considered active with one signal remaining"
    );

    // Clearing the final signal should report empty.
    assert!(
        tree.mark_signal_inactive(0, 1),
        "leaf should report empty after last signal cleared"
    );
}
