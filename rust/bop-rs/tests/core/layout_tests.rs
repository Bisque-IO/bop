use std::sync::Arc;

use bop_rs::aof::Aof;
use bop_rs::aof::archive_fs::FilesystemArchiveStorage;
use bop_rs::aof::error::{AofError, AofResult};
use bop_rs::aof::filesystem::AsyncFileSystem;
use bop_rs::aof::index::SegmentFooter;
use bop_rs::aof::record::{AofConfig, FlushCheckpointStamp, FlushStrategy, SegmentMetadataRecord};
use tempfile::TempDir;

fn new_tempdir() -> AofResult<TempDir> {
    tempfile::tempdir().map_err(|e| AofError::FileSystem(e.to_string()))
}

fn build_config(segment_size: u64, index_path: String) -> AofConfig {
    let mut config = AofConfig::default();
    config.segment_size = segment_size;
    config.segment_cache_size = 4;
    config.flush_strategy = FlushStrategy::Sync;
    config.archive_enabled = false;
    config.index_path = Some(index_path);
    config
}

fn find_segment_file(temp: &TempDir) -> AofResult<std::path::PathBuf> {
    let data_dir = temp.path().join("data");
    let entries = std::fs::read_dir(&data_dir)
        .map_err(|e| AofError::FileSystem(format!("failed to read data dir: {}", e)))?;

    let mut candidates: Vec<(u64, u64, std::path::PathBuf)> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let file_name = path.file_name()?.to_str()?;
            if !path.is_file() || !file_name.starts_with("segment_") || !file_name.ends_with(".log")
            {
                return None;
            }

            let numeric = file_name
                .trim_start_matches("segment_")
                .trim_end_matches(".log")
                .parse::<u64>()
                .ok()?;

            let size = std::fs::metadata(&path).ok()?.len();

            Some((numeric, size, path))
        })
        .collect();

    candidates.sort_by_key(|(_, size, _)| *size);

    candidates
        .into_iter()
        .find(|(_, size, _)| *size > 0)
        .map(|(_, _, path)| path)
        .ok_or_else(|| AofError::FileSystem("no segment files found".to_string()))
}

async fn create_aof(temp: &TempDir) -> AofResult<Aof<AsyncFileSystem, FilesystemArchiveStorage>> {
    let data_dir = temp.path().join("data");
    let archive_dir = temp.path().join("archive");
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| AofError::FileSystem(format!("failed to create data dir: {}", e)))?;
    std::fs::create_dir_all(&archive_dir)
        .map_err(|e| AofError::FileSystem(format!("failed to create archive dir: {}", e)))?;

    let data_fs = AsyncFileSystem::new(&data_dir)?;
    let archive_fs = Arc::new(AsyncFileSystem::new(&archive_dir)?);
    let archive_storage =
        Arc::new(FilesystemArchiveStorage::new(Arc::clone(&archive_fs), "segments").await?);

    let index_path = archive_dir
        .join("segment_index.mdbx")
        .to_string_lossy()
        .into_owned();
    let config = build_config(16 * 1024, index_path);

    Aof::open_with_fs_and_config(data_fs, archive_storage, config).await
}

#[tokio::test(flavor = "multi_thread")]
async fn finalization_writes_metadata_and_footer() -> AofResult<()> {
    let temp = new_tempdir()?;
    let mut aof = create_aof(&temp).await?;

    aof.append(b"alpha")?;
    aof.flush().await?;
    aof.close().await?;

    let segment_path = find_segment_file(&temp)?;
    let bytes = std::fs::read(&segment_path)
        .map_err(|e| AofError::FileSystem(format!("failed to read segment: {}", e)))?;
    assert!(
        bytes.iter().any(|&b| b != 0),
        "segment file appears empty: {}",
        segment_path.display()
    );

    let footer_size = SegmentFooter::size();
    let footer_start = bytes.len() - footer_size;
    let footer = SegmentFooter::deserialize(&bytes[footer_start..])?;
    assert!(footer.verify(), "footer verification failed: {:?}", footer);
    assert_eq!(footer.record_count, 1);
    assert_eq!(footer.index_size, 0);

    let metadata_offset = footer.index_offset as usize - SegmentMetadataRecord::BYTE_SIZE;
    let metadata_bytes =
        &bytes[metadata_offset..metadata_offset + SegmentMetadataRecord::BYTE_SIZE];
    let metadata_record = SegmentMetadataRecord::from_bytes(metadata_bytes)
        .map_err(|e| AofError::Serialization(format!("failed to decode metadata: {}", e)))?;
    assert!(metadata_record.is_valid());
    assert_eq!(metadata_record.record_count, 1);
    assert_eq!(metadata_record.last_record_id, 1);

    let stamp_magic = FlushCheckpointStamp::MAGIC.to_le_bytes();
    let stamp_count = bytes
        .windows(stamp_magic.len())
        .filter(|window| *window == stamp_magic)
        .count();
    assert_eq!(
        stamp_count, 1,
        "finalization should not append a second checkpoint stamp"
    );

    assert_eq!(footer_start, footer.index_offset as usize);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn flush_appends_single_checkpoint_without_metadata() -> AofResult<()> {
    let temp = new_tempdir()?;
    let mut aof = create_aof(&temp).await?;

    aof.append(b"one")?;
    aof.append(b"two")?;
    aof.flush().await?;

    let segment_path = find_segment_file(&temp)?;
    let bytes = std::fs::read(&segment_path)
        .map_err(|e| AofError::FileSystem(format!("failed to read segment: {}", e)))?;

    let stamp_magic = FlushCheckpointStamp::MAGIC.to_le_bytes();
    let stamp_count = bytes
        .windows(stamp_magic.len())
        .filter(|window| *window == stamp_magic)
        .count();
    assert_eq!(
        stamp_count, 1,
        "flush should append exactly one checkpoint stamp"
    );

    let metadata_magic = SegmentMetadataRecord::MAGIC.to_le_bytes();
    let metadata_present = bytes
        .windows(metadata_magic.len())
        .any(|window| window == metadata_magic);
    assert!(
        !metadata_present,
        "metadata record should only appear after finalization"
    );

    assert!(SegmentFooter::read_from_end(&bytes[..]).is_err());

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn reader_from_id_streams_records_in_order() -> AofResult<()> {
    let temp = new_tempdir()?;
    let mut aof = create_aof(&temp).await?;

    let first = aof.append(b"alpha")?;
    let second = aof.append(b"bravo")?;
    let third = aof.append(b"charlie")?;
    aof.flush().await?;

    assert_eq!(first, 1);
    assert_eq!(second, 2);
    assert_eq!(third, 3);
    assert_eq!(aof.next_id(), 4);

    let mut reader = aof.create_reader_from_id(2)?;
    let record = reader
        .read_next_record()
        .await?
        .expect("expected record 2 to be available");
    assert_eq!(record, b"bravo");
    assert_eq!(reader.current_record_id(), 2);

    let record = reader
        .read_next_record()
        .await?
        .expect("expected record 3 to be available");
    assert_eq!(record, b"charlie");
    assert_eq!(reader.current_record_id(), 3);

    assert!(reader.read_next_record().await?.is_none());

    aof.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn reader_from_future_id_observes_empty_stream() -> AofResult<()> {
    let temp = new_tempdir()?;
    let mut aof = create_aof(&temp).await?;

    aof.append(b"delta")?;
    aof.flush().await?;

    let mut reader = aof.create_reader_from_id(99)?;
    assert!(reader.read_next_record().await?.is_none());
    assert!(reader.is_at_end());

    aof.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn tail_reader_picks_up_new_appends() -> AofResult<()> {
    let temp = new_tempdir()?;
    let mut aof = create_aof(&temp).await?;

    aof.append(b"echo")?;
    aof.flush().await?;

    let mut reader = aof.create_reader_at_tail()?;
    assert!(reader.read_next_record().await?.is_none());

    let payload = b"foxtrot";
    aof.append(payload)?;
    aof.flush().await?;

    let observed = aof
        .tail_next_record(&mut reader)
        .await?
        .expect("tail reader should see new record");
    assert_eq!(observed, payload);
    assert!(reader.is_at_end());

    aof.close().await?;
    Ok(())
}
