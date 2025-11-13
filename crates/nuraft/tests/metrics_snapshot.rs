use std::time::{Duration, SystemTime};

#[cfg(feature = "serde")]
use bop_raft::metrics::{HistogramBucket, HistogramBuckets, MetricPoint};
#[cfg(feature = "serde")]
use bop_raft::{MetricsSnapshot, ServerMetrics};

#[cfg(feature = "serde")]
#[test]
fn snapshot_serializes_to_json() {
    let snapshot = MetricsSnapshot {
        timestamp: SystemTime::UNIX_EPOCH,
        counters: vec![MetricPoint {
            name: "raft_commits_total".to_string(),
            value: 42,
        }],
        gauges: vec![MetricPoint {
            name: "leader_uptime_us".to_string(),
            value: 10_000,
        }],
        histograms: vec![HistogramBuckets {
            name: "append_latency".to_string(),
            buckets: vec![
                HistogramBucket {
                    upper_bound: 5.0,
                    count: 2,
                },
                HistogramBucket {
                    upper_bound: 10.0,
                    count: 3,
                },
            ],
        }],
    };

    let json = snapshot.to_json().expect("json");
    assert!(json.contains("raft_commits_total"));
}

#[cfg(feature = "serde")]
#[test]
fn server_metrics_serializes() {
    let metrics = ServerMetrics {
        leader_uptime: Duration::from_secs(2),
        commit_throughput: 128,
        append_entries_latency: None,
    };

    let value = serde_json::to_value(metrics).expect("serialize server metrics");
    assert_eq!(value["leader_uptime_us"], serde_json::json!(2_000_000));
}
