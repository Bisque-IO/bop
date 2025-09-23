use std::sync::Arc;
use std::time::Duration;

use bop_raft::{MetricCatalog, RaftResult, RaftServer};

#[tokio::main]
async fn main() -> RaftResult<()> {
    // Replace this with the handle you maintain inside your application.
    let server = obtain_server_handle();

    // Spawn a background task that publishes metrics every five seconds.
    let publisher = tokio::spawn(async move {
        if let Err(err) = metrics_loop(server).await {
            eprintln!("metrics loop exited: {err}");
        }
    });

    publisher.await.expect("metrics task panicked");
    Ok(())
}

fn obtain_server_handle() -> Arc<RaftServer> {
    unimplemented!(
        "Pass in the Arc<RaftServer> your application already owns.          See crate documentation for bootstrapping examples."
    );
}

async fn metrics_loop(server: Arc<RaftServer>) -> RaftResult<()> {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    let catalog = MetricCatalog::default();

    loop {
        interval.tick().await;

        match server.collect_server_metrics_with_catalog(&catalog) {
            Ok(summary) => {
                #[cfg(feature = "tracing")]
                tracing::debug!(
                    target = "bop_raft::example",
                    uptime_us = summary.leader_uptime.as_micros(),
                    commit_throughput = summary.commit_throughput,
                    "collected server metrics"
                );
            }
            Err(err) => {
                eprintln!("failed to gather server metrics: {err}");
            }
        }

        match server.metrics_snapshot() {
            Ok(snapshot) => {
                #[cfg(feature = "serde")]
                println!("{}", snapshot.to_json().unwrap_or_else(|_| "{}".to_string()));

                #[cfg(not(feature = "serde"))]
                println!(
                    "counters: {} gauges: {} histograms: {}",
                    snapshot.counters.len(),
                    snapshot.gauges.len(),
                    snapshot.histograms.len()
                );
            }
            Err(err) => eprintln!("failed to capture metrics snapshot: {err}"),
        }
    }
}
