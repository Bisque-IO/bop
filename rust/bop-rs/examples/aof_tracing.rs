#[cfg(feature = "tiered-store")]
fn main() {
    eprintln!("aof_tracing example is disabled when the `tiered-store` feature is enabled");
}

#[cfg(not(feature = "tiered-store"))]
use std::sync::Arc;
#[cfg(not(feature = "tiered-store"))]
use std::time::Duration;

#[cfg(not(feature = "tiered-store"))]
use bop_rs::aof2::{
    Aof, AofManager, AofManagerConfig, FlushMetricSample, FlushMetricsExporter, TailState,
    config::AofConfig,
};
#[cfg(not(feature = "tiered-store"))]
use tempfile::TempDir;
#[cfg(not(feature = "tiered-store"))]
use tokio::time::sleep;
#[cfg(not(feature = "tiered-store"))]
use tracing::{info, warn};
#[cfg(not(feature = "tiered-store"))]
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(not(feature = "tiered-store"))]
fn main() -> anyhow::Result<()> {
    // Install a subscriber that keeps the default formatter but raises AOF2 to debug level.
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_thread_ids(true)
        .compact();
    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,bop_rs::aof2=debug")),
        )
        .init();

    let manager = AofManager::with_config(AofManagerConfig::default())?;
    let handle = manager.handle();
    let _manager = manager;
    let mut cfg = AofConfig::default();
    cfg.root_dir = TempDir::new()?.into_path();

    let aof = Arc::new(Aof::new(handle.clone(), cfg)?);
    let mut follower = aof.tail_follower();

    let tail = Arc::clone(&aof);
    handle.runtime_handle().spawn(async move {
        loop {
            match tail.next_sealed_segment(&mut follower).await {
                Some(segment) => {
                    info!(
                        segment_id = segment.id().as_u64(),
                        durable_bytes = segment.durable_size(),
                        record_count = segment.record_count(),
                        "sealed segment ready for archival"
                    );
                }
                None => {
                    warn!("tail follower closed; exiting");
                    break;
                }
            }
        }
    });

    // Append a handful of records to drive the tracing hooks.
    for idx in 0..3 {
        let payload = format!("tracing-payload-{idx}");
        let record = aof.append_record(payload.as_bytes())?;
        info!(?record, "appended record");
    }

    aof.seal_active()?;
    handle
        .runtime_handle()
        .block_on(sleep(Duration::from_millis(100)));

    // Snapshot flush metrics and emit them as structured log records.
    FlushMetricsExporter::new(aof.flush_metrics()).emit(|sample: FlushMetricSample| {
        info!(
            metric = sample.name,
            value = sample.value,
            "flush metric sample"
        );
    });

    // Inspect the tail state once to show what the watch channel publishes.
    let TailState { tail, pending, .. } = aof.tail_events().borrow().clone();
    info!(?tail, ?pending, "tail state snapshot");

    Ok(())
}
