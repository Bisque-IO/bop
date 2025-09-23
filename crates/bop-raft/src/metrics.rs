use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::ptr::NonNull;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

#[cfg(feature = "serde")]
use serde::ser::SerializeStruct;

use bop_sys::*;

use crate::error::{RaftError, RaftResult};
use crate::types::{DataCenterId, LogIndex, ServerId};

const HISTOGRAM_MAX_BINS: usize = 65;

#[derive(Clone)]
pub struct Counter {
    inner: Arc<CounterInner>,
}

struct CounterInner {
    ptr: NonNull<bop_raft_counter>,
}

impl CounterInner {
    fn new(name: &str) -> RaftResult<Self> {
        let c_name = CString::new(name)?;
        let ptr = unsafe { bop_raft_counter_make(c_name.as_ptr(), name.len()) };
        let ptr = NonNull::new(ptr).ok_or(RaftError::NullPointer)?;
        Ok(Self { ptr })
    }
}

impl Counter {
    /// Create a counter handle for the given metric name.
    pub fn new(name: &str) -> RaftResult<Self> {
        Ok(Self {
            inner: Arc::new(CounterInner::new(name)?),
        })
    }

    /// Get counter name.
    pub fn name(&self) -> RaftResult<String> {
        unsafe {
            let ptr = bop_raft_counter_name(self.inner.ptr.as_ptr());
            if ptr.is_null() {
                return Ok(String::new());
            }
            let cstr = CStr::from_ptr(ptr);
            Ok(cstr.to_str()?.to_owned())
        }
    }

    /// Get the cached counter value from the handle.
    ///
    /// The returned value reflects the last successful fetch through the
    /// underlying FFI stat call.
    pub fn value(&self) -> u64 {
        unsafe { bop_raft_counter_value(self.inner.ptr.as_ptr()) }
    }

    #[inline]
    pub(crate) fn as_mut_ptr(&self) -> *mut bop_raft_counter {
        self.inner.ptr.as_ptr()
    }
}

impl Drop for CounterInner {
    fn drop(&mut self) {
        unsafe {
            bop_raft_counter_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for CounterInner {}
unsafe impl Sync for CounterInner {}

#[derive(Clone)]
pub struct Gauge {
    inner: Arc<GaugeInner>,
}

struct GaugeInner {
    ptr: NonNull<bop_raft_gauge>,
}

impl GaugeInner {
    fn new(name: &str) -> RaftResult<Self> {
        let c_name = CString::new(name)?;
        let ptr = unsafe { bop_raft_gauge_make(c_name.as_ptr(), name.len()) };
        let ptr = NonNull::new(ptr).ok_or(RaftError::NullPointer)?;
        Ok(Self { ptr })
    }
}

impl Gauge {
    /// Create a gauge handle.
    pub fn new(name: &str) -> RaftResult<Self> {
        Ok(Self {
            inner: Arc::new(GaugeInner::new(name)?),
        })
    }

    /// Get gauge name.
    pub fn name(&self) -> RaftResult<String> {
        unsafe {
            let ptr = bop_raft_gauge_name(self.inner.ptr.as_ptr());
            if ptr.is_null() {
                return Ok(String::new());
            }
            let cstr = CStr::from_ptr(ptr);
            Ok(cstr.to_str()?.to_owned())
        }
    }

    /// Get the cached gauge value from the handle.
    pub fn value(&self) -> i64 {
        unsafe { bop_raft_gauge_value(self.inner.ptr.as_ptr()) }
    }

    #[inline]
    pub(crate) fn as_mut_ptr(&self) -> *mut bop_raft_gauge {
        self.inner.ptr.as_ptr()
    }
}

impl Drop for GaugeInner {
    fn drop(&mut self) {
        unsafe {
            bop_raft_gauge_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for GaugeInner {}
unsafe impl Sync for GaugeInner {}

#[derive(Clone)]
pub struct Histogram {
    inner: Arc<HistogramInner>,
}

struct HistogramInner {
    ptr: NonNull<bop_raft_histogram>,
}

impl HistogramInner {
    fn new(name: &str) -> RaftResult<Self> {
        let c_name = CString::new(name)?;
        let ptr = unsafe { bop_raft_histogram_make(c_name.as_ptr(), name.len()) };
        let ptr = NonNull::new(ptr).ok_or(RaftError::NullPointer)?;
        Ok(Self { ptr })
    }
}

impl Histogram {
    /// Create a histogram handle.
    pub fn new(name: &str) -> RaftResult<Self> {
        Ok(Self {
            inner: Arc::new(HistogramInner::new(name)?),
        })
    }

    /// Get histogram name.
    pub fn name(&self) -> RaftResult<String> {
        unsafe {
            let ptr = bop_raft_histogram_name(self.inner.ptr.as_ptr());
            if ptr.is_null() {
                return Ok(String::new());
            }
            Ok(CStr::from_ptr(ptr).to_str()?.to_owned())
        }
    }

    /// Number of tracked buckets currently populated.
    pub fn size(&self) -> usize {
        unsafe { bop_raft_histogram_size(self.inner.ptr.as_ptr()) }
    }

    /// Read the count for a bucket capped at `upper_bound`.
    pub fn bucket(&self, upper_bound: f64) -> u64 {
        unsafe { bop_raft_histogram_get(self.inner.ptr.as_ptr(), upper_bound) }
    }

    /// Snapshot a set of bucket boundaries.
    pub fn snapshot(&self, bounds: &[f64]) -> Vec<(f64, u64)> {
        bounds
            .iter()
            .map(|&bound| (bound, self.bucket(bound)))
            .collect()
    }

    pub(crate) fn as_mut_ptr(&self) -> *mut bop_raft_histogram {
        self.inner.ptr.as_ptr()
    }

    pub(crate) fn collect_with_bounds(
        &self,
        server: *mut bop_raft_server,
        bounds: &[f64],
    ) -> RaftResult<HistogramBuckets> {
        let ok = unsafe { bop_raft_server_get_stat_histogram(server, self.as_mut_ptr()) };
        if !ok {
            return Err(RaftError::ServerError);
        }
        let mut buckets = Vec::with_capacity(bounds.len());
        for &bound in bounds {
            let count = unsafe { bop_raft_histogram_get(self.inner.ptr.as_ptr(), bound) };
            buckets.push(HistogramBucket {
                upper_bound: bound,
                count,
            });
        }
        Ok(HistogramBuckets {
            name: self.name()?,
            buckets,
        })
    }

    pub(crate) fn collect_default(
        &self,
        server: *mut bop_raft_server,
    ) -> RaftResult<HistogramBuckets> {
        self.collect_with_bounds(server, default_histogram_bounds())
    }
}

impl Drop for HistogramInner {
    fn drop(&mut self) {
        unsafe {
            bop_raft_histogram_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for HistogramInner {}
unsafe impl Sync for HistogramInner {}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct HistogramBucket {
    pub upper_bound: f64,
    pub count: u64,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct HistogramBuckets {
    pub name: String,
    pub buckets: Vec<HistogramBucket>,
}

impl HistogramBuckets {
    pub fn total_count(&self) -> u64 {
        self.buckets.iter().map(|b| b.count).sum()
    }

    pub fn percentiles(&self, percentiles: &[f64]) -> Vec<HistogramPercentile> {
        if self.buckets.is_empty() || percentiles.is_empty() {
            return Vec::new();
        }
        let total = self.total_count() as f64;
        if total == 0.0 {
            return percentiles
                .iter()
                .copied()
                .map(|p| HistogramPercentile {
                    percentile: p,
                    upper_bound: 0.0,
                })
                .collect();
        }

        let mut ordered: Vec<(usize, f64)> = percentiles.iter().copied().enumerate().collect();
        ordered.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut results = vec![None; percentiles.len()];
        let mut cumulative = 0u64;
        let mut ordered_iter = ordered.iter();
        let mut current = ordered_iter.next();

        for bucket in &self.buckets {
            cumulative = cumulative.saturating_add(bucket.count);
            while let Some((idx, pct)) = current {
                if *pct <= 0.0 {
                    results[*idx] = Some(0.0);
                    current = ordered_iter.next();
                    continue;
                }
                if *pct >= 100.0 {
                    results[*idx] = Some(
                        self.buckets
                            .last()
                            .map(|b| b.upper_bound)
                            .unwrap_or(bucket.upper_bound),
                    );
                    current = ordered_iter.next();
                    continue;
                }
                let threshold = (*pct / 100.0) * total;
                if (cumulative as f64) >= threshold {
                    results[*idx] = Some(bucket.upper_bound);
                    current = ordered_iter.next();
                } else {
                    break;
                }
            }
        }

        // Fill any remaining entries with the max bound to avoid Nones.
        let fallback = self.buckets.last().map(|b| b.upper_bound).unwrap_or(0.0);
        results
            .into_iter()
            .zip(percentiles.iter().copied())
            .map(|(maybe, pct)| HistogramPercentile {
                percentile: pct,
                upper_bound: maybe.unwrap_or(fallback),
            })
            .collect()
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct HistogramPercentile {
    pub percentile: f64,
    pub upper_bound: f64,
}

#[derive(Clone, Default)]
pub struct MetricsRegistry {
    inner: Arc<MetricsRegistryInner>,
}

#[derive(Default)]
struct MetricsRegistryInner {
    counters: Mutex<HashMap<String, Counter>>,
    gauges: Mutex<HashMap<String, Gauge>>,
    histograms: Mutex<HashMap<String, Histogram>>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn counter(&self, name: &str) -> RaftResult<Counter> {
        let mut guard = self
            .inner
            .counters
            .lock()
            .map_err(|_| RaftError::ServerError)?;
        if let Some(counter) = guard.get(name) {
            return Ok(counter.clone());
        }
        let counter = Counter::new(name)?;
        guard.insert(name.to_owned(), counter.clone());
        Ok(counter)
    }

    pub fn gauge(&self, name: &str) -> RaftResult<Gauge> {
        let mut guard = self
            .inner
            .gauges
            .lock()
            .map_err(|_| RaftError::ServerError)?;
        if let Some(gauge) = guard.get(name) {
            return Ok(gauge.clone());
        }
        let gauge = Gauge::new(name)?;
        guard.insert(name.to_owned(), gauge.clone());
        Ok(gauge)
    }

    pub fn histogram(&self, name: &str) -> RaftResult<Histogram> {
        let mut guard = self
            .inner
            .histograms
            .lock()
            .map_err(|_| RaftError::ServerError)?;
        if let Some(histogram) = guard.get(name) {
            return Ok(histogram.clone());
        }
        let histogram = Histogram::new(name)?;
        guard.insert(name.to_owned(), histogram.clone());
        Ok(histogram)
    }

    pub fn snapshot(&self, server: *mut bop_raft_server) -> RaftResult<MetricsSnapshot> {
        self.snapshot_with_bounds(server, default_histogram_bounds())
    }

    pub fn snapshot_with_bounds(
        &self,
        server: *mut bop_raft_server,
        bounds: &[f64],
    ) -> RaftResult<MetricsSnapshot> {
        let counters: Vec<(String, Counter)> = self
            .inner
            .counters
            .lock()
            .map_err(|_| RaftError::ServerError)?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let gauges: Vec<(String, Gauge)> = self
            .inner
            .gauges
            .lock()
            .map_err(|_| RaftError::ServerError)?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let histograms: Vec<(String, Histogram)> = self
            .inner
            .histograms
            .lock()
            .map_err(|_| RaftError::ServerError)?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let mut counter_points = Vec::with_capacity(counters.len());
        for (name, handle) in counters {
            let value = unsafe { bop_raft_server_get_stat_counter(server, handle.as_mut_ptr()) };
            counter_points.push(MetricPoint { name, value });
        }

        let mut gauge_points = Vec::with_capacity(gauges.len());
        for (name, handle) in gauges {
            let value = unsafe { bop_raft_server_get_stat_gauge(server, handle.as_mut_ptr()) };
            gauge_points.push(MetricPoint { name, value });
        }

        let mut histogram_points = Vec::with_capacity(histograms.len());
        for (name, handle) in histograms {
            match handle.collect_with_bounds(server, bounds) {
                Ok(mut collected) => {
                    collected.name = name;
                    histogram_points.push(collected);
                }
                Err(_) => continue,
            }
        }

        Ok(MetricsSnapshot {
            timestamp: SystemTime::now(),
            counters: counter_points,
            gauges: gauge_points,
            histograms: histogram_points,
        })
    }

    pub fn reset_counters(&self, server: *mut bop_raft_server) -> RaftResult<()> {
        let counters: Vec<Counter> = self
            .inner
            .counters
            .lock()
            .map_err(|_| RaftError::ServerError)?
            .values()
            .cloned()
            .collect();

        for counter in counters {
            unsafe { bop_raft_server_reset_counter(server, counter.as_mut_ptr()) };
        }
        Ok(())
    }

    pub fn reset_gauges(&self, server: *mut bop_raft_server) -> RaftResult<()> {
        let gauges: Vec<Gauge> = self
            .inner
            .gauges
            .lock()
            .map_err(|_| RaftError::ServerError)?
            .values()
            .cloned()
            .collect();

        for gauge in gauges {
            unsafe { bop_raft_server_reset_gauge(server, gauge.as_mut_ptr()) };
        }
        Ok(())
    }

    pub fn reset_histograms(&self, server: *mut bop_raft_server) -> RaftResult<()> {
        let histograms: Vec<Histogram> = self
            .inner
            .histograms
            .lock()
            .map_err(|_| RaftError::ServerError)?
            .values()
            .cloned()
            .collect();

        for histogram in histograms {
            unsafe { bop_raft_server_reset_histogram(server, histogram.as_mut_ptr()) };
        }
        Ok(())
    }

    pub fn reset_all(&self, server: *mut bop_raft_server) -> RaftResult<()> {
        self.reset_counters(server)?;
        self.reset_gauges(server)?;
        self.reset_histograms(server)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(bound(serialize = "T: serde::Serialize")))]
#[derive(Debug, Clone)]
pub struct MetricPoint<T> {
    pub name: String,
    pub value: T,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub timestamp: SystemTime,
    pub counters: Vec<MetricPoint<u64>>,
    pub gauges: Vec<MetricPoint<i64>>,
    pub histograms: Vec<HistogramBuckets>,
}

impl MetricsSnapshot {
    pub fn is_empty(&self) -> bool {
        self.counters.is_empty() && self.gauges.is_empty() && self.histograms.is_empty()
    }

    #[cfg(feature = "serde")]
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct MetricCatalog {
    pub leader_uptime: &'static str,
    pub commit_throughput: &'static str,
    pub append_entries_latency: &'static str,
}

impl Default for MetricCatalog {
    fn default() -> Self {
        Self {
            leader_uptime: "leader_uptime_us",
            commit_throughput: "commit_throughput",
            append_entries_latency: "append_entries_latency_us",
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct LatencyPercentiles {
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
}

impl LatencyPercentiles {
    fn from_percentiles(percentiles: Vec<HistogramPercentile>) -> Option<Self> {
        if percentiles.len() < 3 {
            return None;
        }
        let mut p50 = None;
        let mut p95 = None;
        let mut p99 = None;
        for item in percentiles {
            if (item.percentile - 50.0).abs() < f64::EPSILON {
                p50 = Some(item.upper_bound);
            }
            if (item.percentile - 95.0).abs() < f64::EPSILON {
                p95 = Some(item.upper_bound);
            }
            if (item.percentile - 99.0).abs() < f64::EPSILON {
                p99 = Some(item.upper_bound);
            }
        }
        Some(Self {
            p50: p50.unwrap_or_default(),
            p95: p95.unwrap_or_default(),
            p99: p99.unwrap_or_default(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ServerMetrics {
    pub leader_uptime: Duration,
    pub commit_throughput: u64,
    pub append_entries_latency: Option<LatencyPercentiles>,
}

impl ServerMetrics {
    pub(crate) fn gather(
        server: *mut bop_raft_server,
        registry: &MetricsRegistry,
        catalog: &MetricCatalog,
    ) -> RaftResult<Self> {
        let leader_uptime_gauge = registry.gauge(catalog.leader_uptime)?;
        let uptime_us =
            unsafe { bop_raft_server_get_stat_gauge(server, leader_uptime_gauge.as_mut_ptr()) };
        let commit_counter = registry.counter(catalog.commit_throughput)?;
        let commit_throughput =
            unsafe { bop_raft_server_get_stat_counter(server, commit_counter.as_mut_ptr()) };
        let append_entries_latency = registry
            .histogram(catalog.append_entries_latency)
            .ok()
            .and_then(|histogram| {
                histogram
                    .collect_default(server)
                    .ok()
                    .map(|buckets| buckets.percentiles(&[50.0, 95.0, 99.0]))
                    .and_then(LatencyPercentiles::from_percentiles)
            });

        Ok(Self {
            leader_uptime: Duration::from_micros(uptime_us as u64),
            commit_throughput,
            append_entries_latency,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PeerMetrics {
    pub server_id: ServerId,
    pub endpoint: Option<String>,
    pub data_center: Option<DataCenterId>,
    pub is_learner: Option<bool>,
    pub last_log_index: LogIndex,
    pub replication_lag: u64,
    pub commit_gap: u64,
    pub last_success_response: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_caches_handles() {
        let registry = MetricsRegistry::new();
        let counter_a = registry.counter("test.counter").expect("counter");
        let counter_b = registry.counter("test.counter").expect("counter");
        assert_eq!(counter_a.as_mut_ptr(), counter_b.as_mut_ptr());

        let gauge_a = registry.gauge("test.gauge").expect("gauge");
        let gauge_b = registry.gauge("test.gauge").expect("gauge");
        assert_eq!(gauge_a.as_mut_ptr(), gauge_b.as_mut_ptr());
    }

    #[test]
    fn histogram_percentiles_basic() {
        let buckets = HistogramBuckets {
            name: "latency".to_string(),
            buckets: vec![
                HistogramBucket {
                    upper_bound: 10.0,
                    count: 5,
                },
                HistogramBucket {
                    upper_bound: 20.0,
                    count: 5,
                },
                HistogramBucket {
                    upper_bound: 30.0,
                    count: 10,
                },
            ],
        };

        let pct = buckets.percentiles(&[50.0, 90.0]);
        assert_eq!(pct.len(), 2);
        assert_eq!(pct[0].percentile, 50.0);
        assert_eq!(pct[0].upper_bound, 20.0);
        assert_eq!(pct[1].upper_bound, 30.0);
    }

    #[test]
    fn registry_reset_with_null_server_is_noop() {
        let registry = MetricsRegistry::new();
        registry.counter("counter").unwrap();
        registry.gauge("gauge").unwrap();
        registry.histogram("hist").unwrap();

        registry.reset_all(std::ptr::null_mut()).expect("reset");
    }
}

fn default_histogram_bounds() -> &'static [f64] {
    static BOUNDS: OnceLock<Vec<f64>> = OnceLock::new();
    BOUNDS.get_or_init(|| {
        let mut bounds = Vec::with_capacity(HISTOGRAM_MAX_BINS);
        bounds.push(u64::MAX as f64);
        for exp in (0..=63).rev() {
            bounds.push((1u128 << exp) as f64);
        }
        bounds
    })
}

#[cfg(feature = "serde")]
fn duration_as_micros(duration: &Duration) -> u64 {
    let micros = duration.as_micros();
    if micros > u64::MAX as u128 {
        u64::MAX
    } else {
        micros as u64
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ServerMetrics {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("ServerMetrics", 3)?;
        state.serialize_field("leader_uptime_us", &duration_as_micros(&self.leader_uptime))?;
        state.serialize_field("commit_throughput", &self.commit_throughput)?;
        state.serialize_field("append_entries_latency", &self.append_entries_latency)?;
        state.end()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for PeerMetrics {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("PeerMetrics", 8)?;
        state.serialize_field("server_id", &self.server_id.0)?;
        state.serialize_field("endpoint", &self.endpoint)?;
        state.serialize_field("data_center", &self.data_center.map(|dc| dc.0))?;
        state.serialize_field("is_learner", &self.is_learner)?;
        state.serialize_field("last_log_index", &self.last_log_index.0)?;
        state.serialize_field("replication_lag", &self.replication_lag)?;
        state.serialize_field("commit_gap", &self.commit_gap)?;
        state.serialize_field(
            "last_success_response_us",
            &duration_as_micros(&self.last_success_response),
        )?;
        state.end()
    }
}
