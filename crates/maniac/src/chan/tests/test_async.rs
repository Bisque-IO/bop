use super::common::*;
use crate::chan::{sink::*, stream::*, *};
use futures_core::stream::FusedStream;
use futures_util::{FutureExt, stream::StreamExt};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::*;
use std::thread;
use std::time::Duration;

// ============================================================================
// test_basic_bounded_empty_full_drop_rx
// ============================================================================

fn _test_basic_bounded_empty_full_drop_rx<T: AsyncTxTrait<usize>, R: AsyncRxTrait<usize>>(
    channel: (T, R),
) {
    let (tx, rx) = channel;
    assert!(tx.is_empty());
    assert!(rx.is_empty());
    assert_eq!(tx.capacity(), Some(1));
    assert_eq!(rx.capacity(), Some(1));
    tx.try_send(1).expect("Ok");
    assert!(tx.is_full());
    assert!(rx.is_full());
    assert!(!tx.is_empty());
    assert_eq!(tx.is_disconnected(), false);
    assert_eq!(rx.is_disconnected(), false);
    drop(rx);
    assert_eq!(tx.is_disconnected(), true);
    assert_eq!(tx.as_ref().get_rx_count(), 0);
    assert_eq!(tx.as_ref().get_tx_count(), 1);
}

#[test]
fn test_basic_bounded_empty_full_drop_rx_spsc() {
    _test_basic_bounded_empty_full_drop_rx(spsc::bounded_async(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_rx_mpsc() {
    _test_basic_bounded_empty_full_drop_rx(mpsc::bounded_async(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_rx_mpmc() {
    _test_basic_bounded_empty_full_drop_rx(mpmc::bounded_async(1));
}

// ============================================================================
// test_basic_bounded_empty_full_drop_tx
// ============================================================================

fn _test_basic_bounded_empty_full_drop_tx<T: AsyncTxTrait<usize>, R: AsyncRxTrait<usize>>(
    channel: (T, R),
) {
    let (tx, rx) = channel;
    assert!(tx.is_empty());
    assert!(rx.is_empty());
    assert_eq!(tx.capacity(), Some(1));
    assert_eq!(rx.capacity(), Some(1));
    tx.try_send(1).expect("Ok");
    assert!(tx.is_full());
    assert!(rx.is_full());
    assert!(!tx.is_empty());
    assert_eq!(tx.is_disconnected(), false);
    assert_eq!(rx.is_disconnected(), false);
    drop(tx);
    assert_eq!(rx.is_disconnected(), true);
    assert_eq!(rx.as_ref().get_tx_count(), 0);
    assert_eq!(rx.as_ref().get_rx_count(), 1);
}

#[test]
fn test_basic_bounded_empty_full_drop_tx_spsc() {
    _test_basic_bounded_empty_full_drop_tx(spsc::bounded_async(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_tx_mpsc() {
    _test_basic_bounded_empty_full_drop_tx(mpsc::bounded_async(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_tx_mpmc() {
    _test_basic_bounded_empty_full_drop_tx(mpmc::bounded_async(1));
}

// ============================================================================
// test_basic_compile_bounded_empty_full
// ============================================================================

#[test]
fn test_basic_compile_bounded_empty_full() {
    let (tx, rx) = mpmc::bounded_async::<usize>(1);
    assert!(tx.is_empty());
    assert!(rx.is_empty());
    tx.try_send(1).expect("ok");
    assert!(tx.is_full());
    assert!(!tx.is_empty());
    assert!(rx.is_full());
    assert_eq!(tx.get_tx_count(), 1);
    assert_eq!(rx.get_tx_count(), 1);
    assert_eq!(tx.is_disconnected(), false);
    assert_eq!(rx.is_disconnected(), false);
    drop(rx);
    assert_eq!(tx.is_disconnected(), true);
}

// ============================================================================
// test_sync
// ============================================================================

#[test]
fn test_sync() {
    runtime_block_on!(async move {
        let (tx, rx) = spsc::bounded_async::<usize>(100);
        let _task = async_spawn!(async move {
            let _ = tx.send(2).await;
        });
        drop(rx);

        let (tx, rx) = mpsc::bounded_async::<usize>(100);
        let _task = async_spawn!(async move {
            let _ = rx.recv().await;
        });
        drop(tx);

        let (tx, rx) = mpsc::bounded_blocking::<usize>(100);
        let _task = std::thread::spawn(move || {
            let _ = rx.recv();
        });
        drop(tx);

        let (tx, rx) = spsc::bounded_blocking::<usize>(100);
        std::thread::spawn(move || {
            let _ = tx.send(1);
        });
        drop(rx);

        let (tx, rx) = mpmc::bounded_blocking::<usize>(100);
        let rx = Arc::new(rx);
        std::thread::spawn(move || {
            let _ = rx.try_recv();
        });
        let tx = Arc::new(tx);
        std::thread::spawn(move || {
            let _ = tx.try_send(1);
        });

        let (tx, rx) = spsc::bounded_async::<usize>(100);
        let th = async_spawn!(async move {
            let mut i = 0;
            loop {
                sleep(Duration::from_secs(1)).await;
                i += 1;
                if let Err(_) = tx.send(i).await {
                    println!("rx dropped");
                    return;
                }
            }
        });
        'LOOP: for _ in 0..10 {
            crate::select! {
                _ = sleep(Duration::from_millis(500)).fuse() => {
                    println!("tick");
                },
                r = rx.recv().fuse() => {
                    match r {
                        Ok(item) => {
                            println!("recv {}", item);
                        }
                        Err(e) => {
                            println!("tx dropped {:?}", e);
                            break 'LOOP;
                        }
                    }
                }
            }
        }
        drop(rx);
        let _ = async_join_result!(th);
    });
}

// ============================================================================
// test_basic_bounded_rx_drop
// ============================================================================

fn _test_basic_bounded_rx_drop<
    T: AsyncTxTrait<usize> + 'static,
    R: AsyncRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    runtime_block_on!(async move {
        let tx = {
            let (tx, _rx) = channel;
            tx.send(1).await.expect("ok");
            tx.send(2).await.expect("ok");
            tx.send(3).await.expect("ok");
            tx
        };
        {
            assert_eq!(tx.send(4).await.unwrap_err(), SendError(4));
            drop(tx);
        }
    });
}

#[test]
fn test_basic_bounded_rx_drop_spsc() {
    _test_basic_bounded_rx_drop(spsc::bounded_async::<usize>(100));
}

#[test]
fn test_basic_bounded_rx_drop_mpsc() {
    _test_basic_bounded_rx_drop(mpsc::bounded_async::<usize>(100));
}

#[test]
fn test_basic_bounded_rx_drop_mpmc() {
    _test_basic_bounded_rx_drop(mpmc::bounded_async::<usize>(100));
}

// ============================================================================
// test_basic_unbounded_rx_drop
// ============================================================================

fn _test_basic_unbounded_rx_drop<
    T: BlockingTxTrait<usize> + 'static,
    R: AsyncRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    runtime_block_on!(async move {
        let tx = {
            let (tx, _rx) = channel;
            tx.send(1).expect("ok");
            tx.send(2).expect("ok");
            tx.send(3).expect("ok");
            tx
        };
        {
            assert_eq!(tx.send(4).unwrap_err(), SendError(4));
            drop(tx);
        }
    });
}

#[test]
fn test_basic_unbounded_rx_drop_spsc() {
    _test_basic_unbounded_rx_drop(spsc::unbounded_async::<usize>());
}

#[test]
fn test_basic_unbounded_rx_drop_mpsc() {
    _test_basic_unbounded_rx_drop(mpsc::unbounded_async::<usize>());
}

#[test]
fn test_basic_unbounded_rx_drop_mpmc() {
    _test_basic_unbounded_rx_drop(mpmc::unbounded_async::<usize>());
}

// ============================================================================
// test_basic_bounded_1_thread
// ============================================================================

fn _test_basic_bounded_1_thread<
    T: AsyncTxTrait<usize> + 'static,
    R: AsyncRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    let (tx, rx) = channel;
    runtime_block_on!(async move {
        let rx_res = rx.try_recv();
        assert!(rx_res.is_err());
        assert!(rx_res.unwrap_err().is_empty());
        for i in 0usize..10 {
            let tx_res = tx.try_send(i);
            assert!(tx_res.is_ok());
        }
        let tx_res = tx.try_send(11);
        assert!(tx_res.is_err());
        assert!(tx_res.unwrap_err().is_full());

        let th = async_spawn!(async move {
            for i in 0usize..12 {
                match rx.recv().await {
                    Ok(j) => {
                        assert_eq!(i, j);
                    }
                    Err(e) => {
                        panic!("error {}", e);
                    }
                }
            }
            let res = rx.recv().await;
            assert!(res.is_err());
        });
        assert!(tx.send(10).await.is_ok());
        sleep(Duration::from_secs(1)).await;
        assert!(tx.send(11).await.is_ok());
        drop(tx);
        let _ = async_join_result!(th);
    });
}

#[test]
fn test_basic_bounded_1_thread_spsc() {
    _test_basic_bounded_1_thread(spsc::bounded_async::<usize>(10));
}

#[test]
fn test_basic_bounded_1_thread_mpsc() {
    _test_basic_bounded_1_thread(mpsc::bounded_async::<usize>(10));
}

#[test]
fn test_basic_bounded_1_thread_mpmc() {
    _test_basic_bounded_1_thread(mpmc::bounded_async::<usize>(10));
}

// ============================================================================
// test_basic_unbounded_1_thread
// ============================================================================

fn _test_basic_unbounded_1_thread<
    T: BlockingTxTrait<usize> + 'static,
    R: AsyncRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    let (tx, rx) = channel;
    assert_eq!(tx.capacity(), None);
    assert_eq!(rx.capacity(), None);
    runtime_block_on!(async move {
        let rx_res = rx.try_recv();
        assert!(rx_res.is_err());
        assert!(rx_res.unwrap_err().is_empty());
        for i in 0usize..10 {
            let tx_res = tx.try_send(i);
            assert!(tx_res.is_ok());
        }

        let th = async_spawn!(async move {
            for i in 0usize..12 {
                match rx.recv().await {
                    Ok(j) => {
                        assert_eq!(i, j);
                    }
                    Err(e) => {
                        panic!("error {}", e);
                    }
                }
            }
            let res = rx.recv().await;
            assert!(res.is_err());
        });
        assert!(tx.send(10).is_ok());
        sleep(Duration::from_secs(1)).await;
        assert!(tx.send(11).is_ok());
        drop(tx);
        let _ = async_join_result!(th);
    });
}

#[test]
fn test_basic_unbounded_1_thread_spsc() {
    _test_basic_unbounded_1_thread(spsc::unbounded_async::<usize>());
}

#[test]
fn test_basic_unbounded_1_thread_mpsc() {
    _test_basic_unbounded_1_thread(mpsc::unbounded_async::<usize>());
}

#[test]
fn test_basic_unbounded_1_thread_mpmc() {
    _test_basic_unbounded_1_thread(mpmc::unbounded_async::<usize>());
}

// ============================================================================
// test_basic_unbounded_idle_select
// ============================================================================

fn _test_basic_unbounded_idle_select<
    T: BlockingTxTrait<usize> + 'static,
    R: AsyncRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    let (_tx, rx) = channel;
    let round = {
        #[cfg(miri)]
        {
            10
        }
        #[cfg(not(miri))]
        {
            200
        }
    };

    runtime_block_on!(async move {
        let mut c = std::pin::pin!(rx.recv());
        for _ in 0..round {
            {
                let f = sleep(Duration::from_millis(1));
                let mut f = std::pin::pin!(f);
                crate::select! {
                    _ = &mut f => {
                        let (_tx_wakers, _rx_wakers) = rx.as_ref().get_wakers_count();
                    },
                    _ = &mut c => {
                        unreachable!()
                    },
                }
            }
        }
        let (tx_wakers, _rx_wakers) = rx.as_ref().get_wakers_count();
        assert_eq!(tx_wakers, 0);
    });
}

#[test]
fn test_basic_unbounded_idle_select_spsc() {
    _test_basic_unbounded_idle_select(spsc::unbounded_async::<usize>());
}

#[test]
fn test_basic_unbounded_idle_select_mpsc() {
    _test_basic_unbounded_idle_select(mpsc::unbounded_async::<usize>());
}

#[test]
fn test_basic_unbounded_idle_select_mpmc() {
    _test_basic_unbounded_idle_select(mpmc::unbounded_async::<usize>());
}

// ============================================================================
// test_basic_bounded_recv_after_sender_close
// ============================================================================

fn _test_basic_bounded_recv_after_sender_close<
    T: AsyncTxTrait<usize> + 'static,
    R: AsyncRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let total_msg_count = 5;
        for i in 0..total_msg_count {
            let _ = tx.try_send(i).expect("send ok");
        }
        drop(tx);

        let mut recv_msg_count = 0;
        loop {
            match rx.recv().await {
                Ok(_) => {
                    recv_msg_count += 1;
                }
                Err(_) => {
                    break;
                }
            }
        }
        assert_eq!(recv_msg_count, total_msg_count);
    });
}

#[test]
fn test_basic_bounded_recv_after_sender_close_spsc() {
    _test_basic_bounded_recv_after_sender_close(spsc::bounded_async::<usize>(10));
}

#[test]
fn test_basic_bounded_recv_after_sender_close_mpsc() {
    _test_basic_bounded_recv_after_sender_close(mpsc::bounded_async::<usize>(10));
}

#[test]
fn test_basic_bounded_recv_after_sender_close_mpmc() {
    _test_basic_bounded_recv_after_sender_close(mpmc::bounded_async::<usize>(10));
}

// ============================================================================
// test_basic_unbounded_recv_after_sender_close
// ============================================================================

fn _test_basic_unbounded_recv_after_sender_close<
    T: BlockingTxTrait<usize> + 'static,
    R: AsyncRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let total_msg_count = 500;
        for i in 0..total_msg_count {
            let _ = tx.send(i).expect("send ok");
        }
        drop(tx);

        let mut recv_msg_count = 0;
        loop {
            match rx.recv().await {
                Ok(_) => {
                    recv_msg_count += 1;
                }
                Err(_) => {
                    break;
                }
            }
        }
        assert_eq!(recv_msg_count, total_msg_count);
    });
}

#[test]
fn test_basic_unbounded_recv_after_sender_close_spsc() {
    _test_basic_unbounded_recv_after_sender_close(spsc::unbounded_async::<usize>());
}

#[test]
fn test_basic_unbounded_recv_after_sender_close_mpsc() {
    _test_basic_unbounded_recv_after_sender_close(mpsc::unbounded_async::<usize>());
}

#[test]
fn test_basic_unbounded_recv_after_sender_close_mpmc() {
    _test_basic_unbounded_recv_after_sender_close(mpmc::unbounded_async::<usize>());
}

// ============================================================================
// test_basic_timeout_recv_async_waker
// ============================================================================

fn _test_basic_timeout_recv_async_waker<
    T: AsyncTxTrait<usize> + 'static,
    R: AsyncRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        for _ in 0..1000 {
            assert!(
                rx.recv_with_timer(sleep(Duration::from_millis(1)))
                    .await
                    .is_err()
            );
        }
        let (tx_wakers, rx_wakers) = rx.as_ref().get_wakers_count();
        println!("wakers: {}, {}", tx_wakers, rx_wakers);
        assert!(tx_wakers <= 1);
        assert!(rx_wakers <= 1);
        sleep(Duration::from_secs(1)).await;
        let _ = tx.send(1).await;
        assert_eq!(rx.recv().await.unwrap(), 1);
        let (tx_wakers, rx_wakers) = rx.as_ref().get_wakers_count();
        println!("wakers: {}, {}", tx_wakers, rx_wakers);
        assert!(tx_wakers <= 1);
        assert!(rx_wakers <= 1);
    });
}

#[test]
fn test_basic_timeout_recv_async_waker_spsc() {
    _test_basic_timeout_recv_async_waker(spsc::bounded_async::<usize>(100));
}

#[test]
fn test_basic_timeout_recv_async_waker_mpsc() {
    _test_basic_timeout_recv_async_waker(mpsc::bounded_async::<usize>(100));
}

#[test]
fn test_basic_timeout_recv_async_waker_mpmc() {
    _test_basic_timeout_recv_async_waker(mpmc::bounded_async::<usize>(100));
}

// ============================================================================
// test_basic_unbounded_recv_timeout_async
// ============================================================================

fn _test_basic_unbounded_recv_timeout_async<
    T: BlockingTxTrait<usize> + 'static,
    R: AsyncRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let th = async_spawn!(async move {
            sleep(Duration::from_millis(50)).await;
            let _ = tx.send(1);
        });
        let r = rx.recv_with_timer(sleep(Duration::from_millis(1))).await;
        #[cfg(not(miri))]
        {
            assert_eq!(r.unwrap_err(), RecvTimeoutError::Timeout);
        }
        let _ = async_join_result!(th);
        let (tx_wakers, rx_wakers) = rx.as_ref().get_wakers_count();
        println!("wakers: {}, {}", tx_wakers, rx_wakers);
        assert_eq!(tx_wakers, 0);
        assert_eq!(rx_wakers, 0);
        let r = rx.recv_with_timer(sleep(Duration::from_millis(200))).await;
        #[cfg(not(miri))]
        {
            assert_eq!(r.unwrap(), 1);
        }
    });
}

#[test]
fn test_basic_unbounded_recv_timeout_async_spsc() {
    _test_basic_unbounded_recv_timeout_async(spsc::unbounded_async::<usize>());
}

#[test]
fn test_basic_unbounded_recv_timeout_async_mpsc() {
    _test_basic_unbounded_recv_timeout_async(mpsc::unbounded_async::<usize>());
}

#[test]
fn test_basic_unbounded_recv_timeout_async_mpmc() {
    _test_basic_unbounded_recv_timeout_async(mpmc::unbounded_async::<usize>());
}

// ============================================================================
// test_basic_send_timeout_async
// ============================================================================

fn _test_basic_send_timeout_async<
    T: AsyncTxTrait<usize> + 'static,
    R: AsyncRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        for i in 0..10 {
            assert!(tx.try_send(i).is_ok());
        }
        assert_eq!(
            tx.send_with_timer(11, sleep(Duration::from_millis(1)))
                .await
                .unwrap_err(),
            SendTimeoutError::Timeout(11)
        );
        let th = async_spawn!(async move {
            loop {
                sleep(Duration::from_millis(2)).await;
                if let Err(_) = rx.recv().await {
                    println!("tx dropped");
                    break;
                }
            }
        });
        let mut try_times = 0;
        loop {
            try_times += 1;
            match tx
                .send_with_timer(11, sleep(Duration::from_millis(1)))
                .await
            {
                Ok(_) => {
                    println!("send ok after {} tries", try_times);
                    break;
                }
                Err(SendTimeoutError::Timeout(msg)) => {
                    println!("timeout");
                    assert_eq!(msg, 11);
                }
                Err(SendTimeoutError::Disconnected(_)) => {
                    unreachable!();
                }
            }
        }
        let (tx_wakers, rx_wakers) = tx.as_ref().get_wakers_count();
        println!("wakers: {}, {}", tx_wakers, rx_wakers);
        assert!(tx_wakers <= 1, "{:?}", tx_wakers);
        assert!(rx_wakers <= 1, "{:?}", rx_wakers);
        drop(tx);
        let _ = async_join_result!(th);
    });
}

#[test]
fn test_basic_send_timeout_async_spsc() {
    _test_basic_send_timeout_async(spsc::bounded_async::<usize>(10));
}

#[test]
fn test_basic_send_timeout_async_mpsc() {
    _test_basic_send_timeout_async(mpsc::bounded_async::<usize>(10));
}

#[test]
fn test_basic_send_timeout_async_mpmc() {
    _test_basic_send_timeout_async(mpmc::bounded_async::<usize>(10));
}

// ============================================================================
// test_pressure_bounded_timeout_async
// ============================================================================

#[test]
fn test_pressure_bounded_timeout_async() {
    use parking_lot::Mutex;
    use std::collections::HashMap;
    let (tx, rx) = mpmc::bounded_async::<usize>(1);
    let tx_count: usize = 3;
    let rx_count: usize = 2;

    runtime_block_on!(async move {
        assert_eq!(
            rx.recv_with_timer(sleep(Duration::from_millis(1)))
                .await
                .unwrap_err(),
            RecvTimeoutError::Timeout
        );
        let (tx_wakers, rx_wakers) = rx.as_ref().get_wakers_count();
        println!("wakers: {}, {}", tx_wakers, rx_wakers);
        assert_eq!(tx_wakers, 0);
        assert_eq!(rx_wakers, 0);

        let recv_map = Arc::new(Mutex::new(HashMap::new()));

        let mut th_tx = Vec::new();
        let mut th_rx = Vec::new();

        for thread_id in 0..tx_count {
            let _recv_map = recv_map.clone();
            let _tx = tx.clone();
            th_tx.push(async_spawn!(async move {
                let mut local_send_timeout_count = 0;
                let mut i = 0;
                sleep(Duration::from_millis((thread_id & 3) as u64)).await;
                loop {
                    if i >= ROUND {
                        return local_send_timeout_count;
                    }
                    {
                        let mut guard = _recv_map.lock();
                        guard.insert(i, ());
                    }
                    if i & 2 == 0 {
                        sleep(Duration::from_millis(3)).await;
                    } else {
                        sleep(Duration::from_millis(1)).await;
                    }
                    loop {
                        match _tx
                            .send_with_timer(i, sleep(Duration::from_millis(1)))
                            .await
                        {
                            Ok(_) => {
                                i += 1;
                                break;
                            }
                            Err(SendTimeoutError::Timeout(_i)) => {
                                local_send_timeout_count += 1;
                                assert_eq!(_i, i);
                            }
                            Err(SendTimeoutError::Disconnected(_)) => {
                                unreachable!();
                            }
                        }
                    }
                }
            }));
        }

        for _thread_id in 0..rx_count {
            let _rx = rx.clone();
            let _recv_map = recv_map.clone();
            th_rx.push(async_spawn!(async move {
                let mut step: usize = 0;
                let mut local_recv_count: usize = 0;
                let mut local_recv_timeout_count: usize = 0;
                loop {
                    step += 1;
                    let timeout = if step & 2 == 0 { 1 } else { 2 };
                    if step & 2 > 0 {
                        sleep(Duration::from_millis(1)).await;
                    }
                    match _rx
                        .recv_with_timer(sleep(Duration::from_millis(timeout)))
                        .await
                    {
                        Ok(item) => {
                            local_recv_count += 1;
                            {
                                let mut guard = _recv_map.lock();
                                guard.remove(&item);
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            local_recv_timeout_count += 1;
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            return (local_recv_count, local_recv_timeout_count);
                        }
                    }
                }
            }));
        }
        drop(tx);
        drop(rx);

        let mut total_send_timeout_count = 0;
        for th in th_tx {
            total_send_timeout_count += async_join_result!(th);
        }
        let mut total_recv_count = 0;
        let mut total_recv_timeout_count = 0;
        for th in th_rx {
            let (recv_count, recv_timeout_count) = async_join_result!(th);
            total_recv_count += recv_count;
            total_recv_timeout_count += recv_timeout_count;
        }
        {
            let guard = recv_map.lock();
            assert!(guard.is_empty());
        }
        assert_eq!(ROUND * tx_count, total_recv_count);
        println!("send timeout count: {}", total_send_timeout_count);
        println!("recv timeout count: {}", total_recv_timeout_count);
    });
}

// ============================================================================
// test_pressure_bounded_async_1_1
// ============================================================================

fn _test_pressure_bounded_async_1_1<
    T: AsyncTxTrait<usize> + 'static,
    R: AsyncRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let mut counter: usize = 0;
        let th = async_spawn!(async move {
            for i in 0..ROUND {
                if let Err(e) = tx.send(i).await {
                    panic!("{:?}", e);
                }
            }
        });
        'A: loop {
            match rx.recv().await {
                Ok(_i) => {
                    counter += 1;
                }
                Err(_) => break 'A,
            }
        }
        drop(rx);
        let _ = async_join_result!(th);
        assert_eq!(counter, ROUND);
    });
}

#[test]
fn test_pressure_bounded_async_1_1_spsc_1() {
    _test_pressure_bounded_async_1_1(spsc::bounded_async::<usize>(1));
}

#[test]
fn test_pressure_bounded_async_1_1_spsc_10() {
    _test_pressure_bounded_async_1_1(spsc::bounded_async::<usize>(10));
}

#[test]
fn test_pressure_bounded_async_1_1_spsc_100() {
    _test_pressure_bounded_async_1_1(spsc::bounded_async::<usize>(100));
}

#[test]
fn test_pressure_bounded_async_1_1_mpmc_1() {
    _test_pressure_bounded_async_1_1(mpmc::bounded_async::<usize>(1));
}

#[test]
fn test_pressure_bounded_async_1_1_mpmc_10() {
    _test_pressure_bounded_async_1_1(mpmc::bounded_async::<usize>(10));
}

#[test]
fn test_pressure_bounded_async_1_1_mpmc_100() {
    _test_pressure_bounded_async_1_1(mpmc::bounded_async::<usize>(100));
}

// ============================================================================
// test_pressure_bounded_async_multi_1
// ============================================================================

fn _test_pressure_bounded_async_multi_1<R: AsyncRxTrait<usize> + 'static>(
    channel: (MAsyncTx<usize>, R),
    tx_count: usize,
) {
    #[cfg(miri)]
    {
        if tx_count > 10 {
            println!("skip");
            return;
        }
    }
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let mut counter = 0;
        let mut th_s = Vec::new();
        for _tx_i in 0..tx_count {
            let _tx = tx.clone();
            th_s.push(async_spawn!(async move {
                for i in 0..ROUND {
                    match _tx.send(i).await {
                        Err(e) => panic!("{:?}", e),
                        _ => {}
                    }
                }
            }));
        }
        drop(tx);
        'A: loop {
            match rx.recv().await {
                Ok(_i) => {
                    counter += 1;
                }
                Err(_) => break 'A,
            }
        }
        drop(rx);
        for th in th_s {
            let _ = async_join_result!(th);
        }
        assert_eq!(counter, ROUND * tx_count);
    });
}

#[test]
fn test_pressure_bounded_async_multi_1_mpsc_1_5() {
    _test_pressure_bounded_async_multi_1(mpsc::bounded_async::<usize>(1), 5);
}

#[test]
fn test_pressure_bounded_async_multi_1_mpsc_10_100() {
    _test_pressure_bounded_async_multi_1(mpsc::bounded_async::<usize>(10), 100);
}

#[test]
fn test_pressure_bounded_async_multi_1_mpmc_1_5() {
    _test_pressure_bounded_async_multi_1(mpmc::bounded_async::<usize>(1), 5);
}

#[test]
fn test_pressure_bounded_async_multi_1_mpmc_10_100() {
    _test_pressure_bounded_async_multi_1(mpmc::bounded_async::<usize>(10), 100);
}

// ============================================================================
// test_pressure_bounded_async_multi
// ============================================================================

fn _test_pressure_bounded_async_multi(
    channel: (MAsyncTx<usize>, MAsyncRx<usize>),
    tx_count: usize,
    rx_count: usize,
) {
    #[cfg(miri)]
    {
        if rx_count > 5 || tx_count > 5 {
            println!("skip");
            return;
        }
    }
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let mut th_tx = Vec::new();
        let mut th_rx = Vec::new();
        for _tx_i in 0..tx_count {
            let _tx = tx.clone();
            th_tx.push(async_spawn!(async move {
                for i in 0..ROUND {
                    match _tx.send(i).await {
                        Err(e) => panic!("{:?}", e),
                        _ => {}
                    }
                }
            }));
        }
        for _rx_i in 0..rx_count {
            let _rx = rx.clone();
            th_rx.push(async_spawn!(async move {
                let mut count = 0;
                'A: loop {
                    match _rx.recv().await {
                        Ok(_i) => {
                            count += 1;
                        }
                        Err(_) => break 'A,
                    }
                }
                count
            }));
        }
        drop(tx);
        drop(rx);
        for th in th_tx {
            let _ = async_join_result!(th);
        }
        let mut recv_count = 0;
        for th in th_rx {
            recv_count += async_join_result!(th);
        }
        assert_eq!(recv_count, ROUND * tx_count);
    });
}

#[test]
fn test_pressure_bounded_async_multi_mpmc_1_5_5() {
    _test_pressure_bounded_async_multi(mpmc::bounded_async::<usize>(1), 5, 5);
}

#[test]
fn test_pressure_bounded_async_multi_mpmc_10_10_10() {
    _test_pressure_bounded_async_multi(mpmc::bounded_async::<usize>(10), 10, 10);
}

#[test]
fn test_pressure_bounded_async_multi_mpmc_100_100_100() {
    _test_pressure_bounded_async_multi(mpmc::bounded_async::<usize>(100), 100, 100);
}

// ============================================================================
// test_pressure_bounded_mixed_async_blocking_conversion
// ============================================================================

fn _test_pressure_bounded_mixed_async_blocking_conversion(
    channel: (MAsyncTx<usize>, MAsyncRx<usize>),
) {
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let mut recv_counter = 0;
        let mut th_tx = Vec::new();
        let mut th_rx = Vec::new();
        let mut co_tx = Vec::new();
        let mut co_rx = Vec::new();
        let _tx: MTx<usize> = tx.clone().into();
        th_tx.push(thread::spawn(move || {
            for i in 0..ROUND {
                match _tx.send(i) {
                    Err(e) => panic!("{:?}", e),
                    _ => {}
                }
            }
        }));
        co_tx.push(async_spawn!(async move {
            for i in 0..ROUND {
                match tx.send(i).await {
                    Err(e) => panic!("{:?}", e),
                    _ => {}
                }
            }
        }));
        let _rx: MRx<usize> = rx.clone().into();
        th_rx.push(thread::spawn(move || {
            let mut count: usize = 0;
            'A: loop {
                match _rx.recv() {
                    Ok(_i) => {
                        count += 1;
                    }
                    Err(_) => break 'A,
                }
            }
            count
        }));

        co_rx.push(async_spawn!(async move {
            let mut count: usize = 0;
            'A: loop {
                match rx.recv().await {
                    Ok(_i) => {
                        count += 1;
                    }
                    Err(_) => break 'A,
                }
            }
            count
        }));
        for th in co_tx {
            let _ = async_join_result!(th);
        }
        for th in co_rx {
            recv_counter += async_join_result!(th);
        }
        for th in th_tx {
            let _ = th.join().unwrap();
        }
        for th in th_rx {
            recv_counter += th.join().unwrap();
        }
        assert_eq!(recv_counter, ROUND * 2);
    });
}

#[test]
fn test_pressure_bounded_mixed_async_blocking_conversion_1() {
    _test_pressure_bounded_mixed_async_blocking_conversion(mpmc::bounded_async::<usize>(1));
}

#[test]
fn test_pressure_bounded_mixed_async_blocking_conversion_10() {
    _test_pressure_bounded_mixed_async_blocking_conversion(mpmc::bounded_async::<usize>(10));
}

#[test]
fn test_pressure_bounded_mixed_async_blocking_conversion_100() {
    _test_pressure_bounded_mixed_async_blocking_conversion(mpmc::bounded_async::<usize>(100));
}

// ============================================================================
// test_conversion
// ============================================================================

#[test]
fn test_conversion() {
    use crate::chan::stream::AsyncStream;
    let (mtx, mrx) = mpmc::bounded_async(1);
    let _tx: AsyncTx<usize> = mtx.into();
    let _rx: AsyncRx<usize> = mrx.into();
    let (_mtx, rx) = mpsc::bounded_async(1);
    let _stream: AsyncStream<usize> = rx.into();
    let (_mtx, mrx) = mpmc::bounded_async(1);
    let _stream: AsyncStream<usize> = mrx.into();
}

// ============================================================================
// Spurious wakeup tests
// ============================================================================

#[allow(dead_code)]
struct SpuriousTx {
    sink: AsyncSink<usize>,
    normal: bool,
    step: usize,
}

impl Future for SpuriousTx {
    type Output = Result<usize, usize>;

    fn poll(self: Pin<&mut Self>, ctx: &mut std::task::Context) -> Poll<Self::Output> {
        let _self = self.get_mut();
        if !_self.normal && _self.step > 0 {
            return Poll::Ready(Err(_self.step));
        }
        match _self.sink.poll_send(ctx, _self.step) {
            Ok(_) => {
                let res = _self.step;
                _self.step += 1;
                Poll::Ready(Ok(res))
            }
            Err(TrySendError::Disconnected(_)) => Poll::Ready(Err(_self.step)),
            Err(TrySendError::Full(_)) => {
                _self.step += 1;
                Poll::Pending
            }
        }
    }
}

#[allow(dead_code)]
struct SpuriousRx {
    stream: AsyncStream<usize>,
    normal: bool,
    step: usize,
}

impl Future for SpuriousRx {
    type Output = Result<usize, usize>;

    fn poll(self: Pin<&mut Self>, ctx: &mut std::task::Context) -> Poll<Self::Output> {
        let _self = self.get_mut();
        if !_self.normal && _self.step > 0 {
            return Poll::Ready(Err(_self.step));
        }
        match _self.stream.poll_item(ctx) {
            Poll::Ready(Some(item)) => {
                _self.step += 1;
                Poll::Ready(Ok(item))
            }
            Poll::Ready(None) => Poll::Ready(Err(_self.step)),
            Poll::Pending => {
                _self.step += 1;
                Poll::Pending
            }
        }
    }
}

// ============================================================================
// test_basic_into_stream_1_1
// ============================================================================

fn _test_basic_into_stream_1_1<
    T: AsyncTxTrait<usize> + 'static,
    R: AsyncRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    runtime_block_on!(async move {
        let total_message = 100;
        let (tx, rx) = channel;
        let th = async_spawn!(async move {
            for i in 0usize..total_message {
                let _ = tx.send(i).await;
            }
        });
        let mut s: AsyncStream<usize> = rx.into();

        for _i in 0..total_message {
            assert_eq!(s.next().await, Some(_i));
        }
        assert_eq!(s.next().await, None);
        assert!(s.is_terminated());
        async_join_result!(th);
    });
}

#[test]
fn test_basic_into_stream_1_1_spsc_1() {
    _test_basic_into_stream_1_1(spsc::bounded_async::<usize>(1));
}

#[test]
fn test_basic_into_stream_1_1_spsc_2() {
    _test_basic_into_stream_1_1(spsc::bounded_async::<usize>(2));
}

#[test]
fn test_basic_into_stream_1_1_mpsc_1() {
    _test_basic_into_stream_1_1(mpsc::bounded_async::<usize>(1));
}

#[test]
fn test_basic_into_stream_1_1_mpmc_1() {
    _test_basic_into_stream_1_1(mpmc::bounded_async::<usize>(1));
}

// ============================================================================
// test_pressure_stream_multi
// ============================================================================

fn _test_pressure_stream_multi(channel: (MAsyncTx<usize>, MAsyncRx<usize>), rx_count: usize) {
    #[cfg(miri)]
    {
        if rx_count > 5 {
            println!("skip");
            return;
        }
    }
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let mut th_s = Vec::new();
        let mut recv_counter = 0;
        for _rx_i in 0..rx_count {
            let _rx = rx.clone();
            th_s.push(async_spawn!(async move {
                let mut counter = 0;
                let mut stream = _rx.into_stream();
                while let Some(_item) = stream.next().await {
                    counter += 1;
                }
                counter
            }));
        }
        drop(rx);
        for i in 0..ROUND {
            tx.send(i).await.expect("send");
        }
        drop(tx);
        for th in th_s {
            recv_counter += async_join_result!(th);
        }
        assert_eq!(recv_counter, ROUND);
    });
}

#[test]
fn test_pressure_stream_multi_1_2() {
    _test_pressure_stream_multi(mpmc::bounded_async::<usize>(1), 2);
}

#[test]
fn test_pressure_stream_multi_10_10() {
    _test_pressure_stream_multi(mpmc::bounded_async::<usize>(10), 10);
}

// ============================================================================
// Drop message tests
// ============================================================================

fn _test_async_drop_msg<
    M: TestDropMsg + 'static,
    T: AsyncTxTrait<M> + 'static,
    R: AsyncRxTrait<M> + 'static,
>(
    channel: (T, R),
) {
    let _lock = DROP_COUNTER_LOCK.lock().unwrap();
    reset_drop_counter();
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let cap = tx.capacity().unwrap();
        let mut ids = cap;
        for i in 0..ids {
            let msg = M::new(i);
            assert!(tx.try_send(msg).is_ok());
        }
        assert_eq!(get_drop_counter(), 0);
        let msg = M::new(ids);
        if let Err(TrySendError::Full(_msg)) = tx.try_send(msg) {
            assert_eq!(_msg.get_value(), ids);
            assert_eq!(get_drop_counter(), 0);
            drop(_msg);
            assert_eq!(get_drop_counter(), 1);
        } else {
            unreachable!();
        }
        let th = async_spawn!(async move {
            let _msg = rx.recv().await.expect("recv");
            assert_eq!(_msg.get_value(), 0);
            drop(_msg);
            rx
        });
        let msg = M::new(ids);
        tx.send(msg).await.expect("send");
        ids += 1;
        let rx = async_join_result!(th);
        drop(rx);
        assert_eq!(get_drop_counter(), 2);
        let msg = M::new(ids);
        if let Err(TrySendError::Disconnected(_msg)) = tx.try_send(msg) {
            assert_eq!(_msg.get_value(), ids);
        } else {
            unreachable!();
        }
        ids += 1;
        let msg = M::new(ids);
        if let Err(SendError(_msg)) = tx.send(msg).await {
            assert_eq!(_msg.get_value(), ids);
        } else {
            unreachable!();
        }
        assert_eq!(get_drop_counter(), 4);
        ids += 1;
        drop(tx);
        assert_eq!(get_drop_counter(), ids + 1);
        assert_eq!(get_drop_counter(), 4 + cap);
    });
}

#[test]
fn test_async_drop_small_msg_spsc_1() {
    _test_async_drop_msg::<SmallMsg, _, _>(spsc::bounded_async::<SmallMsg>(1));
}

#[test]
fn test_async_drop_small_msg_spsc_10() {
    _test_async_drop_msg::<SmallMsg, _, _>(spsc::bounded_async::<SmallMsg>(10));
}

#[test]
fn test_async_drop_small_msg_mpsc_1() {
    _test_async_drop_msg::<SmallMsg, _, _>(mpsc::bounded_async::<SmallMsg>(1));
}

#[test]
fn test_async_drop_small_msg_mpmc_1() {
    _test_async_drop_msg::<SmallMsg, _, _>(mpmc::bounded_async::<SmallMsg>(1));
}

#[test]
fn test_async_drop_large_msg_spsc_1() {
    _test_async_drop_msg::<LargeMsg, _, _>(spsc::bounded_async::<LargeMsg>(1));
}

#[test]
fn test_async_drop_large_msg_mpsc_1() {
    _test_async_drop_msg::<LargeMsg, _, _>(mpsc::bounded_async::<LargeMsg>(1));
}

#[test]
fn test_async_drop_large_msg_mpmc_1() {
    _test_async_drop_msg::<LargeMsg, _, _>(mpmc::bounded_async::<LargeMsg>(1));
}
