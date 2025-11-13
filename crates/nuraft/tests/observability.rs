use std::time::SystemTime;

use bop_raft::metrics::{HistogramBucket, HistogramBuckets, MetricPoint};
use bop_raft::{MetricsSnapshot, PrometheusExporter};

#[test]
fn prometheus_exporter_formats_snapshot() {
    let snapshot = MetricsSnapshot {
        timestamp: SystemTime::UNIX_EPOCH,
        counters: vec![MetricPoint {
            name: "raft_commits_total".to_string(),
            value: 64,
        }],
        gauges: vec![MetricPoint {
            name: "leader_uptime_us".to_string(),
            value: 1024,
        }],
        histograms: vec![HistogramBuckets {
            name: "append_entries_latency_us".to_string(),
            buckets: vec![
                HistogramBucket {
                    upper_bound: 5.0,
                    count: 4,
                },
                HistogramBucket {
                    upper_bound: 10.0,
                    count: 6,
                },
            ],
        }],
    };

    let exporter = PrometheusExporter::new();
    let text = exporter.export(&snapshot).expect("prometheus export");

    assert!(text.contains("# TYPE raft_commits_total counter"));
    assert!(text.contains("leader_uptime_us 1024"));
    assert!(text.contains("append_entries_latency_us_bucket"));
    assert!(text.contains("append_entries_latency_us_count"));
}

#[cfg(feature = "serde")]
#[test]
fn json_exporter_respects_pretty_flag() {
    use bop_raft::JsonMetricsExporter;

    let snapshot = MetricsSnapshot {
        timestamp: SystemTime::UNIX_EPOCH,
        counters: vec![MetricPoint {
            name: "raft_commits_total".to_string(),
            value: 1,
        }],
        gauges: Vec::new(),
        histograms: Vec::new(),
    };

    let exporter = JsonMetricsExporter::new();
    let compact = exporter.export(&snapshot).expect("json export");
    assert!(compact.contains("raft_commits_total"));

    let pretty = exporter
        .with_pretty(true)
        .export(&snapshot)
        .expect("pretty export");
    assert!(pretty.contains('\n'));
}

#[cfg(feature = "serde")]
#[test]
fn metric_catalog_serializes() {
    use bop_raft::metric_descriptor_catalog_json;

    let json = metric_descriptor_catalog_json(false).expect("catalog json");
    assert!(json.contains("leader_uptime_us"));
}
