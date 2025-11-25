use maniac::blocking;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd};

#[cfg(unix)]
fn create_pipe() -> std::io::Result<(File, File)> {
    unsafe {
        let mut fds = [0 as libc::c_int; 2];
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok((File::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])))
    }
}

#[test]
#[cfg(unix)]
fn test_blocking_cancellation_unix_pipe() {
    // 1. Create a pipe
    // We keep _tx_pipe open so the reader blocks indefinitely waiting for data
    let (rx_pipe, _tx_pipe) = create_pipe().expect("failed to create pipe");
    let fd = rx_pipe.as_raw_fd();

    // 2. Buffer
    let mut buffer = vec![0u8; 1024];

    println!("Spawning task...");
    // Safety: buffer outlives the task because we block/drop the task within this scope
    // before buffer is dropped.
    let task = unsafe { blocking::unblock_fread(fd, buffer.as_mut_ptr(), buffer.len()) };

    // 3. Sleep to let it block on read()
    thread::sleep(Duration::from_millis(100));

    println!("Dropping task (should cancel)...");
    let start = Instant::now();
    // 4. Drop the task.
    // The reader is blocked on read().
    // Drop should trigger SIGUSR1.
    // sys::read loop should catch EINTR, check is_closed(), and return Err.
    // Task::drop loop should see !EXECUTING (eventually) and finish.
    drop(task);
    let elapsed = start.elapsed();
    println!("Dropped in {:?}", elapsed);

    // 5. Ensure it returned quickly (e.g. < 1s)
    // Without cancellation, it would hang forever (until pipe closed, but we hold tx_pipe).
    assert!(
        elapsed < Duration::from_secs(1),
        "Cancellation took too long"
    );
}

#[test]
#[cfg(unix)]
fn test_blocking_read_success() {
    let (rx_pipe, mut tx_pipe) = create_pipe().expect("failed to create pipe");
    let fd = rx_pipe.as_raw_fd();
    let mut buffer = vec![0u8; 1024];

    // Spawn read
    let task = unsafe { blocking::unblock_fread(fd, buffer.as_mut_ptr(), buffer.len()) };

    // Write to pipe
    tx_pipe.write_all(b"hello").expect("write failed");

    // Wait for result
    let res = futures_lite::future::block_on(task).expect("read failed");
    assert_eq!(res, 5);
    assert_eq!(&buffer[0..5], b"hello");
}

#[test]
fn test_blocking_execute_safety() {
    // Basic test for Execute variant (Safe)
    // This task uses a closure, so it's "Safe" regarding UAF (owns data).
    // Dropping it should be instant (detach), not spin-wait.
    let task = blocking::unblock(|| {
        // Simulate long work
        thread::sleep(Duration::from_millis(200));
        42
    });

    // Drop immediately. Should not block/spin.
    let start = Instant::now();
    drop(task);
    let elapsed = start.elapsed();
    println!("Dropped safe task in {:?}", elapsed);
    // It should be instant (well under the 200ms sleep time).
    assert!(elapsed < Duration::from_millis(50));
}
