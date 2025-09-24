#![cfg(debug_assertions)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use bop_aof::config::SegmentId;
use bop_aof::error::AofError;
use bop_aof::store::Tier2Config;
use bop_aof::store::Tier2Manager;
use bop_aof::store::tier2::{
    GetObjectRequest, GetObjectResult, HeadObjectResult, PutObjectRequest, PutObjectResult,
    RetryPolicy, Tier2Client, Tier2DeleteRequest, Tier2UploadDescriptor,
};
use bop_aof::test_support::{
    MetadataPersistContext, clear_metadata_persist_hook, install_metadata_persist_hook,
    make_instance_id,
};
use bop_aof::{Aof, AofConfig, AofManager, AofManagerConfig, FlushConfig};
use futures::future::BoxFuture;
use tempfile::TempDir;
use tokio::runtime::Runtime;
use tokio::time::sleep;

fn manager_config_for_tests() -> AofManagerConfig {
    let mut cfg = AofManagerConfig::default();
    cfg.runtime_worker_threads = Some(2);
    cfg.flush.flush_watermark_bytes = 1024;
    cfg.flush.flush_interval_ms = 0;
    cfg.flush.max_unflushed_bytes = 8 * 1024;
    cfg
}

#[test]
fn metadata_retry_increments_metrics() {
    let attempts = Arc::new(AtomicU32::new(0));
    let hook_attempts = attempts.clone();
    let _guard = install_metadata_persist_hook(move |_ctx: &MetadataPersistContext| {
        let attempt = hook_attempts.fetch_add(1, AtomicOrdering::SeqCst);
        if attempt < 2 {
            Some(Err(AofError::FileSystem(format!(
                "injected metadata failure attempt {}",
                attempt + 1
            ))))
        } else {
            None
        }
    });

    let temp = TempDir::new().expect("tempdir");
    let manager = AofManager::with_config(manager_config_for_tests()).expect("manager");
    let handle = manager.handle();

    let mut cfg = AofConfig::default();
    cfg.root_dir = temp.path().join("aof");
    cfg.segment_min_bytes = 2048;
    cfg.segment_max_bytes = 2048;
    cfg.segment_target_bytes = 2048;
    cfg.flush = FlushConfig {
        flush_watermark_bytes: u64::MAX,
        flush_interval_ms: u64::MAX,
        max_unflushed_bytes: u64::MAX,
        max_concurrent_flushes: 1,
    };

    let aof = Aof::new(handle.clone(), cfg).expect("aof");

    let payload = vec![7u8; 256];
    let record = aof.append_record(&payload).expect("append");
    aof.flush_until(record).expect("flush_until");

    let metrics = aof.flush_metrics();
    assert!(metrics.metadata_retry_attempts >= 2);
    assert_eq!(metrics.metadata_retry_failures, 0);
    assert_eq!(metrics.metadata_failures, 0);
    assert!(attempts.load(AtomicOrdering::SeqCst) >= 3);

    clear_metadata_persist_hook();
    drop(aof);
    drop(handle);
    drop(manager);
}

#[test]
fn tier2_upload_delete_retry_metrics() {
    let upload_failures = 2;
    let delete_failures = 1;
    let rt = Runtime::new().expect("runtime");

    rt.block_on(async {
        let client = Arc::new(ScriptedTier2Client::new(
            upload_failures,
            delete_failures,
            0,
        ));

        let config = Tier2Config::new("http://127.0.0.1:9000", "us-east-1", "bucket", "ak", "sk")
            .with_retry(RetryPolicy::new(5, Duration::from_millis(10)))
            .with_concurrency(1)
            .with_prefix("tests");

        let manager =
            Tier2Manager::with_client(rt.handle().clone(), config.clone(), client.clone())
                .expect("tier2 manager");
        let handle = manager.handle();

        let temp = TempDir::new().expect("tempdir");
        let warm_path = temp.path().join("segment.zst");
        std::fs::write(&warm_path, b"payload").expect("write warm file");

        let instance_id = make_instance_id(7);
        let segment_id = SegmentId::new(42);
        let descriptor = Tier2UploadDescriptor {
            instance_id,
            segment_id,
            warm_path: warm_path.clone(),
            sealed_at: 0,
            base_offset: 0,
            base_record_count: 0,
            checksum: 0,
            compressed_bytes: 0,
            original_bytes: 8,
        };

        handle.schedule_upload(descriptor).expect("schedule upload");

        wait_for(|| client.upload_attempts() >= upload_failures + 1).await;

        let object_key = config.object_key(instance_id, segment_id, 0);
        handle
            .schedule_delete(Tier2DeleteRequest {
                instance_id,
                segment_id,
                object_key,
            })
            .expect("schedule delete");

        wait_for(|| client.delete_attempts() >= delete_failures + 1).await;

        let metrics = manager.metrics();
        assert_eq!(metrics.upload_retry_attempts, upload_failures as u64);
        assert_eq!(metrics.delete_retry_attempts, delete_failures as u64);
    });
}

#[test]
fn tier2_fetch_retry_metrics() {
    let fetch_failures = 1;
    let rt = Runtime::new().expect("runtime");

    rt.block_on(async {
        let client = Arc::new(ScriptedTier2Client::new(0, 0, fetch_failures));

        let config = Tier2Config::new("http://127.0.0.1:9000", "us-east-1", "bucket", "ak", "sk")
            .with_retry(RetryPolicy::new(4, Duration::from_millis(10)))
            .with_concurrency(1)
            .with_prefix("tests");

        let manager =
            Tier2Manager::with_client(rt.handle().clone(), config.clone(), client.clone())
                .expect("tier2 manager");
        let handle = manager.handle();

        let temp = TempDir::new().expect("tempdir");
        let warm_path = temp.path().join("segment.zst");
        std::fs::write(&warm_path, b"payload").expect("write warm file");

        let instance_id = make_instance_id(11);
        let segment_id = SegmentId::new(99);
        let descriptor = Tier2UploadDescriptor {
            instance_id,
            segment_id,
            warm_path: warm_path.clone(),
            sealed_at: 0,
            base_offset: 0,
            base_record_count: 0,
            checksum: 0,
            compressed_bytes: 0,
            original_bytes: 8,
        };

        handle.schedule_upload(descriptor).expect("schedule upload");
        wait_for(|| client.upload_attempts() >= 1).await;

        let fetch_dir = TempDir::new().expect("fetch dir");
        handle
            .schedule_fetch(bop_aof::store::tier2::Tier2FetchRequest {
                instance_id,
                segment_id,
                sealed_at: 0,
                destination: fetch_dir.path().join("segment.bin"),
            })
            .expect("schedule fetch");

        wait_for(|| client.fetch_attempts() >= fetch_failures + 1).await;

        assert_eq!(client.fetch_attempts(), fetch_failures + 1);
        assert!(fetch_dir.path().join("segment.bin").exists());
    });
}

async fn wait_for(mut predicate: impl FnMut() -> bool) {
    for _ in 0..200 {
        if predicate() {
            return;
        }
        sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out waiting for condition");
}

#[derive(Clone)]
struct ScriptedTier2Client {
    upload_failures_before_success: u32,
    delete_failures_before_success: u32,
    fetch_failures_before_success: u32,
    upload_attempts: Arc<AtomicU32>,
    delete_attempts: Arc<AtomicU32>,
    fetch_attempts: Arc<AtomicU32>,
    objects: Arc<StdMutex<Option<PathBuf>>>,
}

impl ScriptedTier2Client {
    fn new(upload_failures: u32, delete_failures: u32, fetch_failures: u32) -> Self {
        Self {
            upload_failures_before_success: upload_failures,
            delete_failures_before_success: delete_failures,
            fetch_failures_before_success: fetch_failures,
            upload_attempts: Arc::new(AtomicU32::new(0)),
            delete_attempts: Arc::new(AtomicU32::new(0)),
            fetch_attempts: Arc::new(AtomicU32::new(0)),
            objects: Arc::new(StdMutex::new(None)),
        }
    }

    fn upload_attempts(&self) -> u32 {
        self.upload_attempts.load(AtomicOrdering::SeqCst)
    }

    fn delete_attempts(&self) -> u32 {
        self.delete_attempts.load(AtomicOrdering::SeqCst)
    }

    fn fetch_attempts(&self) -> u32 {
        self.fetch_attempts.load(AtomicOrdering::SeqCst)
    }
}

impl Tier2Client for ScriptedTier2Client {
    fn put_object(
        &self,
        request: PutObjectRequest,
    ) -> BoxFuture<'_, bop_aof::error::AofResult<PutObjectResult>> {
        let attempts = self.upload_attempts.clone();
        let failures = self.upload_failures_before_success;
        let objects = self.objects.clone();
        Box::pin(async move {
            let attempt = attempts.fetch_add(1, AtomicOrdering::SeqCst) + 1;
            if attempt <= failures {
                return Err(AofError::RemoteStorage(format!(
                    "forced upload failure on attempt {attempt}"
                )));
            }
            let data = std::fs::read(&request.source).map_err(AofError::from)?;
            *objects.lock().unwrap() = Some(request.source.clone());
            Ok(PutObjectResult {
                etag: Some(format!("etag-{attempt}")),
                size: data.len() as u64,
            })
        })
    }

    fn get_object(
        &self,
        request: GetObjectRequest,
    ) -> BoxFuture<'_, bop_aof::error::AofResult<GetObjectResult>> {
        let attempts = self.fetch_attempts.clone();
        let failures = self.fetch_failures_before_success;
        let objects = self.objects.clone();
        Box::pin(async move {
            let attempt = attempts.fetch_add(1, AtomicOrdering::SeqCst) + 1;
            if attempt <= failures {
                return Err(AofError::RemoteStorage(format!(
                    "forced fetch failure on attempt {attempt}"
                )));
            }
            let source = {
                objects
                    .lock()
                    .unwrap()
                    .clone()
                    .ok_or_else(|| AofError::RemoteStorage("object missing".to_string()))?
            };
            if let Some(parent) = request.destination.parent() {
                std::fs::create_dir_all(parent).map_err(AofError::from)?;
            }
            std::fs::copy(&source, &request.destination).map_err(AofError::from)?;
            let size = std::fs::metadata(&request.destination)
                .map_err(AofError::from)?
                .len();
            Ok(GetObjectResult {
                etag: Some("etag-fetch".to_string()),
                size,
            })
        })
    }

    fn delete_object(
        &self,
        _bucket: &str,
        _key: &str,
    ) -> BoxFuture<'_, bop_aof::error::AofResult<()>> {
        let attempts = self.delete_attempts.clone();
        let failures = self.delete_failures_before_success;
        let objects = self.objects.clone();
        Box::pin(async move {
            let attempt = attempts.fetch_add(1, AtomicOrdering::SeqCst) + 1;
            if attempt <= failures {
                return Err(AofError::RemoteStorage(format!(
                    "forced delete failure on attempt {attempt}"
                )));
            }
            *objects.lock().unwrap() = None;
            Ok(())
        })
    }

    fn head_object(
        &self,
        _bucket: &str,
        _key: &str,
    ) -> BoxFuture<'_, bop_aof::error::AofResult<Option<HeadObjectResult>>> {
        Box::pin(async { Ok(None) })
    }
}
