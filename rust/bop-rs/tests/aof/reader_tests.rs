use std::sync::Arc;
use std::sync::atomic::Ordering;

use bop_rs::aof::error::{AofError, AofResult};
use bop_rs::aof::filesystem::{FileHandle, FileMetadata, FileSystem};
use bop_rs::aof::reader::{BinaryIndex, BinaryIndexEntry, Reader};
use memmap2::{Mmap, MmapMut};
use tokio::sync::Notify;
use tokio::time::{Duration, sleep};

#[derive(Clone)]
struct StubHandle;

#[allow(async_fn_in_trait)]
impl FileHandle for StubHandle {
    async fn read_at(&mut self, _offset: u64, _buffer: &mut [u8]) -> AofResult<usize> {
        Err(AofError::FileSystem("stub handle".to_string()))
    }

    async fn write_at(&mut self, _offset: u64, _data: &[u8]) -> AofResult<usize> {
        Err(AofError::FileSystem("stub handle".to_string()))
    }

    async fn set_size(&self, _size: u64) -> AofResult<()> {
        Err(AofError::FileSystem("stub handle".to_string()))
    }

    async fn sync(&self) -> AofResult<()> {
        Err(AofError::FileSystem("stub handle".to_string()))
    }

    async fn flush(&mut self) -> AofResult<()> {
        Err(AofError::FileSystem("stub handle".to_string()))
    }

    async fn size(&self) -> AofResult<u64> {
        Ok(0)
    }

    async fn read_all(&mut self) -> AofResult<Vec<u8>> {
        Ok(Vec::new())
    }

    async fn write_all(&mut self, _data: &[u8]) -> AofResult<()> {
        Err(AofError::FileSystem("stub handle".to_string()))
    }

    async fn memory_map(&self) -> AofResult<Option<Mmap>> {
        Ok(None)
    }

    async fn memory_map_mut(&self) -> AofResult<Option<MmapMut>> {
        Ok(None)
    }
}

struct StubFileSystem;

#[allow(async_fn_in_trait)]
impl FileSystem for StubFileSystem {
    type Handle = StubHandle;

    async fn create_file(&self, _path: &str) -> AofResult<Self::Handle> {
        Err(AofError::FileSystem("stub fs".to_string()))
    }

    async fn open_file(&self, _path: &str) -> AofResult<Self::Handle> {
        Err(AofError::FileSystem("stub fs".to_string()))
    }

    async fn open_file_mut(&self, _path: &str) -> AofResult<Self::Handle> {
        Err(AofError::FileSystem("stub fs".to_string()))
    }

    async fn delete_file(&self, _path: &str) -> AofResult<()> {
        Err(AofError::FileSystem("stub fs".to_string()))
    }

    async fn file_exists(&self, _path: &str) -> AofResult<bool> {
        Ok(false)
    }

    async fn file_metadata(&self, _path: &str) -> AofResult<FileMetadata> {
        Err(AofError::FileSystem("stub fs".to_string()))
    }

    async fn list_files(&self, _pattern: &str) -> AofResult<Vec<String>> {
        Ok(Vec::new())
    }
}

#[test]
fn binary_index_entry_roundtrip() {
    let entry = BinaryIndexEntry::new(42, 1_024);
    let bytes = entry.to_bytes();
    let decoded = BinaryIndexEntry::from_bytes(&bytes).expect("deserialize entry");

    assert_eq!(decoded.record_id, 42);
    assert_eq!(decoded.file_offset, 1_024);
}

#[test]
fn binary_index_entry_rejects_short_payload() {
    let bytes = [0u8; BinaryIndexEntry::SIZE - 1];
    let err = BinaryIndexEntry::from_bytes(&bytes).expect_err("expected failure");
    assert!(matches!(err, AofError::CorruptedRecord(_)));
}

#[test]
fn binary_index_finds_offsets_for_known_ids() {
    let mut index = BinaryIndex::new();
    index.add_entry(BinaryIndexEntry::new(10, 64));
    index.add_entry(BinaryIndexEntry::new(20, 128));
    index.add_entry(BinaryIndexEntry::new(30, 256));

    assert_eq!(index.find_offset(10), Some(64));
    assert_eq!(index.find_offset(25), None);
    assert_eq!(index.find_offset(30), Some(256));
    assert_eq!(index.entries().len(), 3);
}

#[tokio::test]
async fn wait_for_tail_notification_requires_notify_handle() {
    let reader: Reader<StubFileSystem> = Reader::new(1);
    let err = reader
        .wait_for_tail_notification()
        .await
        .expect_err("expected invalid state when no notify is set");
    assert!(matches!(err, AofError::InvalidState(_)));
}

#[tokio::test]
async fn wait_for_tail_notification_completes_after_signal() {
    let mut reader: Reader<StubFileSystem> = Reader::new(2);
    let notify = Arc::new(Notify::new());
    reader.set_tail_notify(notify.clone());
    reader.is_tailing.store(true, Ordering::SeqCst);

    let signal = notify.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(5)).await;
        signal.notify_one();
    });

    reader
        .wait_for_tail_notification()
        .await
        .expect("wait_for_tail_notification should succeed once notified");
}

#[tokio::test]
async fn tail_next_record_returns_none_when_not_tailing() {
    let mut reader: Reader<StubFileSystem> = Reader::new(3);
    reader.is_tailing.store(false, Ordering::SeqCst);

    let result = reader
        .tail_next_record()
        .await
        .expect("tail_next_record should complete successfully");
    assert!(result.is_none());
}
