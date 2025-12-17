use super::common::*;
use crate::chan::*;
use std::thread;
use std::time::*;

// ============================================================================
// test_basic_bounded_empty_full_drop_rx
// ============================================================================

fn _test_basic_bounded_empty_full_drop_rx<T: BlockingTxTrait<usize>, R: AsyncRxTrait<usize>>(
    channel: (T, R),
) {
    let (tx, rx) = channel;
    assert!(tx.is_empty());
    assert!(rx.is_empty());
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
    _test_basic_bounded_empty_full_drop_rx(spsc::bounded_tx_blocking_rx_async(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_rx_mpsc() {
    _test_basic_bounded_empty_full_drop_rx(mpsc::bounded_tx_blocking_rx_async(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_rx_mpmc() {
    _test_basic_bounded_empty_full_drop_rx(mpmc::bounded_tx_blocking_rx_async(1));
}

// ============================================================================
// test_basic_bounded_empty_full_drop_tx
// ============================================================================

fn _test_basic_bounded_empty_full_drop_tx<T: BlockingTxTrait<usize>, R: AsyncRxTrait<usize>>(
    channel: (T, R),
) {
    let (tx, rx) = channel;
    assert!(tx.is_empty());
    assert!(rx.is_empty());
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
    _test_basic_bounded_empty_full_drop_tx(spsc::bounded_tx_blocking_rx_async(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_tx_mpsc() {
    _test_basic_bounded_empty_full_drop_tx(mpsc::bounded_tx_blocking_rx_async(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_tx_mpmc() {
    _test_basic_bounded_empty_full_drop_tx(mpmc::bounded_tx_blocking_rx_async(1));
}

// ============================================================================
// test_basic_unbounded_empty_drop_tx
// ============================================================================

fn _test_basic_unbounded_empty_drop_tx<T: BlockingTxTrait<usize>, R: AsyncRxTrait<usize>>(
    channel: (T, R),
) {
    let (tx, rx) = channel;
    assert!(tx.is_empty());
    assert!(rx.is_empty());
    tx.try_send(1).expect("Ok");
    assert!(!tx.is_empty());
    assert_eq!(tx.is_disconnected(), false);
    assert_eq!(rx.is_disconnected(), false);
    drop(tx);
    assert_eq!(rx.is_disconnected(), true);
    assert_eq!(rx.as_ref().get_tx_count(), 0);
    assert_eq!(rx.as_ref().get_rx_count(), 1);
}

#[test]
fn test_basic_unbounded_empty_drop_tx_spsc() {
    _test_basic_unbounded_empty_drop_tx(spsc::unbounded_async::<usize>());
}

#[test]
fn test_basic_unbounded_empty_drop_tx_mpsc() {
    _test_basic_unbounded_empty_drop_tx(mpsc::unbounded_async::<usize>());
}

#[test]
fn test_basic_unbounded_empty_drop_tx_mpmc() {
    _test_basic_unbounded_empty_drop_tx(mpmc::unbounded_async::<usize>());
}

// ============================================================================
// test_basic_compile_bounded_empty_full
// ============================================================================

#[test]
fn test_basic_compile_bounded_empty_full() {
    let (tx, rx) = mpmc::bounded_tx_blocking_rx_async::<usize>(1);
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
// test_basic_1_tx_blocking_1_rx_async
// ============================================================================

fn _test_basic_1_tx_blocking_1_rx_async<
    T: BlockingTxTrait<usize> + 'static,
    R: AsyncRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let rx_res = rx.try_recv();
        assert!(rx_res.is_err());
        assert!(rx_res.unwrap_err().is_empty());
        for i in 0usize..10 {
            let tx_res = tx.send(i);
            assert!(tx_res.is_ok());
        }
        let tx_res = tx.try_send(11);
        assert!(tx_res.is_err());
        assert!(tx_res.unwrap_err().is_full());

        let th = thread::spawn(move || {
            assert!(tx.send(10).is_ok());
            std::thread::sleep(Duration::from_secs(1));
            assert!(tx.send(11).is_ok());
        });

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
        let _ = th.join().unwrap();
    });
}

#[test]
fn test_basic_1_tx_blocking_1_rx_async_spsc() {
    _test_basic_1_tx_blocking_1_rx_async(spsc::bounded_tx_blocking_rx_async::<usize>(10));
}

#[test]
fn test_basic_1_tx_blocking_1_rx_async_mpsc() {
    _test_basic_1_tx_blocking_1_rx_async(mpsc::bounded_tx_blocking_rx_async::<usize>(10));
}

#[test]
fn test_basic_1_tx_blocking_1_rx_async_mpmc() {
    _test_basic_1_tx_blocking_1_rx_async(mpmc::bounded_tx_blocking_rx_async::<usize>(10));
}

// ============================================================================
// test_pressure_1_tx_blocking_1_rx_async
// ============================================================================

fn _test_pressure_1_tx_blocking_1_rx_async<
    T: BlockingTxTrait<usize> + 'static,
    R: AsyncRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    let round: usize;
    #[cfg(miri)]
    {
        round = ROUND;
    }
    #[cfg(not(miri))]
    {
        round = ROUND * 100;
    }
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let th = thread::spawn(move || {
            for i in 0..round {
                tx.send(i).expect("send ok");
            }
        });

        for i in 0..round {
            match rx.recv().await {
                Ok(msg) => {
                    assert_eq!(msg, i);
                }
                Err(_e) => {
                    panic!("channel closed");
                }
            }
        }
        assert!(rx.recv().await.is_err());
        let _ = th.join().unwrap();
    });
}

#[test]
fn test_pressure_1_tx_blocking_1_rx_async_spsc_1() {
    _test_pressure_1_tx_blocking_1_rx_async(spsc::bounded_tx_blocking_rx_async::<usize>(1));
}

#[test]
fn test_pressure_1_tx_blocking_1_rx_async_spsc_100() {
    _test_pressure_1_tx_blocking_1_rx_async(spsc::bounded_tx_blocking_rx_async::<usize>(100));
}

#[test]
fn test_pressure_1_tx_blocking_1_rx_async_mpsc_1() {
    _test_pressure_1_tx_blocking_1_rx_async(mpsc::bounded_tx_blocking_rx_async::<usize>(1));
}

#[test]
fn test_pressure_1_tx_blocking_1_rx_async_mpsc_100() {
    _test_pressure_1_tx_blocking_1_rx_async(mpsc::bounded_tx_blocking_rx_async::<usize>(100));
}

#[test]
fn test_pressure_1_tx_blocking_1_rx_async_mpmc_1() {
    _test_pressure_1_tx_blocking_1_rx_async(mpmc::bounded_tx_blocking_rx_async::<usize>(1));
}

#[test]
fn test_pressure_1_tx_blocking_1_rx_async_mpmc_100() {
    _test_pressure_1_tx_blocking_1_rx_async(mpmc::bounded_tx_blocking_rx_async::<usize>(100));
}

#[test]
fn test_pressure_1_tx_blocking_1_rx_async_spsc_unbounded() {
    _test_pressure_1_tx_blocking_1_rx_async(spsc::unbounded_async::<usize>());
}

#[test]
fn test_pressure_1_tx_blocking_1_rx_async_mpsc_unbounded() {
    _test_pressure_1_tx_blocking_1_rx_async(mpsc::unbounded_async::<usize>());
}

#[test]
fn test_pressure_1_tx_blocking_1_rx_async_mpmc_unbounded() {
    _test_pressure_1_tx_blocking_1_rx_async(mpmc::unbounded_async::<usize>());
}

// ============================================================================
// test_pressure_tx_multi_blocking_1_rx_async
// ============================================================================

fn _test_pressure_tx_multi_blocking_1_rx_async<R: AsyncRxTrait<usize> + 'static>(
    channel: (MTx<usize>, R),
    tx_count: usize,
) {
    #[cfg(miri)]
    {
        if tx_count > 5 {
            println!("skip");
            return;
        }
    }

    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let mut tx_th_s = Vec::new();
        for _tx_i in 0..tx_count {
            let _tx = tx.clone();
            tx_th_s.push(thread::spawn(move || {
                for i in 0..ROUND {
                    match _tx.send(i) {
                        Err(e) => panic!("{}", e),
                        _ => {}
                    }
                }
            }));
        }
        drop(tx);

        let mut count = 0;
        'A: loop {
            match rx.recv().await {
                Ok(_i) => {
                    count += 1;
                }
                Err(_) => break 'A,
            }
        }
        for th in tx_th_s {
            let _ = th.join().unwrap();
        }
        assert_eq!(count, ROUND * tx_count);
    });
}

#[test]
fn test_pressure_tx_multi_blocking_1_rx_async_mpsc_1_5() {
    _test_pressure_tx_multi_blocking_1_rx_async(mpsc::bounded_tx_blocking_rx_async::<usize>(1), 5);
}

#[test]
fn test_pressure_tx_multi_blocking_1_rx_async_mpsc_100_10() {
    _test_pressure_tx_multi_blocking_1_rx_async(
        mpsc::bounded_tx_blocking_rx_async::<usize>(100),
        10,
    );
}

#[test]
fn test_pressure_tx_multi_blocking_1_rx_async_mpmc_1_5() {
    _test_pressure_tx_multi_blocking_1_rx_async(mpmc::bounded_tx_blocking_rx_async::<usize>(1), 5);
}

#[test]
fn test_pressure_tx_multi_blocking_1_rx_async_mpmc_100_10() {
    _test_pressure_tx_multi_blocking_1_rx_async(
        mpmc::bounded_tx_blocking_rx_async::<usize>(100),
        10,
    );
}

#[test]
fn test_pressure_tx_multi_blocking_1_rx_async_mpsc_unbounded_5() {
    _test_pressure_tx_multi_blocking_1_rx_async(mpsc::unbounded_async::<usize>(), 5);
}

#[test]
fn test_pressure_tx_multi_blocking_1_rx_async_mpmc_unbounded_5() {
    _test_pressure_tx_multi_blocking_1_rx_async(mpmc::unbounded_async::<usize>(), 5);
}

// ============================================================================
// test_pressure_tx_multi_blocking_multi_rx_async
// ============================================================================

fn _test_pressure_tx_multi_blocking_multi_rx_async(
    channel: (MTx<usize>, MAsyncRx<usize>),
    tx_count: usize,
    rx_count: usize,
) {
    #[cfg(miri)]
    {
        if tx_count > 5 || rx_count > 5 {
            println!("skip");
            return;
        }
    }

    use std::sync::atomic::{AtomicUsize, Ordering};
    let sent_count = std::sync::Arc::new(AtomicUsize::new(0));
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let mut tx_th_s = Vec::new();
        for _tx_i in 0..tx_count {
            let _tx = tx.clone();
            let sent = sent_count.clone();
            tx_th_s.push(thread::spawn(move || {
                for i in 0..ROUND {
                    match _tx.send(i) {
                        Err(_e) => {
                            break;
                        }
                        _ => {
                            sent.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }));
        }
        drop(tx);

        let mut th_co = Vec::new();
        for _rx_i in 0..rx_count {
            let _rx = rx.clone();
            th_co.push(async_spawn!(async move {
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
        drop(rx);
        let mut total_count = 0;
        for th in th_co {
            total_count += async_join_result!(th);
        }
        for th in tx_th_s {
            let _ = th.join().unwrap();
        }
        let actual_sent = sent_count.load(Ordering::Relaxed);
        assert_eq!(
            total_count, actual_sent,
            "received {} but sent {}",
            total_count, actual_sent
        );
    });
}

#[test]
fn test_pressure_tx_multi_blocking_multi_rx_async_1_5_5() {
    _test_pressure_tx_multi_blocking_multi_rx_async(
        mpmc::bounded_tx_blocking_rx_async::<usize>(1),
        5,
        5,
    );
}

#[test]
fn test_pressure_tx_multi_blocking_multi_rx_async_10_10_10() {
    _test_pressure_tx_multi_blocking_multi_rx_async(
        mpmc::bounded_tx_blocking_rx_async::<usize>(10),
        10,
        10,
    );
}

#[test]
fn test_pressure_tx_multi_blocking_multi_rx_async_unbounded_5_5() {
    _test_pressure_tx_multi_blocking_multi_rx_async(mpmc::unbounded_async::<usize>(), 5, 5);
}
