use std::path::PathBuf;
use std::time::SystemTime;

use bop_rs::aof::filesystem::{AsyncFileHandle, AsyncFileSystem, FileSystem};
use tokio::io::AsyncReadExt;
use tokio::time::{Duration, sleep};

#[tokio::test]
async fn async_filesystem_roundtrip_and_listing() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let fs = AsyncFileSystem::new(temp_dir.path()).expect("init fs");

    // Create a file and write content
    let mut handle = fs.create_file("alpha.txt").await.expect("create file");
    handle.write_all(b"alpha").await.expect("write alpha");
    handle.flush().await.expect("flush alpha");
    handle.sync().await.expect("sync alpha");

    // Metadata reflects write
    let metadata = fs.file_metadata("alpha.txt").await.expect("metadata");
    assert_eq!(metadata.size, 5);
    assert!(metadata.modified >= metadata.created);

    // Listing should include the file
    let mut files = fs
        .list_files(temp_dir.path().join("*").to_str().unwrap())
        .await
        .expect("list files");
    files.sort();
    assert!(files.iter().any(|name| name == "alpha.txt"));

    // Move it and confirm original is gone
    fs.move_file("alpha.txt", "beta/alpha.txt")
        .await
        .expect("move file");
    assert!(!fs.file_exists("alpha.txt").await.expect("exists"));
    assert!(
        fs.file_exists("beta/alpha.txt")
            .await
            .expect("exists at new path")
    );

    // Read back via open_file
    let mut handle = fs
        .open_file("beta/alpha.txt")
        .await
        .expect("open moved file");
    let data = handle.read_all().await.expect("read moved data");
    assert_eq!(data, b"alpha");

    // Copy and delete
    fs.copy_file("beta/alpha.txt", "gamma/copy.txt")
        .await
        .expect("copy file");
    assert!(
        fs.file_exists("gamma/copy.txt")
            .await
            .expect("copied file exists")
    );
    fs.delete_file("beta/alpha.txt")
        .await
        .expect("delete original");
    assert!(!fs.file_exists("beta/alpha.txt").await.expect("deleted"));
}

#[tokio::test]
async fn async_file_handle_random_access() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let fs = AsyncFileSystem::new(temp_dir.path()).expect("init fs");

    let mut handle = fs.create_file("random.bin").await.expect("create file");
    handle.set_size(32).await.expect("set size");
    handle.write_at(4, b"abc").await.expect("write at offset");
    handle.write_at(16, b"xyz").await.expect("write at offset");
    handle.flush().await.expect("flush");
    handle.sync().await.expect("sync");

    let mut reopen = fs.open_file_mut("random.bin").await.expect("open mut");
    let mut buf = vec![0u8; 3];
    reopen.read_at(4, &mut buf).await.expect("read abc");
    assert_eq!(buf, b"abc");
    reopen.read_at(16, &mut buf).await.expect("read xyz");
    assert_eq!(buf, b"xyz");
}
