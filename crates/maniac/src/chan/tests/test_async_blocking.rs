use super::common::*;
use crate::chan::*;
use std::thread;
use std::time::Duration;

// ============================================================================
// test_basic_bounded_empty_full_drop_rx
// ============================================================================

fn _test_basic_bounded_empty_full_drop_rx<T: AsyncTxTrait<usize>, R: BlockingRxTrait<usize>>(
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
    _test_basic_bounded_empty_full_drop_rx(spsc::bounded_tx_async_rx_blocking(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_rx_mpsc() {
    _test_basic_bounded_empty_full_drop_rx(mpsc::bounded_tx_async_rx_blocking(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_rx_mpmc() {
    _test_basic_bounded_empty_full_drop_rx(mpmc::bounded_tx_async_rx_blocking(1));
}

// ============================================================================
// test_basic_bounded_empty_full_drop_tx
// ============================================================================

fn _test_basic_bounded_empty_full_drop_tx<T: AsyncTxTrait<usize>, R: BlockingRxTrait<usize>>(
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
    _test_basic_bounded_empty_full_drop_tx(spsc::bounded_tx_async_rx_blocking(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_tx_mpsc() {
    _test_basic_bounded_empty_full_drop_tx(mpsc::bounded_tx_async_rx_blocking(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_tx_mpmc() {
    _test_basic_bounded_empty_full_drop_tx(mpmc::bounded_tx_async_rx_blocking(1));
}

// ============================================================================
// test_basic_compile_bounded_empty_full
// ============================================================================

#[test]
fn test_basic_compile_bounded_empty_full() {
    let (tx, rx) = mpmc::bounded_tx_async_rx_blocking::<usize>(1);
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
// test_basic_1_tx_async_1_rx_blocking
// ============================================================================

fn _test_basic_1_tx_async_1_rx_blocking<
    T: AsyncTxTrait<usize> + 'static,
    R: BlockingRxTrait<usize> + 'static,
>(
    channel: (T, R),
) {
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let rx_res = rx.try_recv();
        assert!(rx_res.is_err());
        assert!(rx_res.unwrap_err().is_empty());
        let batch_1: usize = 100;
        let batch_2: usize = 200;
        let th = thread::spawn(move || {
            for _ in 0..(batch_1 + batch_2) {
                match rx.recv() {
                    Ok(_i) => {}
                    Err(e) => {
                        panic!("error {}", e);
                    }
                }
            }
            let res = rx.recv();
            assert!(res.is_err());
        });

        for i in 0..batch_1 {
            let tx_res = tx.send(i).await;
            assert!(tx_res.is_ok());
        }
        for i in batch_1..(batch_1 + batch_2) {
            assert!(tx.send(10 + i).await.is_ok());
            sleep(Duration::from_millis(2)).await;
        }
        drop(tx);
        let _ = th.join().unwrap();
    });
}

#[test]
fn test_basic_1_tx_async_1_rx_blocking_spsc() {
    _test_basic_1_tx_async_1_rx_blocking(spsc::bounded_tx_async_rx_blocking::<usize>(100));
}

#[test]
fn test_basic_1_tx_async_1_rx_blocking_mpsc() {
    _test_basic_1_tx_async_1_rx_blocking(mpsc::bounded_tx_async_rx_blocking::<usize>(100));
}

#[test]
fn test_basic_1_tx_async_1_rx_blocking_mpmc() {
    _test_basic_1_tx_async_1_rx_blocking(mpmc::bounded_tx_async_rx_blocking::<usize>(100));
}

// ============================================================================
// test_basic_multi_tx_async_1_rx_blocking
// ============================================================================

fn _test_basic_multi_tx_async_1_rx_blocking<R: BlockingRxTrait<usize> + 'static>(
    channel: (MAsyncTx<usize>, R),
    tx_count: usize,
) {
    let batch_1: usize;
    let batch_2: usize;

    #[cfg(miri)]
    {
        if tx_count > 5 {
            println!("skip");
            return;
        }
        batch_1 = 10;
        batch_2 = 20;
    }
    #[cfg(not(miri))]
    {
        batch_1 = 100;
        batch_2 = 200;
    }
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let rx_res = rx.try_recv();
        assert!(rx_res.is_err());
        assert!(rx_res.unwrap_err().is_empty());
        let th = thread::spawn(move || {
            for _ in 0..((batch_1 + batch_2) * tx_count) {
                match rx.recv() {
                    Ok(_i) => {}
                    Err(e) => {
                        panic!("error {}", e);
                    }
                }
            }
            let res = rx.recv();
            assert!(res.is_err());
        });

        let mut th_s = Vec::new();
        for _tx_i in 0..tx_count {
            let _tx = tx.clone();
            th_s.push(async_spawn!(async move {
                for i in 0..batch_1 {
                    let tx_res = _tx.send(i).await;
                    assert!(tx_res.is_ok());
                }
                for i in batch_1..(batch_1 + batch_2) {
                    assert!(_tx.send(10 + i).await.is_ok());
                    sleep(Duration::from_millis(2)).await;
                }
            }));
        }
        drop(tx);

        for th in th_s {
            let _ = async_join_result!(th);
        }
        let _ = th.join().unwrap();
    });
}

#[test]
fn test_basic_multi_tx_async_1_rx_blocking_mpsc_5() {
    _test_basic_multi_tx_async_1_rx_blocking(mpsc::bounded_tx_async_rx_blocking::<usize>(10), 5);
}

#[test]
fn test_basic_multi_tx_async_1_rx_blocking_mpmc_5() {
    _test_basic_multi_tx_async_1_rx_blocking(mpmc::bounded_tx_async_rx_blocking::<usize>(10), 5);
}

// ============================================================================
// test_pressure_1_tx_async_1_rx_blocking
// ============================================================================

fn _test_pressure_1_tx_async_1_rx_blocking<
    T: AsyncTxTrait<usize> + 'static,
    R: BlockingRxTrait<usize> + 'static,
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
            let mut count = 0;
            'A: loop {
                match rx.recv() {
                    Ok(_i) => {
                        count += 1;
                    }
                    Err(_) => break 'A,
                }
            }
            count
        });

        for i in 0..round {
            match tx.send(i).await {
                Err(e) => panic!("{}", e),
                _ => {}
            }
        }
        drop(tx);
        let rx_count = th.join().unwrap();
        assert_eq!(rx_count, round);
    });
}

#[test]
fn test_pressure_1_tx_async_1_rx_blocking_spsc_1() {
    _test_pressure_1_tx_async_1_rx_blocking(spsc::bounded_tx_async_rx_blocking::<usize>(1));
}

#[test]
fn test_pressure_1_tx_async_1_rx_blocking_spsc_100() {
    _test_pressure_1_tx_async_1_rx_blocking(spsc::bounded_tx_async_rx_blocking::<usize>(100));
}

#[test]
fn test_pressure_1_tx_async_1_rx_blocking_mpsc_1() {
    _test_pressure_1_tx_async_1_rx_blocking(mpsc::bounded_tx_async_rx_blocking::<usize>(1));
}

#[test]
fn test_pressure_1_tx_async_1_rx_blocking_mpsc_100() {
    _test_pressure_1_tx_async_1_rx_blocking(mpsc::bounded_tx_async_rx_blocking::<usize>(100));
}

#[test]
fn test_pressure_1_tx_async_1_rx_blocking_mpmc_1() {
    _test_pressure_1_tx_async_1_rx_blocking(mpmc::bounded_tx_async_rx_blocking::<usize>(1));
}

#[test]
fn test_pressure_1_tx_async_1_rx_blocking_mpmc_100() {
    _test_pressure_1_tx_async_1_rx_blocking(mpmc::bounded_tx_async_rx_blocking::<usize>(100));
}

// ============================================================================
// test_pressure_multi_tx_async_1_rx_blocking
// ============================================================================

fn _test_pressure_multi_tx_async_1_rx_blocking<R: BlockingRxTrait<usize> + 'static>(
    channel: (MAsyncTx<usize>, R),
    tx_count: usize,
) {
    #[cfg(miri)]
    {
        if tx_count > 5 {
            println!("skip");
            return;
        }
    }

    let round: usize = ROUND;
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let th = thread::spawn(move || {
            let mut count = 0;
            'A: loop {
                match rx.recv() {
                    Ok(_i) => {
                        count += 1;
                    }
                    Err(_) => break 'A,
                }
            }
            count
        });

        let mut th_co = Vec::new();
        for _tx_i in 0..tx_count {
            let _tx = tx.clone();
            th_co.push(async_spawn!(async move {
                for i in 0..round {
                    match _tx.send(i).await {
                        Err(e) => panic!("{}", e),
                        _ => {}
                    }
                }
            }));
        }
        drop(tx);
        for th in th_co {
            let _ = async_join_result!(th);
        }
        let rx_count = th.join().unwrap();
        assert_eq!(rx_count, round * tx_count);
    });
}

#[test]
fn test_pressure_multi_tx_async_1_rx_blocking_mpsc_5() {
    _test_pressure_multi_tx_async_1_rx_blocking(mpsc::bounded_tx_async_rx_blocking::<usize>(10), 5);
}

#[test]
fn test_pressure_multi_tx_async_1_rx_blocking_mpmc_5() {
    _test_pressure_multi_tx_async_1_rx_blocking(mpmc::bounded_tx_async_rx_blocking::<usize>(10), 5);
}

// ============================================================================
// test_pressure_multi_tx_async_multi_rx_blocking
// ============================================================================

fn _test_pressure_multi_tx_async_multi_rx_blocking(
    channel: (MAsyncTx<usize>, MRx<usize>),
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

    let round: usize = ROUND;
    runtime_block_on!(async move {
        let (tx, rx) = channel;
        let mut rx_th_s = Vec::new();
        for _rx_i in 0..rx_count {
            let _rx = rx.clone();
            rx_th_s.push(thread::spawn(move || {
                let mut count = 0;
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
        }
        drop(rx);

        let mut th_co = Vec::new();
        for _tx_i in 0..tx_count {
            let _tx = tx.clone();
            th_co.push(async_spawn!(async move {
                for i in 0..round {
                    match _tx.send(i).await {
                        Err(e) => panic!("{}", e),
                        _ => {}
                    }
                }
            }));
        }
        drop(tx);
        for th in th_co {
            let _ = async_join_result!(th);
        }
        let mut total_count = 0;
        for th in rx_th_s {
            total_count += th.join().unwrap();
        }
        assert_eq!(total_count, round * tx_count);
    });
}

#[test]
fn test_pressure_multi_tx_async_multi_rx_blocking_5_5() {
    _test_pressure_multi_tx_async_multi_rx_blocking(
        mpmc::bounded_tx_async_rx_blocking::<usize>(10),
        5,
        5,
    );
}
