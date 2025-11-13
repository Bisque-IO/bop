use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Write};
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

use crate::metrics::{HistogramBuckets, MetricsSnapshot};

#[cfg(feature = "serde")]
use serde::Serialize;
#[cfg(feature = "serde")]
use serde_json::ser::{CompactFormatter, PrettyFormatter, Serializer};
#[cfg(feature = "serde")]
use std::io;
#[cfg(feature = "serde")]
use std::str;
use thiserror::Error;

#[cfg(feature = "tracing")]
use tracing::{debug, info, warn};

#[cfg(feature = "tracing")]
use crate::error::RaftResult;
#[cfg(feature = "tracing")]
use crate::traits::ServerCallbacks;
#[cfg(feature = "tracing")]
use crate::types::{CallbackAction, CallbackContext, CallbackType};

/// Result alias for observability helpers.
pub type ObservabilityResult<T> = Result<T, ObservabilityError>;

/// Errors surfaced by observability utilities.
#[derive(Debug, Error)]
pub enum ObservabilityError {
    #[cfg(feature = "serde")]
    #[error("JSON serialization failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("metrics formatting failed")]
    Format(#[from] fmt::Error),

    #[error("system clock is before UNIX_EPOCH")]
    ClockSkew,
}

/// Metric type enumeration for documentation tooling.
#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricKind {
    Counter,
    Gauge,
    Histogram,
}

/// Descriptor that links Rust-facing metric identifiers to NuRaft counters.
#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug, Clone, Copy)]
pub struct MetricDescriptor {
    pub rust_name: &'static str,
    pub nuraft_name: &'static str,
    pub kind: MetricKind,
    pub description: &'static str,
}

const METRIC_DESCRIPTORS: &[MetricDescriptor] = &[
    MetricDescriptor {
        rust_name: "leader_uptime_us",
        nuraft_name: "leader_uptime_us",
        kind: MetricKind::Gauge,
        description: "Time spent as cluster leader (microseconds).",
    },
    MetricDescriptor {
        rust_name: "commit_throughput",
        nuraft_name: "commit_throughput",
        kind: MetricKind::Counter,
        description: "Number of log entries committed since the last sample.",
    },
    MetricDescriptor {
        rust_name: "append_entries_latency_us",
        nuraft_name: "append_entries_latency_us",
        kind: MetricKind::Histogram,
        description: "AppendEntries round-trip latency (microseconds).",
    },
];

/// Expose the static metric descriptor catalog for documentation tooling.
pub fn metric_descriptors() -> &'static [MetricDescriptor] {
    METRIC_DESCRIPTORS
}

/// Lookup the descriptor for a Rust metric identifier.
pub fn metric_descriptor_by_rust_name(name: &str) -> Option<&'static MetricDescriptor> {
    static MAP: OnceLock<HashMap<&'static str, &'static MetricDescriptor>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = HashMap::with_capacity(METRIC_DESCRIPTORS.len());
        for descriptor in METRIC_DESCRIPTORS {
            map.insert(descriptor.rust_name, descriptor);
        }
        map
    })
    .get(name)
    .copied()
}

#[cfg(feature = "serde")]
/// Serialize the metric descriptor map to JSON for drift detection.
pub fn metric_descriptor_catalog_json(pretty: bool) -> ObservabilityResult<String> {
    if pretty {
        Ok(serde_json::to_string_pretty(METRIC_DESCRIPTORS)?)
    } else {
        Ok(serde_json::to_string(METRIC_DESCRIPTORS)?)
    }
}

#[cfg(feature = "serde")]
struct StringWriter<'a> {
    buffer: &'a mut String,
}

#[cfg(feature = "serde")]
impl<'a> io::Write for StringWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let slice =
            str::from_utf8(buf).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        self.buffer.push_str(slice);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// JSON exporter that reuses an internal buffer when formatting snapshots.
#[cfg(feature = "serde")]
#[derive(Default)]
pub struct JsonMetricsExporter {
    pretty: bool,
}

#[cfg(feature = "serde")]
impl JsonMetricsExporter {
    pub fn new() -> Self {
        Self { pretty: false }
    }

    pub fn with_pretty(mut self, pretty: bool) -> Self {
        self.pretty = pretty;
        self
    }

    pub fn export(&self, snapshot: &MetricsSnapshot) -> ObservabilityResult<String> {
        let mut output = String::new();
        self.export_into(snapshot, &mut output)?;
        Ok(output)
    }

    pub fn export_into(
        &self,
        snapshot: &MetricsSnapshot,
        buffer: &mut String,
    ) -> ObservabilityResult<()> {
        buffer.clear();
        let mut writer = StringWriter { buffer };
        if self.pretty {
            let formatter = PrettyFormatter::with_indent(b"  ");
            let mut serializer = Serializer::with_formatter(&mut writer, formatter);
            snapshot.serialize(&mut serializer)?;
        } else {
            let mut serializer = Serializer::with_formatter(&mut writer, CompactFormatter);
            snapshot.serialize(&mut serializer)?;
        }
        Ok(())
    }
}

/// Prometheus text format exporter.
#[derive(Default)]
pub struct PrometheusExporter {
    include_timestamp: bool,
}

impl PrometheusExporter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_timestamp(mut self, include: bool) -> Self {
        self.include_timestamp = include;
        self
    }

    pub fn export(&self, snapshot: &MetricsSnapshot) -> ObservabilityResult<String> {
        let mut output = String::with_capacity(512);
        self.export_into(snapshot, &mut output)?;
        Ok(output)
    }

    pub fn export_into(
        &self,
        snapshot: &MetricsSnapshot,
        buffer: &mut String,
    ) -> ObservabilityResult<()> {
        buffer.clear();
        let timestamp = if self.include_timestamp {
            Some(
                snapshot
                    .timestamp
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| ObservabilityError::ClockSkew)?
                    .as_millis(),
            )
        } else {
            None
        };

        let mut seen_counters = HashSet::new();
        for counter in &snapshot.counters {
            let name = sanitize_metric_name(&counter.name);
            if seen_counters.insert(name.as_ref().to_string()) {
                writeln!(buffer, "# TYPE {} counter", name)?;
            }
            write!(buffer, "{} {}", name, counter.value)?;
            if let Some(ts) = timestamp {
                write!(buffer, " {}", ts)?;
            }
            buffer.push('\n');
        }

        let mut seen_gauges = HashSet::new();
        for gauge in &snapshot.gauges {
            let name = sanitize_metric_name(&gauge.name);
            if seen_gauges.insert(name.as_ref().to_string()) {
                writeln!(buffer, "# TYPE {} gauge", name)?;
            }
            write!(buffer, "{} {}", name, gauge.value)?;
            if let Some(ts) = timestamp {
                write!(buffer, " {}", ts)?;
            }
            buffer.push('\n');
        }

        let mut seen_histograms = HashSet::new();
        for histogram in &snapshot.histograms {
            let name = sanitize_metric_name(&histogram.name);
            if seen_histograms.insert(name.as_ref().to_string()) {
                writeln!(buffer, "# TYPE {} histogram", name)?;
            }
            format_histogram(name.as_ref(), histogram, timestamp, buffer)?;
        }

        Ok(())
    }
}

fn sanitize_metric_name(name: &str) -> Cow<'_, str> {
    if name
        .chars()
        .all(|ch| matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | ':' ))
    {
        return Cow::Borrowed(name);
    }
    let mut sanitized = String::with_capacity(name.len());
    for ch in name.chars() {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | ':' => sanitized.push(ch),
            _ => sanitized.push('_'),
        }
    }
    Cow::Owned(sanitized)
}

fn format_histogram(
    name: &str,
    histogram: &HistogramBuckets,
    timestamp: Option<u128>,
    buffer: &mut String,
) -> ObservabilityResult<()> {
    for bucket in &histogram.buckets {
        if bucket.upper_bound.is_infinite() {
            write!(buffer, "{}_bucket{{le=\"+Inf\"}} {}", name, bucket.count)?;
        } else {
            write!(
                buffer,
                "{}_bucket{{le=\"{}\"}} {}",
                name, bucket.upper_bound, bucket.count
            )?;
        }
        if let Some(ts) = timestamp {
            write!(buffer, " {}", ts)?;
        }
        buffer.push('\n');
    }

    let total = histogram.total_count();
    write!(buffer, "{}_count {}", name, total)?;
    if let Some(ts) = timestamp {
        write!(buffer, " {}", ts)?;
    }
    buffer.push('\n');

    Ok(())
}

#[cfg(feature = "tracing")]
pub struct TracingObserver {
    target: &'static str,
}

#[cfg(feature = "tracing")]
impl TracingObserver {
    pub fn new() -> Self {
        Self {
            target: "bop_raft::server",
        }
    }

    pub fn with_target(target: &'static str) -> Self {
        Self { target }
    }
}

#[cfg(feature = "tracing")]
impl ServerCallbacks for TracingObserver {
    fn handle_event(&self, context: CallbackContext<'_>) -> RaftResult<CallbackAction> {
        let ty = context.callback_type();
        let params = context.param();
        let my_id = params.my_id().0;
        match ty {
            CallbackType::BecomeLeader => {
                info!(target = self.target, my_id, "became leader");
            }
            CallbackType::BecomeFollower => {
                info!(
                    target = self.target,
                    my_id,
                    leader_id = params.leader_id().0,
                    "became follower"
                );
            }
            CallbackType::ResignationFromLeader => {
                info!(target = self.target, my_id, "resigned leadership");
            }
            CallbackType::JoinedCluster => {
                info!(
                    target = self.target,
                    my_id,
                    peer_id = params.peer_id().0,
                    "peer joined cluster"
                );
            }
            CallbackType::NewConfig => {
                info!(
                    target = self.target,
                    my_id, "new cluster configuration applied"
                );
            }
            CallbackType::RemovedFromCluster => {
                warn!(
                    target = self.target,
                    my_id,
                    peer_id = params.peer_id().0,
                    "peer removed from cluster"
                );
            }
            CallbackType::ServerJoinFailed => {
                warn!(
                    target = self.target,
                    my_id,
                    peer_id = params.peer_id().0,
                    "peer join request failed"
                );
            }
            CallbackType::SnapshotCreationBegin => {
                info!(target = self.target, my_id, "snapshot creation started");
            }
            CallbackType::ReceivedMisbehavingMessage => {
                warn!(target = self.target, my_id, "received misbehaving message");
            }
            CallbackType::SentAppendEntriesReq
            | CallbackType::ReceivedAppendEntriesReq
            | CallbackType::SentAppendEntriesResp
            | CallbackType::ReceivedAppendEntriesResp
            | CallbackType::GotAppendEntryRespFromPeer
            | CallbackType::RequestAppendEntries => {
                debug!(target = self.target, my_id, event = ?ty, "append entries traffic");
            }
            CallbackType::AutoAdjustQuorum => {
                debug!(target = self.target, my_id, "auto quorum adjustment");
            }
            _ => {
                debug!(target = self.target, my_id, event = ?ty, "server callback");
            }
        }
        Ok(CallbackAction::Continue)
    }
}
