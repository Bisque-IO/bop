use super::common::*;
use crate::chan::*;
use std::sync::Arc;
use std::thread;
use std::thread::sleep;
use std::time::{Duration, Instant};

// ============================================================================
// test_basic_bounded_empty_full_drop_rx
// ============================================================================

fn _test_basic_bounded_empty_full_drop_rx<T: BlockingTxTrait<usize>, R: BlockingRxTrait<usize>>(
    channel: (T, R),
) {
    #[cfg(not(feature = "async_std"))]
    {
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
        assert_eq!(tx.try_send(2).unwrap_err(), TrySendError::Disconnected(2));
        assert_eq!(tx.send(2).unwrap_err(), SendError(2));
        let start = Instant::now();
        assert_eq!(
            tx.send_timeout(3, Duration::from_secs(1)).unwrap_err(),
            SendTimeoutError::Disconnected(3)
        );
        assert!(Instant::now() - start < Duration::from_secs(1));
    }
    #[cfg(feature = "async_std")]
    let _ = channel;
}

#[test]
fn test_basic_bounded_empty_full_drop_rx_spsc() {
    _test_basic_bounded_empty_full_drop_rx(spsc::bounded_blocking(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_rx_mpsc() {
    _test_basic_bounded_empty_full_drop_rx(mpsc::bounded_blocking(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_rx_mpmc() {
    _test_basic_bounded_empty_full_drop_rx(mpmc::bounded_blocking(1));
}

// ============================================================================
// test_basic_bounded_empty_full_drop_tx
// ============================================================================

fn _test_basic_bounded_empty_full_drop_tx<T: BlockingTxTrait<usize>, R: BlockingRxTrait<usize>>(
    channel: (T, R),
) {
    #[cfg(not(feature = "async_std"))]
    {
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
        assert_eq!(rx.try_recv().unwrap(), 1);
        assert_eq!(rx.try_recv().unwrap_err(), TryRecvError::Disconnected);
        assert_eq!(rx.recv().unwrap_err(), RecvError);
        let start = Instant::now();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap_err(),
            RecvTimeoutError::Disconnected
        );
        assert!(Instant::now() - start < Duration::from_secs(1));
    }
    #[cfg(feature = "async_std")]
    let _ = channel;
}

#[test]
fn test_basic_bounded_empty_full_drop_tx_spsc() {
    _test_basic_bounded_empty_full_drop_tx(spsc::bounded_blocking(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_tx_mpsc() {
    _test_basic_bounded_empty_full_drop_tx(mpsc::bounded_blocking(1));
}

#[test]
fn test_basic_bounded_empty_full_drop_tx_mpmc() {
    _test_basic_bounded_empty_full_drop_tx(mpmc::bounded_blocking(1));
}

// ============================================================================
// test_basic_unbounded_empty_drop_rx
// ============================================================================

fn _test_basic_unbounded_empty_drop_rx<T: BlockingTxTrait<usize>, R: BlockingRxTrait<usize>>(
    channel: (T, R),
) {
    #[cfg(not(feature = "async_std"))]
    {
        let (tx, rx) = channel;
        assert!(tx.is_empty());
        assert!(rx.is_empty());
        assert_eq!(tx.capacity(), None);
        assert_eq!(rx.capacity(), None);
        tx.try_send(1).expect("Ok");
        assert!(!tx.is_empty());
        assert_eq!(tx.is_disconnected(), false);
        assert_eq!(rx.is_disconnected(), false);
        drop(rx);
        assert_eq!(tx.is_disconnected(), true);
        assert_eq!(tx.as_ref().get_rx_count(), 0);
        assert_eq!(tx.as_ref().get_tx_count(), 1);
        assert_eq!(tx.try_send(2).unwrap_err(), TrySendError::Disconnected(2));
        assert_eq!(tx.send(2).unwrap_err(), SendError(2));
        let start = Instant::now();
        assert_eq!(
            tx.send_timeout(3, Duration::from_secs(1)).unwrap_err(),
            SendTimeoutError::Disconnected(3)
        );
        assert!(Instant::now() - start < Duration::from_secs(1));
    }
    #[cfg(feature = "async_std")]
    let _ = channel;
}

#[test]
fn test_basic_unbounded_empty_drop_rx_spsc() {
    _test_basic_unbounded_empty_drop_rx(spsc::unbounded_blocking());
}

#[test]
fn test_basic_unbounded_empty_drop_rx_mpsc() {
    _test_basic_unbounded_empty_drop_rx(mpsc::unbounded_blocking());
}

#[test]
fn test_basic_unbounded_empty_drop_rx_mpmc() {
    _test_basic_unbounded_empty_drop_rx(mpmc::unbounded_blocking());
}

// ============================================================================
// test_basic_unbounded_empty_drop_tx
// ============================================================================

fn _test_basic_unbounded_empty_drop_tx<T: BlockingTxTrait<usize>, R: BlockingRxTrait<usize>>(
    channel: (T, R),
) {
    #[cfg(not(feature = "async_std"))]
    {
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
        assert_eq!(rx.recv().unwrap(), 1);
        assert_eq!(rx.try_recv().unwrap_err(), TryRecvError::Disconnected);
        assert_eq!(rx.recv().unwrap_err(), RecvError);
        let start = Instant::now();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap_err(),
            RecvTimeoutError::Disconnected
        );
        assert!(Instant::now() - start < Duration::from_secs(1));
    }
    #[cfg(feature = "async_std")]
    let _ = channel;
}

#[test]
fn test_basic_unbounded_empty_drop_tx_spsc() {
    _test_basic_unbounded_empty_drop_tx(spsc::unbounded_blocking());
}

#[test]
fn test_basic_unbounded_empty_drop_tx_mpsc() {
    _test_basic_unbounded_empty_drop_tx(mpsc::unbounded_blocking());
}

#[test]
fn test_basic_unbounded_empty_drop_tx_mpmc() {
    _test_basic_unbounded_empty_drop_tx(mpmc::unbounded_blocking());
}

// ============================================================================
// test_basic_bounded_1_thread
// ============================================================================

fn _test_basic_bounded_1_thread<T: BlockingTxTrait<i32>, R: BlockingRxTrait<i32>>(channel: (T, R)) {
    #[cfg(not(feature = "async_std"))]
    {
        let (tx, rx) = channel;
        let rx_res = rx.try_recv();
        assert!(rx_res.is_err());
        assert!(rx_res.unwrap_err().is_empty());
        for i in 0i32..10 {
            let tx_res = tx.try_send(i);
            assert!(tx_res.is_ok());
        }
        let tx_res = tx.try_send(11);
        assert!(tx_res.is_err());
        assert!(tx_res.unwrap_err().is_full());

        let th = thread::spawn(move || {
            for i in 0i32..12 {
                match rx.recv() {
                    Ok(j) => {
                        assert_eq!(i, j);
                    }
                    Err(e) => {
                        panic!("error {}", e);
                    }
                }
            }
            let res = rx.recv();
            assert!(res.is_err());
        });
        assert!(tx.send(10).is_ok());
        sleep(Duration::from_secs(1));
        assert!(tx.send(11).is_ok());
        drop(tx);
        let _ = th.join().unwrap();
    }
    #[cfg(feature = "async_std")]
    let _ = channel;
}

#[test]
fn test_basic_bounded_1_thread_spsc() {
    _test_basic_bounded_1_thread(spsc::bounded_blocking::<i32>(10));
}

#[test]
fn test_basic_bounded_1_thread_mpsc() {
    _test_basic_bounded_1_thread(mpsc::bounded_blocking::<i32>(10));
}

#[test]
fn test_basic_bounded_1_thread_mpmc() {
    _test_basic_bounded_1_thread(mpmc::bounded_blocking::<i32>(10));
}

// ============================================================================
// test_basic_unbounded_1_thread
// ============================================================================

fn _test_basic_unbounded_1_thread<T: BlockingTxTrait<i32>, R: BlockingRxTrait<i32>>(
    channel: (T, R),
) {
    #[cfg(not(feature = "async_std"))]
    {
        let (tx, rx) = channel;
        let rx_res = rx.try_recv();
        assert!(rx_res.is_err());
        assert!(rx_res.unwrap_err().is_empty());
        for i in 0i32..10 {
            let tx_res = tx.try_send(i);
            assert!(tx_res.is_ok());
        }

        let th = thread::spawn(move || {
            for i in 0i32..12 {
                match rx.recv() {
                    Ok(j) => {
                        assert_eq!(i, j);
                    }
                    Err(e) => {
                        panic!("error {}", e);
                    }
                }
            }
            let res = rx.recv();
            assert!(res.is_err());
        });
        assert!(tx.send(10).is_ok());
        sleep(Duration::from_secs(1));
        assert!(tx.send(11).is_ok());
        drop(tx);
        let _ = th.join().unwrap();
    }
    #[cfg(feature = "async_std")]
    let _ = channel;
}

#[test]
fn test_basic_unbounded_1_thread_spsc() {
    _test_basic_unbounded_1_thread(spsc::unbounded_blocking::<i32>());
}

#[test]
fn test_basic_unbounded_1_thread_mpsc() {
    _test_basic_unbounded_1_thread(mpsc::unbounded_blocking::<i32>());
}

#[test]
fn test_basic_unbounded_1_thread_mpmc() {
    _test_basic_unbounded_1_thread(mpmc::unbounded_blocking::<i32>());
}

// ============================================================================
// test_basic_recv_after_sender_close
// ============================================================================

fn _test_basic_recv_after_sender_close<T: BlockingTxTrait<i32>, R: BlockingRxTrait<i32>>(
    channel: (T, R),
) {
    #[cfg(not(feature = "async_std"))]
    {
        let (tx, rx) = channel;
        let total_msg_count = 5;
        for i in 0..total_msg_count {
            let _ = tx.try_send(i).expect("send ok");
        }
        drop(tx);

        let mut recv_msg_count = 0;
        loop {
            match rx.recv() {
                Ok(_) => {
                    recv_msg_count += 1;
                }
                Err(_) => {
                    break;
                }
            }
        }
        assert_eq!(recv_msg_count, total_msg_count);
    }
    #[cfg(feature = "async_std")]
    let _ = channel;
}

#[test]
fn test_basic_recv_after_sender_close_spsc_bounded() {
    _test_basic_recv_after_sender_close(spsc::bounded_blocking::<i32>(10));
}

#[test]
fn test_basic_recv_after_sender_close_mpsc_bounded() {
    _test_basic_recv_after_sender_close(mpsc::bounded_blocking::<i32>(10));
}

#[test]
fn test_basic_recv_after_sender_close_mpmc_bounded() {
    _test_basic_recv_after_sender_close(mpmc::bounded_blocking::<i32>(10));
}

#[test]
fn test_basic_recv_after_sender_close_spsc_unbounded() {
    _test_basic_recv_after_sender_close(spsc::unbounded_blocking::<i32>());
}

#[test]
fn test_basic_recv_after_sender_close_mpsc_unbounded() {
    _test_basic_recv_after_sender_close(mpsc::unbounded_blocking::<i32>());
}

#[test]
fn test_basic_recv_after_sender_close_mpmc_unbounded() {
    _test_basic_recv_after_sender_close(mpmc::unbounded_blocking::<i32>());
}

// ============================================================================
// test_pressure_bounded_blocking_1_1
// ============================================================================

fn _test_pressure_bounded_blocking_1_1<T: BlockingTxTrait<usize>, R: BlockingRxTrait<usize>>(
    channel: (T, R),
) {
    #[cfg(not(feature = "async_std"))]
    {
        let (tx, rx) = channel;
        let round: usize;
        #[cfg(miri)]
        {
            round = ROUND;
        }
        #[cfg(not(miri))]
        {
            round = ROUND * 100;
        }
        let th = thread::spawn(move || {
            for i in 0..round {
                if let Err(e) = tx.send(i) {
                    panic!("{:?}", e);
                }
            }
        });
        let mut count = 0;
        'A: loop {
            match rx.recv() {
                Ok(_i) => {
                    count += 1;
                }
                Err(_) => break 'A,
            }
        }
        drop(rx);
        let _ = th.join().unwrap();
        assert_eq!(count, round);
    }
    #[cfg(feature = "async_std")]
    let _ = channel;
}

#[test]
fn test_pressure_bounded_blocking_1_1_spsc_1() {
    _test_pressure_bounded_blocking_1_1(spsc::bounded_blocking::<usize>(1));
}

#[test]
fn test_pressure_bounded_blocking_1_1_spsc_100() {
    _test_pressure_bounded_blocking_1_1(spsc::bounded_blocking::<usize>(100));
}

#[test]
fn test_pressure_bounded_blocking_1_1_mpsc_1() {
    _test_pressure_bounded_blocking_1_1(mpsc::bounded_blocking::<usize>(1));
}

#[test]
fn test_pressure_bounded_blocking_1_1_mpsc_100() {
    _test_pressure_bounded_blocking_1_1(mpsc::bounded_blocking::<usize>(100));
}

#[test]
fn test_pressure_bounded_blocking_1_1_mpmc_1() {
    _test_pressure_bounded_blocking_1_1(mpmc::bounded_blocking::<usize>(1));
}

#[test]
fn test_pressure_bounded_blocking_1_1_mpmc_100() {
    _test_pressure_bounded_blocking_1_1(mpmc::bounded_blocking::<usize>(100));
}

// ============================================================================
// test_pressure_bounded_blocking_multi_1
// ============================================================================

fn _test_pressure_bounded_blocking_multi_1<R: BlockingRxTrait<usize>>(
    channel: (MTx<usize>, R),
    tx_count: usize,
) {
    #[cfg(not(feature = "async_std"))]
    {
        let (tx, rx) = channel;
        #[cfg(miri)]
        {
            if tx_count > 5 {
                println!("skip");
                return;
            }
        }

        let round: usize = ROUND * 10;
        let mut th_s = Vec::new();
        for _tx_i in 0..tx_count {
            let _tx = tx.clone();
            th_s.push(thread::spawn(move || {
                for i in 0..round {
                    match _tx.send(i) {
                        Err(e) => panic!("{:?}", e),
                        _ => {}
                    }
                }
            }));
        }
        drop(tx);
        let mut count = 0;
        'A: loop {
            match rx.recv() {
                Ok(_i) => {
                    count += 1;
                }
                Err(_) => break 'A,
            }
        }
        drop(rx);
        for th in th_s {
            let _ = th.join().unwrap();
        }
        assert_eq!(count, round * tx_count);
    }
    #[cfg(feature = "async_std")]
    let _ = (channel, tx_count);
}

#[test]
fn test_pressure_bounded_blocking_multi_1_mpsc_1_3() {
    _test_pressure_bounded_blocking_multi_1(mpsc::bounded_blocking::<usize>(1), 3);
}

#[test]
fn test_pressure_bounded_blocking_multi_1_mpsc_10_5() {
    _test_pressure_bounded_blocking_multi_1(mpsc::bounded_blocking::<usize>(10), 5);
}

#[test]
fn test_pressure_bounded_blocking_multi_1_mpmc_1_3() {
    _test_pressure_bounded_blocking_multi_1(mpmc::bounded_blocking::<usize>(1), 3);
}

#[test]
fn test_pressure_bounded_blocking_multi_1_mpmc_10_5() {
    _test_pressure_bounded_blocking_multi_1(mpmc::bounded_blocking::<usize>(10), 5);
}

// ============================================================================
// test_pressure_bounded_blocking_multi
// ============================================================================

fn _test_pressure_bounded_blocking_multi(
    channel: (MTx<usize>, MRx<usize>),
    tx_count: usize,
    rx_count: usize,
) {
    #[cfg(not(feature = "async_std"))]
    {
        let round: usize;
        #[cfg(miri)]
        {
            if tx_count > 5 || rx_count > 5 {
                println!("skip");
                return;
            }
            round = ROUND;
        }
        #[cfg(not(miri))]
        {
            round = ROUND * 10;
        }
        let (tx, rx) = channel;
        let mut th_tx = Vec::new();
        let mut th_rx = Vec::new();
        for _tx_i in 0..tx_count {
            let _tx = tx.clone();
            th_tx.push(thread::spawn(move || {
                for i in 0..round {
                    match _tx.send(i) {
                        Err(e) => panic!("{:?}", e),
                        _ => {}
                    }
                }
            }));
        }
        for _rx_i in 0..rx_count {
            let _rx = rx.clone();
            th_rx.push(thread::spawn(move || {
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
        drop(tx);
        drop(rx);
        let mut total_count = 0;
        for th in th_tx {
            let _ = th.join().unwrap();
        }
        for th in th_rx {
            total_count += th.join().unwrap();
        }
        assert_eq!(total_count, round * tx_count);
    }
    #[cfg(feature = "async_std")]
    let _ = (channel, tx_count, rx_count);
}

#[test]
fn test_pressure_bounded_blocking_multi_mpmc_1_2_2() {
    _test_pressure_bounded_blocking_multi(mpmc::bounded_blocking::<usize>(1), 2, 2);
}

#[test]
fn test_pressure_bounded_blocking_multi_mpmc_10_5_5() {
    _test_pressure_bounded_blocking_multi(mpmc::bounded_blocking::<usize>(10), 5, 5);
}

#[test]
fn test_pressure_bounded_blocking_multi_mpmc_100_5_5() {
    _test_pressure_bounded_blocking_multi(mpmc::bounded_blocking::<usize>(100), 5, 5);
}

// ============================================================================
// test_pressure_bounded_timeout_blocking
// ============================================================================

fn _test_pressure_bounded_timeout_blocking(channel: (MTx<usize>, MRx<usize>)) {
    #[cfg(not(feature = "async_std"))]
    {
        use parking_lot::Mutex;
        use std::collections::HashMap;
        let (tx, rx) = channel;

        assert_eq!(
            rx.recv_timeout(Duration::from_millis(1)).unwrap_err(),
            RecvTimeoutError::Timeout
        );
        let (tx_wakers, rx_wakers) = rx.as_ref().get_wakers_count();
        println!("wakers: {}, {}", tx_wakers, rx_wakers);
        assert_eq!(tx_wakers, 0);
        assert_eq!(rx_wakers, 0);

        let recv_map = Arc::new(Mutex::new(HashMap::new()));

        let mut th_tx = Vec::new();
        let mut th_rx = Vec::new();
        let tx_count: usize = 3;
        for thread_id in 0..tx_count {
            let _recv_map = recv_map.clone();
            let _tx = tx.clone();
            th_tx.push(thread::spawn(move || {
                sleep(Duration::from_millis((thread_id & 3) as u64));
                let mut local_timeout_counter = 0;
                for i in 0..ROUND {
                    {
                        let mut guard = _recv_map.lock();
                        guard.insert(i, ());
                    }
                    if i & 2 == 0 {
                        sleep(Duration::from_millis(3));
                    } else {
                        sleep(Duration::from_millis(1));
                    }
                    loop {
                        match _tx.send_timeout(i, Duration::from_millis(1)) {
                            Ok(_) => break,
                            Err(SendTimeoutError::Timeout(_i)) => {
                                local_timeout_counter += 1;
                                assert_eq!(_i, i);
                            }
                            Err(SendTimeoutError::Disconnected(_)) => {
                                unreachable!();
                            }
                        }
                    }
                }
                local_timeout_counter
            }));
        }
        for _thread_id in 0..2 {
            let _rx = rx.clone();
            let _recv_map = recv_map.clone();
            th_rx.push(thread::spawn(move || {
                let mut step: usize = 0;
                let mut local_recv_counter = 0;
                let mut local_timeout_counter = 0;
                loop {
                    step += 1;
                    let timeout = if step & 2 == 0 { 1 } else { 2 };
                    if step & 2 > 0 {
                        sleep(Duration::from_millis(1));
                    }
                    match _rx.recv_timeout(Duration::from_millis(timeout)) {
                        Ok(item) => {
                            local_recv_counter += 1;
                            {
                                let mut guard = _recv_map.lock();
                                guard.remove(&item);
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            local_timeout_counter += 1;
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            return (local_recv_counter, local_timeout_counter);
                        }
                    }
                }
            }));
        }
        drop(tx);
        drop(rx);
        let mut total_recv_count = 0;
        let mut total_send_timeout = 0;
        let mut total_recv_timeout = 0;
        for th in th_tx {
            total_send_timeout += th.join().unwrap();
        }
        for th in th_rx {
            let (local_recv_counter, local_timeout_counter) = th.join().unwrap();
            total_recv_count += local_recv_counter;
            total_recv_timeout += local_timeout_counter;
        }
        {
            let guard = recv_map.lock();
            assert!(guard.is_empty());
        }
        assert_eq!(ROUND * tx_count, total_recv_count);
        println!("send timeout count: {}", total_send_timeout);
        println!("recv timeout count: {}", total_recv_timeout);
    }
    #[cfg(feature = "async_std")]
    let _ = channel;
}

#[test]
fn test_pressure_bounded_timeout_blocking_1() {
    _test_pressure_bounded_timeout_blocking(mpmc::bounded_blocking::<usize>(1));
}

#[test]
fn test_pressure_bounded_timeout_blocking_10() {
    _test_pressure_bounded_timeout_blocking(mpmc::bounded_blocking::<usize>(10));
}

// ============================================================================
// test_conversion
// ============================================================================

#[test]
fn test_conversion() {
    let (mtx, mrx) = mpmc::bounded_blocking::<usize>(1);
    let _tx: Tx<usize> = mtx.into();
    let _rx: Rx<usize> = mrx.into();
}

// ============================================================================
// Drop message tests
// ============================================================================

fn _test_drop_msg<M: TestDropMsg, T: BlockingTxTrait<M>, R: BlockingRxTrait<M>>(channel: (T, R)) {
    let _lock = DROP_COUNTER_LOCK.lock().unwrap();
    let (tx, rx) = channel;
    reset_drop_counter();
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
    let th = thread::spawn(move || {
        let _msg = rx.recv().expect("recv");
        assert_eq!(_msg.get_value(), 0);
        drop(_msg);
        rx
    });
    let msg = M::new(ids);
    tx.send(msg).expect("send");
    ids += 1;
    let rx = th.join().unwrap();
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
    if let Err(SendError(_msg)) = tx.send(msg) {
        assert_eq!(_msg.get_value(), ids);
    } else {
        unreachable!();
    }
    assert_eq!(get_drop_counter(), 4);
    ids += 1;
    drop(tx);
    assert_eq!(get_drop_counter(), ids + 1);
    assert_eq!(get_drop_counter(), 4 + cap);
}

#[test]
fn test_drop_small_msg_spsc_1() {
    _test_drop_msg::<SmallMsg, _, _>(spsc::bounded_blocking::<SmallMsg>(1));
}

#[test]
fn test_drop_small_msg_spsc_10() {
    _test_drop_msg::<SmallMsg, _, _>(spsc::bounded_blocking::<SmallMsg>(10));
}

#[test]
fn test_drop_small_msg_mpsc_1() {
    _test_drop_msg::<SmallMsg, _, _>(mpsc::bounded_blocking::<SmallMsg>(1));
}

#[test]
fn test_drop_small_msg_mpmc_1() {
    _test_drop_msg::<SmallMsg, _, _>(mpmc::bounded_blocking::<SmallMsg>(1));
}

#[test]
fn test_drop_large_msg_spsc_1() {
    _test_drop_msg::<LargeMsg, _, _>(spsc::bounded_blocking::<LargeMsg>(1));
}

#[test]
fn test_drop_large_msg_mpsc_1() {
    _test_drop_msg::<LargeMsg, _, _>(mpsc::bounded_blocking::<LargeMsg>(1));
}

#[test]
fn test_drop_large_msg_mpmc_1() {
    _test_drop_msg::<LargeMsg, _, _>(mpmc::bounded_blocking::<LargeMsg>(1));
}
