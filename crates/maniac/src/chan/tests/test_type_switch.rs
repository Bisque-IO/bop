use crate::chan::tests::common::{async_join_result, async_spawn, runtime_block_on, timeout};
use crate::chan::*;
use std::time::Duration;

// Macro to wrap tests with a 5-second timeout
macro_rules! runtime_block_on_with_timeout {
    ($async_block:expr) => {{
        runtime_block_on!(async move {
            timeout(Duration::from_secs(5), $async_block)
                .await
                .expect("Test timed out after 5 seconds")
        })
    }};
}

// ============================================================================
// test_bounded_async_with_sync_receiver_switch_buffered
// ============================================================================

fn _test_bounded_async_with_sync_receiver_switch_buffered<T: AsyncTxTrait<usize> + 'static>(
    channel: (T, AsyncRx<usize>),
) {
    let (tx, rx) = channel;
    let total_messages = 20;
    let async_consumed = 4;

    runtime_block_on_with_timeout!(async move {
        let sender_task = async_spawn!(async move {
            for i in 0..total_messages {
                tx.send(i).await.expect("Failed to send message");
            }
        });

        let receiver_task = async_spawn!(async move {
            let mut async_received = Vec::new();
            for _ in 0..async_consumed {
                match rx.recv().await {
                    Ok(value) => {
                        async_received.push(value);
                    }
                    Err(e) => {
                        panic!("Failed to receive message: {:?}", e);
                    }
                }
            }
            (rx, async_received)
        });

        let (rx, async_received) = async_join_result!(receiver_task);
        let blocking_rx: Rx<usize> = rx.into();

        let remaining_messages = total_messages - async_consumed;
        let sync_th = std::thread::spawn(move || {
            let mut sync_received = Vec::new();
            while let Ok(value) = blocking_rx.recv() {
                sync_received.push(value);
            }
            sync_received
        });

        let _ = sender_task.await;
        let sync_received = sync_th.join().expect("Sync receiver thread panicked");

        assert_eq!(async_received.len(), async_consumed);
        assert_eq!(sync_received.len(), remaining_messages);

        let mut all_received = async_received;
        all_received.extend(sync_received);
        assert_eq!(all_received.len(), total_messages);

        for i in 0..total_messages {
            assert!(all_received.contains(&i), "Missing value: {}", i);
        }
    });
}

#[test]
fn test_bounded_async_with_sync_receiver_switch_buffered_spsc() {
    _test_bounded_async_with_sync_receiver_switch_buffered(spsc::bounded_async::<usize>(5));
}

#[test]
fn test_bounded_async_with_sync_receiver_switch_buffered_mpsc() {
    _test_bounded_async_with_sync_receiver_switch_buffered(mpsc::bounded_async::<usize>(5));
}

// ============================================================================
// test_mpmc_bounded_async_with_sync_receiver_switch_buffered
// ============================================================================

#[test]
fn test_mpmc_bounded_async_with_sync_receiver_switch_buffered() {
    let (tx, rx) = mpmc::bounded_async::<usize>(5);
    let total_messages = 20;
    let async_consumed = 4;

    runtime_block_on_with_timeout!(async move {
        let sender_task = async_spawn!(async move {
            for i in 0..total_messages {
                tx.send(i).await.expect("Failed to send message");
            }
        });

        let receiver_task = async_spawn!(async move {
            let mut async_received = Vec::new();
            for _ in 0..async_consumed {
                match rx.recv().await {
                    Ok(value) => {
                        async_received.push(value);
                    }
                    Err(e) => {
                        panic!("Failed to receive message with async receiver: {:?}", e);
                    }
                }
            }
            (rx, async_received)
        });

        let (rx, async_received) = async_join_result!(receiver_task);
        let sync_rx: MRx<usize> = rx.into();

        let remaining_messages = total_messages - async_consumed;
        let sync_th = std::thread::spawn(move || {
            let mut sync_received = Vec::new();
            while let Ok(value) = sync_rx.recv() {
                sync_received.push(value);
            }
            sync_received
        });

        let _ = sender_task.await;
        let sync_received = sync_th.join().expect("Sync receiver thread panicked");

        assert_eq!(async_received.len(), async_consumed);
        assert_eq!(sync_received.len(), remaining_messages);

        let mut all_received = async_received;
        all_received.extend(sync_received);
        assert_eq!(all_received.len(), total_messages);

        for i in 0..total_messages {
            assert!(all_received.contains(&i), "Missing value: {}", i);
        }
    });
}

// ============================================================================
// test_spsc_bounded_blocking_with_async_sender_switch
// ============================================================================

#[test]
fn test_spsc_bounded_blocking_with_async_sender_switch() {
    let (tx, rx) = spsc::bounded_blocking::<usize>(5);
    let total_messages = 20;
    let sync_sent = 4;

    runtime_block_on_with_timeout!(async move {
        let receiver_handle = std::thread::spawn(move || {
            let mut all_received = Vec::new();
            while let Ok(value) = rx.recv() {
                all_received.push(value);
            }
            all_received
        });

        let sender_handle = std::thread::spawn(move || {
            for i in 0..sync_sent {
                tx.send(i).expect("Failed to send message");
            }
            tx
        });

        let tx = sender_handle.join().expect("Sender thread panicked");
        let async_tx: AsyncTx<usize> = tx.into();

        let async_sender_task = async_spawn!(async move {
            for i in sync_sent..total_messages {
                async_tx.send(i).await.expect("Failed to send message");
            }
        });

        let _ = async_sender_task.await;
        let all_received = receiver_handle
            .join()
            .expect("Final receiver thread panicked");

        assert_eq!(all_received.len(), total_messages);
        for i in 0..total_messages {
            assert!(all_received.contains(&i), "Missing value: {}", i);
        }
    });
}

// ============================================================================
// test_mpsc_bounded_blocking_with_async_sender_switch
// ============================================================================

#[test]
fn test_mpsc_bounded_blocking_with_async_sender_switch() {
    let (tx, rx) = mpsc::bounded_blocking::<usize>(5);
    let total_messages = 20;
    let sync_sent = 4;

    runtime_block_on_with_timeout!(async move {
        let sender_handle = std::thread::spawn(move || {
            for i in 0..sync_sent {
                tx.send(i).expect("Failed to send message");
            }
            tx
        });

        let tx = sender_handle.join().expect("Sender thread panicked");
        let async_tx: MAsyncTx<usize> = tx.into();

        let async_sender_task = async_spawn!(async move {
            for i in sync_sent..total_messages {
                async_tx.send(i).await.expect("Failed to send message");
            }
        });

        let receiver_handle = std::thread::spawn(move || {
            let mut all_received = Vec::new();
            while let Ok(value) = rx.recv() {
                all_received.push(value);
            }
            all_received
        });

        let _ = async_sender_task.await;
        let all_received = receiver_handle.join().expect("Receiver thread panicked");

        assert_eq!(all_received.len(), total_messages);
        for i in 0..total_messages {
            assert!(all_received.contains(&i), "Missing value: {}", i);
        }
    });
}

// ============================================================================
// test_mpmc_bounded_blocking_with_async_sender_switch
// ============================================================================

#[test]
fn test_mpmc_bounded_blocking_with_async_sender_switch() {
    let (tx, rx) = mpmc::bounded_blocking::<usize>(5);
    let total_messages = 20;
    let sync_sent = 4;

    runtime_block_on_with_timeout!(async move {
        let sender_handle = std::thread::spawn(move || {
            for i in 0..sync_sent {
                tx.send(i).expect("Failed to send message");
            }
            tx
        });

        let tx = sender_handle.join().expect("Sender thread panicked");
        let async_tx: MAsyncTx<usize> = tx.into();

        let async_sender_task = async_spawn!(async move {
            for i in sync_sent..total_messages {
                async_tx.send(i).await.expect("Failed to send message");
            }
        });

        let receiver_handle = std::thread::spawn(move || {
            let mut all_received = Vec::new();
            while let Ok(value) = rx.recv() {
                all_received.push(value);
            }
            all_received
        });

        let _ = async_sender_task.await;
        let all_received = receiver_handle.join().expect("Receiver thread panicked");

        assert_eq!(all_received.len(), total_messages);
        for i in 0..total_messages {
            assert!(all_received.contains(&i), "Missing value: {}", i);
        }
    });
}

// ============================================================================
// test_spsc_bounded_blocking_with_async_receiver_switch
// ============================================================================

#[test]
fn test_spsc_bounded_blocking_with_async_receiver_switch() {
    let (tx, rx) = spsc::bounded_blocking::<usize>(5);
    let total_messages = 20;
    let sync_consumed = 4;

    runtime_block_on_with_timeout!(async move {
        let sender_handle = std::thread::spawn(move || {
            for i in 0..total_messages {
                tx.send(i).expect("Failed to send message");
            }
        });

        let receiver_handle = std::thread::spawn(move || {
            let mut sync_received = Vec::new();
            for _ in 0..sync_consumed {
                sync_received.push(rx.recv().expect("Failed to receive message"));
            }
            (rx, sync_received)
        });

        let (rx, sync_received) = receiver_handle.join().expect("Receiver thread panicked");
        let async_rx: AsyncRx<usize> = rx.into();

        let async_receiver_task = async_spawn!(async move {
            let mut async_received = Vec::new();
            while let Ok(value) = async_rx.recv().await {
                async_received.push(value);
            }
            async_received
        });

        let async_received = async_join_result!(async_receiver_task);
        sender_handle.join().expect("Sender thread panicked");

        let remaining_messages = total_messages - sync_consumed;
        assert_eq!(sync_received.len(), sync_consumed);
        assert_eq!(async_received.len(), remaining_messages);

        let mut all_received = sync_received;
        all_received.extend(async_received);
        assert_eq!(all_received.len(), total_messages);

        for i in 0..total_messages {
            assert!(all_received.contains(&i), "Missing value: {}", i);
        }
    });
}

// ============================================================================
// test_mpsc_bounded_blocking_with_async_receiver_switch
// ============================================================================

#[test]
fn test_mpsc_bounded_blocking_with_async_receiver_switch() {
    let (tx, rx) = mpsc::bounded_blocking::<usize>(5);
    let total_messages = 20;
    let sync_consumed = 4;

    runtime_block_on_with_timeout!(async move {
        let sender_handle = std::thread::spawn(move || {
            for i in 0..total_messages {
                tx.send(i).expect("Failed to send message");
            }
        });

        let receiver_handle = std::thread::spawn(move || {
            let mut sync_received = Vec::new();
            for _ in 0..sync_consumed {
                sync_received.push(rx.recv().expect("Failed to receive message"));
            }
            (rx, sync_received)
        });

        let (rx, sync_received) = receiver_handle.join().expect("Receiver thread panicked");
        let async_rx: AsyncRx<usize> = rx.into();

        let async_receiver_task = async_spawn!(async move {
            let mut async_received = Vec::new();
            while let Ok(value) = async_rx.recv().await {
                async_received.push(value);
            }
            async_received
        });

        let async_received = async_join_result!(async_receiver_task);
        sender_handle.join().expect("Sender thread panicked");

        let remaining_messages = total_messages - sync_consumed;
        assert_eq!(sync_received.len(), sync_consumed);
        assert_eq!(async_received.len(), remaining_messages);

        let mut all_received = sync_received;
        all_received.extend(async_received);
        assert_eq!(all_received.len(), total_messages);

        for i in 0..total_messages {
            assert!(all_received.contains(&i), "Missing value: {}", i);
        }
    });
}

// ============================================================================
// test_mpmc_bounded_blocking_with_async_receiver_switch
// ============================================================================

#[test]
fn test_mpmc_bounded_blocking_with_async_receiver_switch() {
    let (tx, rx) = mpmc::bounded_blocking::<usize>(5);
    let total_messages = 20;
    let sync_consumed = 4;

    runtime_block_on_with_timeout!(async move {
        let sender_handle = std::thread::spawn(move || {
            for i in 0..total_messages {
                tx.send(i).expect("Failed to send message");
            }
        });

        let receiver_handle = std::thread::spawn(move || {
            let mut sync_received = Vec::new();
            for _ in 0..sync_consumed {
                sync_received.push(rx.recv().expect("Failed to receive message"));
            }
            (rx, sync_received)
        });

        let (rx, sync_received) = receiver_handle.join().expect("Receiver thread panicked");
        let async_rx: MAsyncRx<usize> = rx.into();

        let async_receiver_task = async_spawn!(async move {
            let mut async_received = Vec::new();
            while let Ok(value) = async_rx.recv().await {
                async_received.push(value);
            }
            async_received
        });

        let async_received = async_join_result!(async_receiver_task);
        sender_handle.join().expect("Sender thread panicked");

        let mut all_received = sync_received;
        all_received.extend(async_received);
        assert_eq!(all_received.len(), total_messages);

        for i in 0..total_messages {
            assert!(all_received.contains(&i), "Missing value: {}", i);
        }
    });
}

// ============================================================================
// test_multi_producer_sender_switch
// ============================================================================

fn _test_multi_producer_sender_switch<R: BlockingRxTrait<usize> + 'static>(
    channel: (MTx<usize>, R),
) {
    let (tx, rx) = channel;
    let total_messages = 20;
    let sync_sent = 4;

    runtime_block_on_with_timeout!(async move {
        let receiver_handle = std::thread::spawn(move || {
            let mut all_received = Vec::new();
            while let Ok(value) = rx.recv() {
                all_received.push(value);
            }
            all_received
        });

        let sender_handle = std::thread::spawn(move || {
            for i in 0..sync_sent {
                tx.send(i).expect("Failed to send message");
            }
            tx
        });

        let tx = sender_handle.join().expect("Sender thread panicked");
        let async_tx: MAsyncTx<usize> = tx.into();

        let async_sender_task = async_spawn!(async move {
            for i in sync_sent..total_messages {
                async_tx.send(i).await.expect("Failed to send message");
            }
        });

        let _ = async_sender_task.await;
        let all_received = receiver_handle.join().expect("Receiver thread panicked");

        assert_eq!(all_received.len(), total_messages);
        for i in 0..total_messages {
            assert!(all_received.contains(&i), "Missing value: {}", i);
        }
    });
}

#[test]
fn test_multi_producer_sender_switch_mpsc() {
    _test_multi_producer_sender_switch(mpsc::bounded_blocking::<usize>(5));
}

#[test]
fn test_multi_producer_sender_switch_mpmc() {
    _test_multi_producer_sender_switch(mpmc::bounded_blocking::<usize>(5));
}

// ============================================================================
// test_spsc_bounded_async_with_blocking_sender_switch
// ============================================================================

#[test]
fn test_spsc_bounded_async_with_blocking_sender_switch() {
    let (tx, rx) = spsc::bounded_async::<usize>(5);
    let total_messages = 10;
    let async_sent = 4;

    runtime_block_on_with_timeout!(async move {
        let sender_task = async_spawn!(async move {
            for i in 0..async_sent {
                tx.send(i).await.expect("Failed to send message");
            }
            tx
        });

        let tx = async_join_result!(sender_task);
        let blocking_tx: Tx<usize> = tx.into();

        let blocking_sender_handle = std::thread::spawn(move || {
            for i in async_sent..total_messages {
                blocking_tx.send(i).expect("Failed to send message");
            }
        });

        let receiver_task = async_spawn!(async move {
            let mut all_received = Vec::new();
            while let Ok(value) = rx.recv().await {
                all_received.push(value);
            }
            all_received
        });

        let all_received = async_join_result!(receiver_task);
        blocking_sender_handle
            .join()
            .expect("Blocking sender thread panicked");

        assert_eq!(all_received.len(), total_messages);
        for i in 0..total_messages {
            assert!(all_received.contains(&i), "Missing value: {}", i);
        }
    });
}

// ============================================================================
// test_mpsc_bounded_async_with_blocking_sender_switch
// ============================================================================

#[test]
fn test_mpsc_bounded_async_with_blocking_sender_switch() {
    let (tx, rx) = mpsc::bounded_async::<usize>(5);
    let total_messages = 10;
    let async_sent = 4;

    runtime_block_on_with_timeout!(async move {
        let sender_task = async_spawn!(async move {
            for i in 0..async_sent {
                tx.send(i).await.expect("Failed to send message");
            }
            tx
        });

        let tx = async_join_result!(sender_task);
        let blocking_tx: MTx<usize> = tx.into();

        let blocking_sender_handle = std::thread::spawn(move || {
            for i in async_sent..total_messages {
                blocking_tx.send(i).expect("Failed to send message");
            }
        });

        let receiver_task = async_spawn!(async move {
            let mut all_received = Vec::new();
            while let Ok(value) = rx.recv().await {
                all_received.push(value);
            }
            all_received
        });

        let all_received = async_join_result!(receiver_task);
        blocking_sender_handle
            .join()
            .expect("Blocking sender thread panicked");

        assert_eq!(all_received.len(), total_messages);
        for i in 0..total_messages {
            assert!(all_received.contains(&i), "Missing value: {}", i);
        }
    });
}

// ============================================================================
// test_mpmc_bounded_async_with_blocking_sender_switch
// ============================================================================

#[test]
fn test_mpmc_bounded_async_with_blocking_sender_switch() {
    let (tx, rx) = mpmc::bounded_async::<usize>(5);
    let total_messages = 10;
    let async_sent = 4;

    runtime_block_on_with_timeout!(async move {
        let sender_task = async_spawn!(async move {
            for i in 0..async_sent {
                tx.send(i).await.expect("Failed to send message");
            }
            tx
        });

        let tx = async_join_result!(sender_task);
        let blocking_tx: MTx<usize> = tx.into();

        let blocking_sender_handle = std::thread::spawn(move || {
            for i in async_sent..total_messages {
                blocking_tx.send(i).expect("Failed to send message");
            }
        });

        let receiver_task = async_spawn!(async move {
            let mut all_received = Vec::new();
            while let Ok(value) = rx.recv().await {
                all_received.push(value);
            }
            all_received
        });

        let all_received = async_join_result!(receiver_task);
        blocking_sender_handle
            .join()
            .expect("Blocking sender thread panicked");

        assert_eq!(all_received.len(), total_messages);
        for i in 0..total_messages {
            assert!(all_received.contains(&i), "Missing value: {}", i);
        }
    });
}
