//! Async filesystem abstraction for AOF operations
//!
//! This module provides a clean, fully async filesystem interface without
//! mixed concerns like remote storage or archiving.

use hdrhistogram::Histogram;
use memmap2::{Mmap, MmapMut};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};

use crate::aof::error::{AofError, AofResult};

/// Comprehensive filesystem metrics using HDR histograms
#[derive(Debug)]
pub struct FileSystemMetrics {
    // Operation counters
    pub create_file_count: AtomicU64,
    pub open_file_count: AtomicU64,
    pub delete_file_count: AtomicU64,
    pub read_at_count: AtomicU64,
    pub write_at_count: AtomicU64,
    pub read_all_count: AtomicU64,
    pub write_all_count: AtomicU64,
    pub sync_count: AtomicU64,
    pub flush_count: AtomicU64,
    pub memory_map_count: AtomicU64,
    pub memory_map_mut_count: AtomicU64,
    pub file_metadata_count: AtomicU64,
    pub file_exists_count: AtomicU64,
    pub list_files_count: AtomicU64,
    pub set_size_count: AtomicU64,

    // Error counters
    pub create_file_errors: AtomicU64,
    pub open_file_errors: AtomicU64,
    pub delete_file_errors: AtomicU64,
    pub read_errors: AtomicU64,
    pub write_errors: AtomicU64,
    pub sync_errors: AtomicU64,
    pub memory_map_errors: AtomicU64,

    // Data volume counters
    pub bytes_read: AtomicU64,
    pub bytes_written: AtomicU64,

    // Latency histograms (microseconds, 1 to 60 seconds range)
    pub create_file_latency: Mutex<Histogram<u64>>,
    pub open_file_latency: Mutex<Histogram<u64>>,
    pub delete_file_latency: Mutex<Histogram<u64>>,
    pub read_at_latency: Mutex<Histogram<u64>>,
    pub write_at_latency: Mutex<Histogram<u64>>,
    pub read_all_latency: Mutex<Histogram<u64>>,
    pub write_all_latency: Mutex<Histogram<u64>>,
    pub sync_latency: Mutex<Histogram<u64>>,
    pub flush_latency: Mutex<Histogram<u64>>,
    pub memory_map_latency: Mutex<Histogram<u64>>,
    pub memory_map_mut_latency: Mutex<Histogram<u64>>,
    pub file_metadata_latency: Mutex<Histogram<u64>>,
    pub file_exists_latency: Mutex<Histogram<u64>>,
    pub list_files_latency: Mutex<Histogram<u64>>,
    pub set_size_latency: Mutex<Histogram<u64>>,
}

impl Default for FileSystemMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystemMetrics {
    /// Create new filesystem metrics with pre-configured histograms
    pub fn new() -> Self {
        Self {
            // Initialize counters
            create_file_count: AtomicU64::new(0),
            open_file_count: AtomicU64::new(0),
            delete_file_count: AtomicU64::new(0),
            read_at_count: AtomicU64::new(0),
            write_at_count: AtomicU64::new(0),
            read_all_count: AtomicU64::new(0),
            write_all_count: AtomicU64::new(0),
            sync_count: AtomicU64::new(0),
            flush_count: AtomicU64::new(0),
            memory_map_count: AtomicU64::new(0),
            memory_map_mut_count: AtomicU64::new(0),
            file_metadata_count: AtomicU64::new(0),
            file_exists_count: AtomicU64::new(0),
            list_files_count: AtomicU64::new(0),
            set_size_count: AtomicU64::new(0),

            // Initialize error counters
            create_file_errors: AtomicU64::new(0),
            open_file_errors: AtomicU64::new(0),
            delete_file_errors: AtomicU64::new(0),
            read_errors: AtomicU64::new(0),
            write_errors: AtomicU64::new(0),
            sync_errors: AtomicU64::new(0),
            memory_map_errors: AtomicU64::new(0),

            // Initialize data volume counters
            bytes_read: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),

            // Initialize histograms (1 microsecond to 60 seconds, 3 significant digits)
            create_file_latency: Mutex::new(Histogram::new(3).unwrap()),
            open_file_latency: Mutex::new(Histogram::new(3).unwrap()),
            delete_file_latency: Mutex::new(Histogram::new(3).unwrap()),
            read_at_latency: Mutex::new(Histogram::new(3).unwrap()),
            write_at_latency: Mutex::new(Histogram::new(3).unwrap()),
            read_all_latency: Mutex::new(Histogram::new(3).unwrap()),
            write_all_latency: Mutex::new(Histogram::new(3).unwrap()),
            sync_latency: Mutex::new(Histogram::new(3).unwrap()),
            flush_latency: Mutex::new(Histogram::new(3).unwrap()),
            memory_map_latency: Mutex::new(Histogram::new(3).unwrap()),
            memory_map_mut_latency: Mutex::new(Histogram::new(3).unwrap()),
            file_metadata_latency: Mutex::new(Histogram::new(3).unwrap()),
            file_exists_latency: Mutex::new(Histogram::new(3).unwrap()),
            list_files_latency: Mutex::new(Histogram::new(3).unwrap()),
            set_size_latency: Mutex::new(Histogram::new(3).unwrap()),
        }
    }

    /// Record operation latency in microseconds
    fn record_latency(&self, histogram: &Mutex<Histogram<u64>>, start_time: Instant) {
        let duration_us = start_time.elapsed().as_micros() as u64;
        if let Ok(mut hist) = histogram.lock() {
            let _ = hist.record(duration_us);
        }
    }

    /// Get percentile latency from histogram in microseconds
    pub fn get_latency_percentile(
        &self,
        histogram: &Mutex<Histogram<u64>>,
        percentile: f64,
    ) -> Option<u64> {
        Some(histogram.lock().ok()?.value_at_percentile(percentile))
    }

    /// Get total operation count
    pub fn total_operations(&self) -> u64 {
        self.create_file_count.load(Ordering::Relaxed)
            + self.open_file_count.load(Ordering::Relaxed)
            + self.delete_file_count.load(Ordering::Relaxed)
            + self.read_at_count.load(Ordering::Relaxed)
            + self.write_at_count.load(Ordering::Relaxed)
            + self.read_all_count.load(Ordering::Relaxed)
            + self.write_all_count.load(Ordering::Relaxed)
            + self.sync_count.load(Ordering::Relaxed)
            + self.flush_count.load(Ordering::Relaxed)
            + self.memory_map_count.load(Ordering::Relaxed)
            + self.memory_map_mut_count.load(Ordering::Relaxed)
            + self.file_metadata_count.load(Ordering::Relaxed)
            + self.file_exists_count.load(Ordering::Relaxed)
            + self.list_files_count.load(Ordering::Relaxed)
            + self.set_size_count.load(Ordering::Relaxed)
    }

    /// Get total error count
    pub fn total_errors(&self) -> u64 {
        self.create_file_errors.load(Ordering::Relaxed)
            + self.open_file_errors.load(Ordering::Relaxed)
            + self.delete_file_errors.load(Ordering::Relaxed)
            + self.read_errors.load(Ordering::Relaxed)
            + self.write_errors.load(Ordering::Relaxed)
            + self.sync_errors.load(Ordering::Relaxed)
            + self.memory_map_errors.load(Ordering::Relaxed)
    }

    /// Get error rate as percentage
    pub fn error_rate(&self) -> f64 {
        let total_ops = self.total_operations();
        if total_ops == 0 {
            0.0
        } else {
            (self.total_errors() as f64 / total_ops as f64) * 100.0
        }
    }

    /// Reset all metrics
    pub fn reset(&self) {
        // Reset counters
        self.create_file_count.store(0, Ordering::Relaxed);
        self.open_file_count.store(0, Ordering::Relaxed);
        self.delete_file_count.store(0, Ordering::Relaxed);
        self.read_at_count.store(0, Ordering::Relaxed);
        self.write_at_count.store(0, Ordering::Relaxed);
        self.read_all_count.store(0, Ordering::Relaxed);
        self.write_all_count.store(0, Ordering::Relaxed);
        self.sync_count.store(0, Ordering::Relaxed);
        self.flush_count.store(0, Ordering::Relaxed);
        self.memory_map_count.store(0, Ordering::Relaxed);
        self.memory_map_mut_count.store(0, Ordering::Relaxed);
        self.file_metadata_count.store(0, Ordering::Relaxed);
        self.file_exists_count.store(0, Ordering::Relaxed);
        self.list_files_count.store(0, Ordering::Relaxed);
        self.set_size_count.store(0, Ordering::Relaxed);

        // Reset error counters
        self.create_file_errors.store(0, Ordering::Relaxed);
        self.open_file_errors.store(0, Ordering::Relaxed);
        self.delete_file_errors.store(0, Ordering::Relaxed);
        self.read_errors.store(0, Ordering::Relaxed);
        self.write_errors.store(0, Ordering::Relaxed);
        self.sync_errors.store(0, Ordering::Relaxed);
        self.memory_map_errors.store(0, Ordering::Relaxed);

        // Reset data volume
        self.bytes_read.store(0, Ordering::Relaxed);
        self.bytes_written.store(0, Ordering::Relaxed);

        // Reset histograms
        let _ = self.create_file_latency.lock().map(|mut h| h.clear());
        let _ = self.open_file_latency.lock().map(|mut h| h.clear());
        let _ = self.delete_file_latency.lock().map(|mut h| h.clear());
        let _ = self.read_at_latency.lock().map(|mut h| h.clear());
        let _ = self.write_at_latency.lock().map(|mut h| h.clear());
        let _ = self.read_all_latency.lock().map(|mut h| h.clear());
        let _ = self.write_all_latency.lock().map(|mut h| h.clear());
        let _ = self.sync_latency.lock().map(|mut h| h.clear());
        let _ = self.flush_latency.lock().map(|mut h| h.clear());
        let _ = self.memory_map_latency.lock().map(|mut h| h.clear());
        let _ = self.memory_map_mut_latency.lock().map(|mut h| h.clear());
        let _ = self.file_metadata_latency.lock().map(|mut h| h.clear());
        let _ = self.file_exists_latency.lock().map(|mut h| h.clear());
        let _ = self.list_files_latency.lock().map(|mut h| h.clear());
        let _ = self.set_size_latency.lock().map(|mut h| h.clear());
    }
}

/// Trait for file handle operations (sync and async compatible)
pub trait FileHandle: Send + Sync {
    async fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> AofResult<usize>;
    async fn write_at(&mut self, offset: u64, data: &[u8]) -> AofResult<usize>;
    async fn set_size(&self, size: u64) -> AofResult<()>;
    async fn sync(&self) -> AofResult<()>;
    async fn flush(&mut self) -> AofResult<()>;
    async fn size(&self) -> AofResult<u64>;
    async fn read_all(&mut self) -> AofResult<Vec<u8>>;
    async fn write_all(&mut self, data: &[u8]) -> AofResult<()>;
    async fn memory_map(&self) -> AofResult<Option<Mmap>>;
    async fn memory_map_mut(&self) -> AofResult<Option<MmapMut>>;
}

/// Trait for filesystem operations (sync and async compatible)
pub trait FileSystem: Send + Sync {
    type Handle: FileHandle;

    async fn create_file(&self, path: &str) -> AofResult<Self::Handle>;
    async fn open_file(&self, path: &str) -> AofResult<Self::Handle>;
    async fn open_file_mut(&self, path: &str) -> AofResult<Self::Handle>;
    async fn delete_file(&self, path: &str) -> AofResult<()>;
    async fn file_exists(&self, path: &str) -> AofResult<bool>;
    async fn file_metadata(&self, path: &str) -> AofResult<FileMetadata>;
    async fn list_files(&self, pattern: &str) -> AofResult<Vec<String>>;
}

/// File metadata information
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub size: u64,
    pub created: SystemTime,
    pub modified: SystemTime,
}

/// Async file handle for file operations
pub struct AsyncFileHandle {
    file: File,
    path: String,
    metrics: Arc<FileSystemMetrics>,
}

impl AsyncFileHandle {
    /// Create a new async file handle
    pub(crate) fn new(file: File, path: String, metrics: Arc<FileSystemMetrics>) -> Self {
        Self {
            file,
            path,
            metrics,
        }
    }

    /// Read data from the file at a specific offset
    pub async fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> AofResult<usize> {
        let start_time = Instant::now();
        self.metrics.read_at_count.fetch_add(1, Ordering::Relaxed);

        let result = async {
            self.file.seek(SeekFrom::Start(offset)).await.map_err(|e| {
                AofError::FileSystem(format!("Failed to seek to {}: {}", offset, e))
            })?;

            self.file.read(buffer).await.map_err(|e| {
                AofError::FileSystem(format!("Failed to read from {}: {}", self.path, e))
            })
        }
        .await;

        // Record metrics
        self.metrics
            .record_latency(&self.metrics.read_at_latency, start_time);
        match &result {
            Ok(bytes_read) => {
                self.metrics
                    .bytes_read
                    .fetch_add(*bytes_read as u64, Ordering::Relaxed);
            }
            Err(_) => {
                self.metrics.read_errors.fetch_add(1, Ordering::Relaxed);
            }
        }

        result
    }

    /// Write data to the file at a specific offset
    pub async fn write_at(&mut self, offset: u64, data: &[u8]) -> AofResult<usize> {
        let start_time = Instant::now();
        self.metrics.write_at_count.fetch_add(1, Ordering::Relaxed);

        let result = async {
            self.file.seek(SeekFrom::Start(offset)).await.map_err(|e| {
                AofError::FileSystem(format!("Failed to seek to {}: {}", offset, e))
            })?;

            self.file.write(data).await.map_err(|e| {
                AofError::FileSystem(format!("Failed to write to {}: {}", self.path, e))
            })
        }
        .await;

        // Record metrics
        self.metrics
            .record_latency(&self.metrics.write_at_latency, start_time);
        match &result {
            Ok(bytes_written) => {
                self.metrics
                    .bytes_written
                    .fetch_add(*bytes_written as u64, Ordering::Relaxed);
            }
            Err(_) => {
                self.metrics.write_errors.fetch_add(1, Ordering::Relaxed);
            }
        }

        result
    }

    /// Flush any pending writes
    pub async fn flush(&mut self) -> AofResult<()> {
        let start_time = Instant::now();
        self.metrics.flush_count.fetch_add(1, Ordering::Relaxed);

        let result = self
            .file
            .flush()
            .await
            .map_err(|e| AofError::FileSystem(format!("Failed to flush {}: {}", self.path, e)));

        self.metrics
            .record_latency(&self.metrics.flush_latency, start_time);
        if result.is_err() {
            self.metrics.sync_errors.fetch_add(1, Ordering::Relaxed);
        }

        result
    }

    /// Get the current size of the file
    pub async fn size(&self) -> AofResult<u64> {
        let metadata = self.file.metadata().await.map_err(|e| {
            AofError::FileSystem(format!("Failed to get metadata for {}: {}", self.path, e))
        })?;
        Ok(metadata.len())
    }

    /// Set the file size (truncate or extend)
    pub async fn set_size(&self, size: u64) -> AofResult<()> {
        self.file.set_len(size).await.map_err(|e| {
            AofError::FileSystem(format!("Failed to set size for {}: {}", self.path, e))
        })
    }

    /// Sync the file to durable storage
    pub async fn sync(&self) -> AofResult<()> {
        let start_time = Instant::now();
        self.metrics.sync_count.fetch_add(1, Ordering::Relaxed);

        let result = self
            .file
            .sync_all()
            .await
            .map_err(|e| AofError::FileSystem(format!("Failed to sync {}: {}", self.path, e)));

        self.metrics
            .record_latency(&self.metrics.sync_latency, start_time);
        if result.is_err() {
            self.metrics.sync_errors.fetch_add(1, Ordering::Relaxed);
        }

        result
    }

    /// Read the entire file content
    pub async fn read_all(&mut self) -> AofResult<Vec<u8>> {
        self.file
            .seek(SeekFrom::Start(0))
            .await
            .map_err(|e| AofError::FileSystem(format!("Failed to seek to start: {}", e)))?;

        let mut content = Vec::new();
        self.file
            .read_to_end(&mut content)
            .await
            .map_err(|e| AofError::FileSystem(format!("Failed to read {}: {}", self.path, e)))?;

        Ok(content)
    }

    /// Write entire content to file
    pub async fn write_all(&mut self, data: &[u8]) -> AofResult<()> {
        self.file
            .seek(SeekFrom::Start(0))
            .await
            .map_err(|e| AofError::FileSystem(format!("Failed to seek to start: {}", e)))?;

        self.file.write_all(data).await.map_err(|e| {
            AofError::FileSystem(format!("Failed to write to {}: {}", self.path, e))
        })?;

        self.file.set_len(data.len() as u64).await.map_err(|e| {
            AofError::FileSystem(format!("Failed to truncate {}: {}", self.path, e))
        })?;

        Ok(())
    }

    /// Memory map the file for reading (async operation on background thread pool)
    pub async fn memory_map(&self) -> AofResult<Option<Mmap>> {
        let start_time = Instant::now();
        self.metrics
            .memory_map_count
            .fetch_add(1, Ordering::Relaxed);

        let path = self.path.clone();

        // Run memory mapping on background thread pool
        let result = tokio::task::spawn_blocking(move || -> AofResult<Option<Mmap>> {
            use std::fs::File;

            let file = File::open(&path).map_err(|e| {
                AofError::FileSystem(format!("Failed to open file for mmap: {}", e))
            })?;

            let mmap = unsafe {
                Mmap::map(&file)
                    .map_err(|e| AofError::FileSystem(format!("Failed to create mmap: {}", e)))?
            };

            Ok(Some(mmap))
        })
        .await
        .map_err(|e| AofError::FileSystem(format!("Background mmap task failed: {}", e)))?;

        self.metrics
            .record_latency(&self.metrics.memory_map_latency, start_time);
        if result.is_err() {
            self.metrics
                .memory_map_errors
                .fetch_add(1, Ordering::Relaxed);
        }

        result
    }

    /// Memory map the file for writing (async operation on background thread pool)
    pub async fn memory_map_mut(&self) -> AofResult<Option<MmapMut>> {
        let start_time = Instant::now();
        self.metrics
            .memory_map_mut_count
            .fetch_add(1, Ordering::Relaxed);

        let path = self.path.clone();

        // Run memory mapping on background thread pool
        let result = tokio::task::spawn_blocking(move || -> AofResult<Option<MmapMut>> {
            use std::fs::OpenOptions;

            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .map_err(|e| {
                    AofError::FileSystem(format!("Failed to open file for mmap_mut: {}", e))
                })?;

            let mmap = unsafe {
                MmapMut::map_mut(&file).map_err(|e| {
                    AofError::FileSystem(format!("Failed to create mmap_mut: {}", e))
                })?
            };

            Ok(Some(mmap))
        })
        .await
        .map_err(|e| AofError::FileSystem(format!("Background mmap_mut task failed: {}", e)))?;

        self.metrics
            .record_latency(&self.metrics.memory_map_mut_latency, start_time);
        if result.is_err() {
            self.metrics
                .memory_map_errors
                .fetch_add(1, Ordering::Relaxed);
        }

        result
    }
}

impl FileHandle for AsyncFileHandle {
    async fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> AofResult<usize> {
        self.read_at(offset, buf).await
    }

    async fn write_at(&mut self, offset: u64, data: &[u8]) -> AofResult<usize> {
        self.write_at(offset, data).await
    }

    async fn set_size(&self, size: u64) -> AofResult<()> {
        self.set_size(size).await
    }

    async fn sync(&self) -> AofResult<()> {
        self.sync().await
    }

    async fn flush(&mut self) -> AofResult<()> {
        self.flush().await
    }

    async fn size(&self) -> AofResult<u64> {
        self.size().await
    }

    async fn read_all(&mut self) -> AofResult<Vec<u8>> {
        self.read_all().await
    }

    async fn write_all(&mut self, data: &[u8]) -> AofResult<()> {
        self.write_all(data).await
    }

    async fn memory_map(&self) -> AofResult<Option<Mmap>> {
        self.memory_map().await
    }

    async fn memory_map_mut(&self) -> AofResult<Option<MmapMut>> {
        self.memory_map_mut().await
    }
}

/// Async filesystem interface
#[derive(Clone)]
pub struct AsyncFileSystem {
    base_path: std::path::PathBuf,
    metrics: Arc<FileSystemMetrics>,
}

impl AsyncFileSystem {
    /// Create a new async filesystem
    pub fn new<P: AsRef<Path>>(base_path: P) -> AofResult<Self> {
        let base_path = base_path.as_ref().to_path_buf();

        if !base_path.exists() {
            std::fs::create_dir_all(&base_path).map_err(|e| {
                AofError::FileSystem(format!(
                    "Failed to create directory {}: {}",
                    base_path.display(),
                    e
                ))
            })?;
        }

        Ok(Self {
            base_path,
            metrics: Arc::new(FileSystemMetrics::new()),
        })
    }

    /// Create a new async filesystem with custom metrics
    pub fn new_with_metrics<P: AsRef<Path>>(
        base_path: P,
        metrics: Arc<FileSystemMetrics>,
    ) -> AofResult<Self> {
        let base_path = base_path.as_ref().to_path_buf();

        if !base_path.exists() {
            std::fs::create_dir_all(&base_path).map_err(|e| {
                AofError::FileSystem(format!(
                    "Failed to create directory {}: {}",
                    base_path.display(),
                    e
                ))
            })?;
        }

        Ok(Self { base_path, metrics })
    }

    /// Get filesystem metrics
    pub fn metrics(&self) -> Arc<FileSystemMetrics> {
        Arc::clone(&self.metrics)
    }

    /// Get the full path for a relative path
    fn full_path(&self, path: &str) -> std::path::PathBuf {
        self.base_path.join(path)
    }

    /// Create a new file
    pub async fn create_file(&self, path: &str) -> AofResult<AsyncFileHandle> {
        let start_time = Instant::now();
        self.metrics
            .create_file_count
            .fetch_add(1, Ordering::Relaxed);

        let result = async {
            let full_path = self.full_path(path);

            // Ensure parent directory exists
            if let Some(parent) = full_path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    AofError::FileSystem(format!(
                        "Failed to create parent directory for {}: {}",
                        path, e
                    ))
                })?;
            }

            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&full_path)
                .await
                .map_err(|e| {
                    AofError::FileSystem(format!("Failed to create file {}: {}", path, e))
                })?;

            Ok(AsyncFileHandle::new(
                file,
                full_path.to_string_lossy().to_string(),
                Arc::clone(&self.metrics),
            ))
        }
        .await;

        self.metrics
            .record_latency(&self.metrics.create_file_latency, start_time);
        if result.is_err() {
            self.metrics
                .create_file_errors
                .fetch_add(1, Ordering::Relaxed);
        }

        result
    }

    /// Open an existing file for reading
    pub async fn open_file(&self, path: &str) -> AofResult<AsyncFileHandle> {
        let full_path = self.full_path(path);

        let file = File::open(&full_path)
            .await
            .map_err(|e| AofError::FileSystem(format!("Failed to open file {}: {}", path, e)))?;

        Ok(AsyncFileHandle::new(
            file,
            full_path.to_string_lossy().to_string(),
            Arc::clone(&self.metrics),
        ))
    }

    /// Open an existing file for reading and writing
    pub async fn open_file_mut(&self, path: &str) -> AofResult<AsyncFileHandle> {
        let full_path = self.full_path(path);

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&full_path)
            .await
            .map_err(|e| {
                AofError::FileSystem(format!("Failed to open file for writing {}: {}", path, e))
            })?;

        Ok(AsyncFileHandle::new(
            file,
            full_path.to_string_lossy().to_string(),
            Arc::clone(&self.metrics),
        ))
    }

    /// Delete a file
    pub async fn delete_file(&self, path: &str) -> AofResult<()> {
        let full_path = self.full_path(path);

        tokio::fs::remove_file(&full_path)
            .await
            .map_err(|e| AofError::FileSystem(format!("Failed to delete file {}: {}", path, e)))?;

        Ok(())
    }

    /// Check if a file exists
    pub async fn file_exists(&self, path: &str) -> AofResult<bool> {
        let full_path = self.full_path(path);
        Ok(full_path.exists())
    }

    /// List files matching a pattern (basic glob support)
    pub async fn list_files(&self, pattern: &str) -> AofResult<Vec<String>> {
        let full_pattern = self.full_path(pattern);
        let parent = full_pattern
            .parent()
            .ok_or_else(|| AofError::FileSystem("Invalid pattern".to_string()))?;

        let mut entries = tokio::fs::read_dir(parent)
            .await
            .map_err(|e| AofError::FileSystem(format!("Failed to read directory: {}", e)))?;

        let mut files = Vec::new();

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AofError::FileSystem(format!("Failed to read directory entry: {}", e)))?
        {
            if entry
                .file_type()
                .await
                .map_err(|e| AofError::FileSystem(format!("Failed to get file type: {}", e)))?
                .is_file()
            {
                if let Some(file_name) = entry.file_name().to_str() {
                    files.push(file_name.to_string());
                }
            }
        }

        Ok(files)
    }

    /// Get file metadata
    pub async fn file_metadata(&self, path: &str) -> AofResult<FileMetadata> {
        let full_path = self.full_path(path);

        let metadata = tokio::fs::metadata(&full_path).await.map_err(|e| {
            AofError::FileSystem(format!("Failed to get metadata for {}: {}", path, e))
        })?;

        Ok(FileMetadata {
            size: metadata.len(),
            created: metadata.created().unwrap_or_else(|_| SystemTime::now()),
            modified: metadata.modified().unwrap_or_else(|_| SystemTime::now()),
        })
    }

    /// Create a directory
    pub async fn create_directory(&self, path: &str) -> AofResult<()> {
        let full_path = self.full_path(path);

        tokio::fs::create_dir_all(&full_path).await.map_err(|e| {
            AofError::FileSystem(format!("Failed to create directory {}: {}", path, e))
        })?;

        Ok(())
    }

    /// Copy a file
    pub async fn copy_file(&self, from: &str, to: &str) -> AofResult<()> {
        let from_path = self.full_path(from);
        let to_path = self.full_path(to);

        // Ensure destination directory exists
        if let Some(parent) = to_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AofError::FileSystem(format!("Failed to create parent directory: {}", e))
            })?;
        }

        tokio::fs::copy(&from_path, &to_path).await.map_err(|e| {
            AofError::FileSystem(format!("Failed to copy {} to {}: {}", from, to, e))
        })?;

        Ok(())
    }

    /// Move/rename a file
    pub async fn move_file(&self, from: &str, to: &str) -> AofResult<()> {
        let from_path = self.full_path(from);
        let to_path = self.full_path(to);

        // Ensure destination directory exists
        if let Some(parent) = to_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AofError::FileSystem(format!("Failed to create parent directory: {}", e))
            })?;
        }

        tokio::fs::rename(&from_path, &to_path).await.map_err(|e| {
            AofError::FileSystem(format!("Failed to move {} to {}: {}", from, to, e))
        })?;

        Ok(())
    }
}

impl FileSystem for AsyncFileSystem {
    type Handle = AsyncFileHandle;

    async fn create_file(&self, path: &str) -> AofResult<Self::Handle> {
        self.create_file(path).await
    }

    async fn open_file(&self, path: &str) -> AofResult<Self::Handle> {
        self.open_file(path).await
    }

    async fn open_file_mut(&self, path: &str) -> AofResult<Self::Handle> {
        self.open_file_mut(path).await
    }

    async fn delete_file(&self, path: &str) -> AofResult<()> {
        self.delete_file(path).await
    }

    async fn file_exists(&self, path: &str) -> AofResult<bool> {
        self.file_exists(path).await
    }

    async fn file_metadata(&self, path: &str) -> AofResult<FileMetadata> {
        self.file_metadata(path).await
    }

    async fn list_files(&self, pattern: &str) -> AofResult<Vec<String>> {
        self.list_files(pattern).await
    }
}

// Type alias for compatibility
pub type LocalFileSystem = AsyncFileSystem;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_async_filesystem_operations() {
        let temp_dir = tempdir().unwrap();
        let fs = AsyncFileSystem::new(temp_dir.path()).unwrap();

        // Test file creation and writing
        let mut file = fs.create_file("test.txt").await.unwrap();
        let data = b"Hello, world!";
        file.write_all(data).await.unwrap();
        file.sync().await.unwrap();

        // Test file reading
        let mut file = fs.open_file("test.txt").await.unwrap();
        let content = file.read_all().await.unwrap();
        assert_eq!(content, data);

        // Test file metadata
        let metadata = fs.file_metadata("test.txt").await.unwrap();
        assert_eq!(metadata.size, data.len() as u64);

        // Test file existence
        assert!(fs.file_exists("test.txt").await.unwrap());
        assert!(!fs.file_exists("nonexistent.txt").await.unwrap());

        // Test file deletion
        fs.delete_file("test.txt").await.unwrap();
        assert!(!fs.file_exists("test.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_async_file_operations() {
        let temp_dir = tempdir().unwrap();
        let fs = AsyncFileSystem::new(temp_dir.path()).unwrap();

        // Test write operations
        {
            let mut file = fs.create_file("test_offsets.txt").await.unwrap();

            // Set file size to ensure sparse file handling works correctly
            file.set_size(20).await.unwrap();

            // Test writing at different offsets
            file.write_at(0, b"Hello").await.unwrap();
            file.write_at(10, b"World").await.unwrap();

            // Ensure data is flushed and synced to disk before closing
            file.flush().await.unwrap();
            file.sync().await.unwrap();
        } // Close file by dropping

        // Test read operations with a separate file handle
        {
            let mut file = fs.open_file_mut("test_offsets.txt").await.unwrap();

            // Test reading at different offsets
            let mut buffer = vec![0u8; 5];
            let n = file.read_at(0, &mut buffer).await.unwrap();
            assert_eq!(n, 5);
            assert_eq!(&buffer, b"Hello");

            let n = file.read_at(10, &mut buffer).await.unwrap();
            assert_eq!(n, 5);
            assert_eq!(&buffer, b"World");
        }
    }
}
