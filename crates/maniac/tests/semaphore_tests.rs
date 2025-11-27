use maniac::future::block_on;
use maniac::runtime::{new_multi_threaded, new_single_threaded};
use maniac::sync::Semaphore;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_semaphore_acquire() {
    let rt = new_single_threaded().unwrap();
    let handle = rt
        .spawn(async {
            let sem = Semaphore::new(2);
            let p1 = sem.acquire().await;
            let p2 = sem.acquire().await;
            assert_eq!(sem.available_permits(), 0);
            drop(p1);
            assert_eq!(sem.available_permits(), 1);
            drop(p2);
            assert_eq!(sem.available_permits(), 2);
        })
        .unwrap();
    block_on(handle);
}

#[test]
fn test_semaphore_try_acquire() {
    let sem = Semaphore::new(1);
    let p1 = sem.try_acquire();
    assert!(p1.is_some());
    let p2 = sem.try_acquire();
    assert!(p2.is_none());
    drop(p1);
    let p3 = sem.try_acquire();
    assert!(p3.is_some());
}

#[test]
fn test_semaphore_add_permits() {
    let rt = new_single_threaded().unwrap();

    let sem = Arc::new(Semaphore::new(0));
    let sem_clone = sem.clone();

    let handle = rt
        .spawn(async move {
            let _p = sem_clone.acquire().await;
        })
        .unwrap();

    std::thread::sleep(Duration::from_millis(10));
    sem.add_permits(1);

    block_on(handle);
}

#[test]
fn test_acquire_owned() {
    let rt = new_single_threaded().unwrap();
    let handle = rt
        .spawn(async {
            let sem = Arc::new(Semaphore::new(1));
            let p = sem.clone().acquire_owned().await;
            assert_eq!(sem.available_permits(), 0);
            drop(p);
            assert_eq!(sem.available_permits(), 1);
        })
        .unwrap();
    block_on(handle);
}

#[test]
fn test_semaphore_contention() {
    let rt = new_multi_threaded(2, 0, 1, 0).unwrap();

    let sem = Arc::new(Semaphore::new(1));
    let sem_clone = sem.clone();

    let handle = rt
        .spawn(async move {
            let _p = sem_clone.acquire().await;
            maniac::time::sleep(Duration::from_millis(50)).await;
            // std::thread::sleep(Duration::from_millis(50));
        })
        .unwrap();

    // Wait a bit to ensure the task has acquired the permit
    std::thread::sleep(Duration::from_millis(10));

    let handle2 = rt
        .spawn(async move {
            // This should block until the spawned task releases the permit
            let _p = sem.acquire().await;
        })
        .unwrap();

    block_on(handle);
    block_on(handle2);
}
