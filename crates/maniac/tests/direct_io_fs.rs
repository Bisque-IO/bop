use std::path::PathBuf;

use maniac::{
    buf::AlignedBuf,
    buf::IoBufMut,
    fs::OpenOptions,
    future::block_on,
    runtime::{
        DefaultExecutor,
        task::{TaskArenaConfig, TaskArenaOptions},
    },
};

fn tmp_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    p.push(format!("maniac-direct-io-{}-{}-{}", std::process::id(), nanos, name));
    p
}

fn run_async<T: Send + 'static>(fut: impl std::future::Future<Output = T> + Send + 'static) -> T {
    let runtime = DefaultExecutor::new(
        TaskArenaConfig::new(2, 4096).unwrap(),
        TaskArenaOptions::default(),
        1,
    )
    .unwrap();
    let handle = runtime.spawn(fut).unwrap();
    block_on(handle)
}

async fn cleanup(path: &PathBuf) {
    // Cleanup (best-effort).
    #[cfg(feature = "unlinkat")]
    let _ = maniac::fs::remove_file(path).await;
    #[cfg(not(feature = "unlinkat"))]
    let _ = maniac::blocking::unblock({
        let path = path.clone();
        move || std::fs::remove_file(path)
    })
    .await;
}

async fn open_direct_file(path: &PathBuf) -> Option<maniac::fs::File> {
    match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .direct_io(true)
        .open(path)
        .await
    {
        Ok(f) => Some(f),
        Err(e) if matches!(e.raw_os_error(), Some(libc::EINVAL) | Some(libc::EOPNOTSUPP)) => {
            None
        }
        Err(e) => panic!("open failed: {e:?}"),
    }
}

#[cfg(unix)]
#[test]
fn direct_io_unaligned_helpers_roundtrip() {
    run_async(async move {
        let path = tmp_path("roundtrip.bin");
        let Some(file) = open_direct_file(&path).await else { return };

        file.write_all_at_unaligned(1, b"abc").await.unwrap();

        // Allocate scratch once; caller can reuse across reads.
        let mut scratch = AlignedBuf::new(4096, 4096);
        let mut out = [0u8; 3];
        file.read_exact_at_unaligned_into(1, &mut out, &mut scratch)
            .await
            .unwrap();
        assert_eq!(&out, b"abc");

        let got = file.read_exact_at_unaligned(1, 3).await.unwrap();
        assert_eq!(got, b"abc");

        cleanup(&path).await;
    });
}

#[cfg(unix)]
#[test]
fn direct_io_aligned_read_write_with_alignedbuf() {
    run_async(async move {
        let path = tmp_path("aligned.bin");
        let Some(file) = open_direct_file(&path).await else { return };

        let req = file.direct_io_requirements();
        let a = req.alignment.max(1);

        // Write a single aligned block.
        let mut w = AlignedBuf::new(a, a);
        w.as_mut_slice_total().fill(0xAB);
        unsafe { w.set_init(a) };
        let (res, _w) = file.write_all_at(w, 0).await;
        if res.is_err() {
            cleanup(&path).await;
            return;
        }

        // Read it back via aligned direct read.
        let r = AlignedBuf::new(a, a);
        let (res, mut r) = file.read_exact_at(r, 0).await;
        res.unwrap();
        unsafe { r.set_init(a) };
        assert!(r.as_slice().iter().all(|b| *b == 0xAB));

        cleanup(&path).await;
    });
}

#[cfg(unix)]
#[test]
fn direct_io_read_exact_at_unaligned_into_reuses_scratch() {
    run_async(async move {
        let path = tmp_path("reuse.bin");
        let Some(file) = open_direct_file(&path).await else { return };

        file.write_all_at_unaligned(1, b"abcdef").await.unwrap();

        let mut scratch = AlignedBuf::new(4096, 4096);
        let cap0 = scratch.capacity();

        let mut out1 = [0u8; 3];
        file.read_exact_at_unaligned_into(1, &mut out1, &mut scratch)
            .await
            .unwrap();
        assert_eq!(&out1, b"abc");
        assert_eq!(scratch.capacity(), cap0);

        let mut out2 = [0u8; 6];
        file.read_exact_at_unaligned_into(1, &mut out2, &mut scratch)
            .await
            .unwrap();
        assert_eq!(&out2, b"abcdef");
        assert_eq!(scratch.capacity(), cap0);

        cleanup(&path).await;
    });
}

#[cfg(unix)]
#[test]
fn direct_io_write_all_at_unaligned_preserves_surrounding_bytes() {
    run_async(async move {
        let path = tmp_path("rmw.bin");
        let Some(file) = open_direct_file(&path).await else { return };

        // Initialize first aligned block to zeros (ensures file exists and avoids EOF).
        let mut zero = AlignedBuf::new(4096, 4096);
        zero.as_mut_slice_total().fill(0);
        unsafe { zero.set_init(4096) };
        let (res, _zero) = file.write_all_at(zero, 0).await;
        if res.is_err() {
            cleanup(&path).await;
            return;
        }

        file.write_all_at_unaligned(1, b"abc").await.unwrap();

        let got = file.read_exact_at_unaligned(0, 4096).await.unwrap();
        assert_eq!(got[0], 0);
        assert_eq!(&got[1..4], b"abc");
        assert!(got[4..].iter().all(|b| *b == 0));

        cleanup(&path).await;
    });
}

#[cfg(unix)]
#[test]
fn direct_io_read_exact_at_unaligned_eof() {
    run_async(async move {
        let path = tmp_path("eof.bin");
        let Some(file) = open_direct_file(&path).await else { return };

        file.write_all_at_unaligned(1, b"abc").await.unwrap();

        let err = file.read_exact_at_unaligned(4090, 16).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);

        cleanup(&path).await;
    });
}

#[cfg(unix)]
#[test]
fn direct_io_requirements_sane() {
    run_async(async move {
        let path = tmp_path("reqs.bin");
        let Some(file) = open_direct_file(&path).await else { return };

        let req = file.direct_io_requirements();
        assert!(req.alignment >= 1);
        assert!(req.alignment.is_power_of_two());

        #[cfg(target_os = "linux")]
        assert_eq!(req.alignment, 4096);

        cleanup(&path).await;
    });
}

#[cfg(unix)]
#[test]
fn direct_io_unaligned_into_can_grow_scratch() {
    run_async(async move {
        let path = tmp_path("grow.bin");
        let Some(file) = open_direct_file(&path).await else { return };

        file.write_all_at_unaligned(1, b"abcdef").await.unwrap();

        let mut scratch = AlignedBuf::default(); // starts empty
        assert_eq!(scratch.capacity(), 0);

        let mut out = [0u8; 6];
        file.read_exact_at_unaligned_into(1, &mut out, &mut scratch)
            .await
            .unwrap();
        assert_eq!(&out, b"abcdef");
        assert!(scratch.capacity() >= 4096, "scratch should have grown to at least one block");

        // Subsequent call should reuse existing allocation (capacity should not shrink).
        let cap = scratch.capacity();
        let mut out2 = [0u8; 3];
        file.read_exact_at_unaligned_into(1, &mut out2, &mut scratch)
            .await
            .unwrap();
        assert_eq!(&out2, b"abc");
        assert_eq!(scratch.capacity(), cap);

        cleanup(&path).await;
    });
}

