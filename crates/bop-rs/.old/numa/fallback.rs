//! NUMA fallback strategies for non-NUMA systems and virtualized environments

use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use super::config::{FallbackStrategy, NumaConfig};
use super::error::{NumaError, NumaResult};
use super::topology::{NumaNode, NumaTopology};
use super::types::{CpuId, MemorySize, NumaNodeId, NumaPriority, ThreadId};

/// Manages NUMA fallback strategies for systems without NUMA support
#[derive(Debug, Clone)]
pub struct NumaFallback {
    /// The fallback strategy being used
    strategy: FallbackStrategy,

    /// The simulated topology (if applicable)
    simulated_topology: Option<Arc<NumaTopology>>,

    /// Thread-to-node mapping for single-node fallback
    thread_mapping: Arc<ThreadMapping>,

    /// Performance metrics for fallback strategies
    metrics: Arc<FallbackMetrics>,
}

/// Maps threads to simulated NUMA nodes
#[derive(Debug, Default)]
struct ThreadMapping {
    /// Round-robin counter for thread assignment
    round_robin: AtomicUsize,

    /// Thread-to-node assignment cache
    thread_assignments: RwLock<std::collections::HashMap<ThreadId, NumaNodeId>>,
}

/// Performance metrics for fallback strategies
#[derive(Debug, Default)]
struct FallbackMetrics {
    /// Number of fallback operations performed
    fallback_count: AtomicUsize,

    /// Number of cache hits in thread mapping
    cache_hits: AtomicUsize,

    /// Number of cache misses in thread mapping
    cache_misses: AtomicUsize,

    /// Average latency for fallback operations (in nanoseconds)
    avg_latency: AtomicUsize,
}

impl NumaFallback {
    /// Create a new NUMA fallback manager
    pub fn new(strategy: FallbackStrategy) -> Self {
        Self {
            strategy,
            simulated_topology: None,
            thread_mapping: Arc::new(ThreadMapping::default()),
            metrics: Arc::new(FallbackMetrics::default()),
        }
    }

    /// Create a fallback manager with configuration
    pub fn with_config(config: &NumaConfig) -> NumaResult<Self> {
        let strategy = config.fallback_strategy;
        let mut fallback = Self::new(strategy);

        // Initialize simulated topology if needed
        if strategy == FallbackStrategy::Simulated {
            fallback.initialize_simulated_topology(config)?;
        }

        Ok(fallback)
    }

    /// Initialize simulated NUMA topology
    fn initialize_simulated_topology(&mut self, config: &NumaConfig) -> NumaResult<()> {
        let node_count = config.simulated_node_count.unwrap_or(2) as usize;
        let topology = self.create_simulated_topology(node_count, config)?;
        self.simulated_topology = Some(Arc::new(topology));
        Ok(())
    }

    /// Create a simulated NUMA topology
    fn create_simulated_topology(
        &self,
        node_count: usize,
        config: &NumaConfig,
    ) -> NumaResult<NumaTopology> {
        if node_count == 0 {
            return Err(NumaError::InvalidConfig(
                "Simulated node count must be greater than 0".to_string(),
            ));
        }

        let mut nodes = Vec::new();
        let total_cpus = self.detect_total_cpus();
        let total_memory = self.detect_total_memory();

        let cpus_per_node = (total_cpus + node_count - 1) / node_count;
        let memory_per_node = MemorySize::new(total_memory.inner() / node_count);

        for i in 0..node_count {
            let start_cpu = (i * cpus_per_node) as u32;
            let end_cpu = std::cmp::min(start_cpu + cpus_per_node as u32, total_cpus as u32);

            let cpus: Vec<CpuId> = (start_cpu..end_cpu).map(CpuId).collect();

            // Create distance matrix (simulated)
            let distance_matrix: Vec<_> = (0..node_count)
                .map(|j| {
                    if i == j {
                        10 // Local distance
                    } else {
                        20 // Remote distance
                    }
                })
                .map(crate::numa::types::NumaDistance::new)
                .collect();

            let node = NumaNode {
                id: NumaNodeId(i as u32),
                cpus,
                memory_size: memory_per_node,
                distance_matrix,
                priority: if config.is_preferred_node(NumaNodeId(i as u32)) {
                    NumaPriority::high()
                } else {
                    NumaPriority::normal()
                },
                preferred: config.is_preferred_node(NumaNodeId(i as u32)),
                info: crate::numa::topology::NumaNodeInfo {
                    online: true,
                    name: Some(format!("Simulated NUMA Node {}", i)),
                    ..Default::default()
                },
            };

            nodes.push(node);
        }

        Ok(NumaTopology {
            nodes,
            total_memory,
            cpu_count: total_cpus,
            is_numa_available: false,
            detection_method: crate::numa::types::DetectionMethod::Fallback,
            system_info: crate::numa::topology::NumaSystemInfo {
                max_nodes: Some(node_count as u32),
                architecture: Some(self.detect_architecture()),
                ..Default::default()
            },
        })
    }

    /// Get the NUMA node for a thread using the fallback strategy
    pub fn get_thread_node(&self, thread_id: ThreadId) -> NumaResult<NumaNodeId> {
        let start_time = std::time::Instant::now();

        let result = match self.strategy {
            FallbackStrategy::SingleNode => self.get_single_node_thread(thread_id),
            FallbackStrategy::Simulated => self.get_simulated_thread_node(thread_id),
            FallbackStrategy::Disabled => Err(NumaError::NumaNotAvailable),
            FallbackStrategy::Hybrid => self.get_hybrid_thread_node(thread_id),
        };

        // Update metrics
        self.metrics.fallback_count.fetch_add(1, Ordering::Relaxed);
        let latency = start_time.elapsed().as_nanos() as usize;
        self.update_avg_latency(latency);

        result
    }

    /// Get thread node for single-node fallback strategy
    fn get_single_node_thread(&self, _thread_id: ThreadId) -> NumaResult<NumaNodeId> {
        // All threads go to node 0
        Ok(NumaNodeId(0))
    }

    /// Get thread node for simulated fallback strategy
    fn get_simulated_thread_node(&self, thread_id: ThreadId) -> NumaResult<NumaNodeId> {
        let topology = self.simulated_topology.as_ref().ok_or_else(|| {
            NumaError::NotSupported("Simulated topology not initialized".to_string())
        })?;

        // Check cache first
        {
            let assignments = self.thread_mapping.thread_assignments.read().map_err(|_| {
                NumaError::SystemCallFailed("Thread assignments read lock poisoned".to_string())
            })?;
            if let Some(&node_id) = assignments.get(&thread_id) {
                self.metrics.cache_hits.fetch_add(1, Ordering::Relaxed);
                return Ok(node_id);
            }
        }

        self.metrics.cache_misses.fetch_add(1, Ordering::Relaxed);

        // Round-robin assignment
        let node_count = topology.nodes.len();
        let index = self
            .thread_mapping
            .round_robin
            .fetch_add(1, Ordering::Relaxed)
            % node_count;
        let node_id = topology.nodes[index].id;

        // Cache the assignment
        {
            let mut assignments = self
                .thread_mapping
                .thread_assignments
                .write()
                .map_err(|_| {
                    NumaError::SystemCallFailed(
                        "Thread assignments write lock poisoned".to_string(),
                    )
                })?;
            assignments.insert(thread_id, node_id);
        }

        Ok(node_id)
    }

    /// Get thread node for hybrid fallback strategy
    fn get_hybrid_thread_node(&self, thread_id: ThreadId) -> NumaResult<NumaNodeId> {
        // Try to use CPU affinity first, fall back to round-robin
        if let Some(cpu_id) = self.get_thread_cpu_affinity(thread_id) {
            // Map CPU to simulated node
            if let Some(topology) = &self.simulated_topology {
                for node in &topology.nodes {
                    if node.cpus.contains(&cpu_id) {
                        return Ok(node.id);
                    }
                }
            }
        }

        // Fall back to round-robin
        self.get_simulated_thread_node(thread_id)
    }

    /// Get CPU affinity for a thread (platform-specific)
    fn get_thread_cpu_affinity(&self, _thread_id: ThreadId) -> Option<CpuId> {
        // This would be platform-specific implementation
        // For now, return None to indicate we couldn't determine affinity
        None
    }

    /// Get the best NUMA node for memory allocation
    pub fn get_best_memory_node(&self, size: usize) -> NumaResult<NumaNodeId> {
        match self.strategy {
            FallbackStrategy::SingleNode => Ok(NumaNodeId(0)),
            FallbackStrategy::Simulated => self.get_simulated_memory_node(size),
            FallbackStrategy::Disabled => Err(NumaError::NumaNotAvailable),
            FallbackStrategy::Hybrid => self.get_hybrid_memory_node(size),
        }
    }

    /// Get memory node for simulated fallback strategy
    fn get_simulated_memory_node(&self, size: usize) -> NumaResult<NumaNodeId> {
        let topology = self.simulated_topology.as_ref().ok_or_else(|| {
            NumaError::NotSupported("Simulated topology not initialized".to_string())
        })?;

        // Find node with most available memory
        let mut best_node = &topology.nodes[0];
        let mut best_memory = best_node.memory_size;

        for node in &topology.nodes[1..] {
            if node.memory_size > best_memory {
                best_node = node;
                best_memory = node.memory_size;
            }
        }

        Ok(best_node.id)
    }

    /// Get memory node for hybrid fallback strategy
    fn get_hybrid_memory_node(&self, size: usize) -> NumaResult<NumaNodeId> {
        // For hybrid, try to allocate on the same node as the current thread
        let current_thread = ThreadId::current();

        if let Ok(thread_node) = self.get_thread_node(current_thread) {
            // Check if the node has enough memory
            if let Some(topology) = &self.simulated_topology {
                if let Some(node) = topology.get_node(thread_node) {
                    if node.memory_size.inner() >= size {
                        return Ok(thread_node);
                    }
                }
            }
        }

        // Fall back to best available node
        self.get_simulated_memory_node(size)
    }

    /// Apply thread affinity using the fallback strategy
    pub fn apply_thread_affinity(&self, thread_id: ThreadId) -> NumaResult<()> {
        let node_id = self.get_thread_node(thread_id)?;

        match self.strategy {
            FallbackStrategy::SingleNode => {
                // No actual affinity needed for single node
                Ok(())
            }
            FallbackStrategy::Simulated | FallbackStrategy::Hybrid => {
                self.apply_simulated_affinity(thread_id, node_id)
            }
            FallbackStrategy::Disabled => Err(NumaError::NumaNotAvailable),
        }
    }

    /// Apply simulated thread affinity
    fn apply_simulated_affinity(&self, thread_id: ThreadId, node_id: NumaNodeId) -> NumaResult<()> {
        let topology = self.simulated_topology.as_ref().ok_or_else(|| {
            NumaError::NotSupported("Simulated topology not initialized".to_string())
        })?;

        let node = topology
            .get_node(node_id)
            .ok_or_else(|| NumaError::InvalidNode(node_id))?;

        // Set CPU affinity to the CPUs in this node
        if !node.cpus.is_empty() {
            self.set_thread_cpu_affinity(thread_id, &node.cpus)?;
        }

        Ok(())
    }

    /// Set CPU affinity for a thread (platform-specific)
    fn set_thread_cpu_affinity(&self, _thread_id: ThreadId, _cpus: &[CpuId]) -> NumaResult<()> {
        // This would be platform-specific implementation
        // For now, just return Ok as a no-op
        Ok(())
    }

    /// Get fallback performance metrics
    pub fn get_metrics(&self) -> FallbackMetricsSnapshot {
        FallbackMetricsSnapshot {
            fallback_count: self.metrics.fallback_count.load(Ordering::Relaxed),
            cache_hits: self.metrics.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.metrics.cache_misses.load(Ordering::Relaxed),
            avg_latency_ns: self.metrics.avg_latency.load(Ordering::Relaxed),
            strategy: self.strategy,
        }
    }

    /// Reset fallback metrics
    pub fn reset_metrics(&self) {
        self.metrics.fallback_count.store(0, Ordering::Relaxed);
        self.metrics.cache_hits.store(0, Ordering::Relaxed);
        self.metrics.cache_misses.store(0, Ordering::Relaxed);
        self.metrics.avg_latency.store(0, Ordering::Relaxed);
    }

    /// Update average latency
    fn update_avg_latency(&self, new_latency: usize) {
        let current_avg = self.metrics.avg_latency.load(Ordering::Relaxed);
        let count = self.metrics.fallback_count.load(Ordering::Relaxed);

        if count > 0 {
            let new_avg = (current_avg * (count - 1) + new_latency) / count;
            self.metrics.avg_latency.store(new_avg, Ordering::Relaxed);
        } else {
            self.metrics
                .avg_latency
                .store(new_latency, Ordering::Relaxed);
        }
    }

    /// Detect total number of CPUs
    fn detect_total_cpus(&self) -> usize {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    }

    /// Detect total system memory
    fn detect_total_memory(&self) -> crate::numa::types::MemorySize {
        crate::numa::topology::NumaTopology::detect_total_memory()
    }

    /// Detect system architecture
    fn detect_architecture(&self) -> String {
        crate::numa::topology::NumaTopology::detect_architecture()
    }

    /// Get the current fallback strategy
    pub fn strategy(&self) -> FallbackStrategy {
        self.strategy
    }

    /// Check if fallback is enabled
    pub fn is_enabled(&self) -> bool {
        self.strategy != FallbackStrategy::Disabled
    }

    /// Get the simulated topology (if available)
    pub fn simulated_topology(&self) -> Option<&Arc<NumaTopology>> {
        self.simulated_topology.as_ref()
    }
}

/// Snapshot of fallback performance metrics
#[derive(Debug, Clone)]
pub struct FallbackMetricsSnapshot {
    /// Number of fallback operations performed
    pub fallback_count: usize,

    /// Number of cache hits in thread mapping
    pub cache_hits: usize,

    /// Number of cache misses in thread mapping
    pub cache_misses: usize,

    /// Average latency for fallback operations (in nanoseconds)
    pub avg_latency_ns: usize,

    /// The fallback strategy being used
    pub strategy: FallbackStrategy,
}

impl FallbackMetricsSnapshot {
    /// Calculate cache hit rate
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }

    /// Get a summary of the metrics
    pub fn summary(&self) -> String {
        format!(
            "Fallback Metrics: {} operations, {:.2}% cache hit rate, {}ns avg latency, strategy: {:?}",
            self.fallback_count,
            self.cache_hit_rate() * 100.0,
            self.avg_latency_ns,
            self.strategy
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_node_fallback() {
        let fallback = NumaFallback::new(FallbackStrategy::SingleNode);

        let thread_id = ThreadId::current();
        let node_id = fallback.get_thread_node(thread_id).unwrap();

        assert_eq!(node_id, NumaNodeId(0));
        assert_eq!(fallback.strategy(), FallbackStrategy::SingleNode);
        assert!(fallback.is_enabled());
    }

    #[test]
    fn test_disabled_fallback() {
        let fallback = NumaFallback::new(FallbackStrategy::Disabled);

        let thread_id = ThreadId::current();
        let result = fallback.get_thread_node(thread_id);

        assert!(matches!(result, Err(NumaError::NumaNotAvailable)));
        assert!(!fallback.is_enabled());
    }

    #[test]
    fn test_simulated_fallback() {
        let mut config = NumaConfig::default();
        config.fallback_strategy = FallbackStrategy::Simulated;
        config.simulated_node_count = Some(2);

        let fallback = NumaFallback::with_config(&config).unwrap();

        let thread_id = ThreadId::current();
        let node_id = fallback.get_thread_node(thread_id).unwrap();

        assert!(node_id.0 < 2); // Should be 0 or 1
        assert_eq!(fallback.strategy(), FallbackStrategy::Simulated);

        // Check that we have a simulated topology
        assert!(fallback.simulated_topology().is_some());
    }

    #[test]
    fn test_hybrid_fallback() {
        let mut config = NumaConfig::default();
        config.fallback_strategy = FallbackStrategy::Hybrid;
        config.simulated_node_count = Some(4);

        let fallback = NumaFallback::with_config(&config).unwrap();

        let thread_id = ThreadId::current();
        let node_id = fallback.get_thread_node(thread_id).unwrap();

        assert!(node_id.0 < 4); // Should be 0, 1, 2, or 3
        assert_eq!(fallback.strategy(), FallbackStrategy::Hybrid);
    }

    #[test]
    fn test_memory_allocation() {
        let mut config = NumaConfig::default();
        config.fallback_strategy = FallbackStrategy::Simulated;
        config.simulated_node_count = Some(2);

        let fallback = NumaFallback::with_config(&config).unwrap();

        let node_id = fallback.get_best_memory_node(1024).unwrap();
        assert!(node_id.0 < 2);
    }

    #[test]
    fn test_thread_affinity() {
        let mut config = NumaConfig::default();
        config.fallback_strategy = FallbackStrategy::Simulated;
        config.simulated_node_count = Some(2);

        let fallback = NumaFallback::with_config(&config).unwrap();

        let thread_id = ThreadId::current();
        let result = fallback.apply_thread_affinity(thread_id);

        assert!(result.is_ok());
    }

    #[test]
    fn test_metrics() {
        let fallback = NumaFallback::new(FallbackStrategy::SingleNode);

        // Perform some operations
        let thread_id = ThreadId::current();
        let _ = fallback.get_thread_node(thread_id);
        let _ = fallback.get_thread_node(thread_id);

        let metrics = fallback.get_metrics();
        assert!(metrics.fallback_count > 0);
        assert_eq!(metrics.strategy, FallbackStrategy::SingleNode);

        println!("Metrics: {}", metrics.summary());

        // Reset metrics
        fallback.reset_metrics();
        let reset_metrics = fallback.get_metrics();
        assert_eq!(reset_metrics.fallback_count, 0);
    }

    #[test]
    fn test_cache_hit_rate() {
        let metrics = FallbackMetricsSnapshot {
            fallback_count: 100,
            cache_hits: 80,
            cache_misses: 20,
            avg_latency_ns: 1000,
            strategy: FallbackStrategy::Simulated,
        };

        assert_eq!(metrics.cache_hit_rate(), 0.8);
    }

    #[test]
    fn test_simulated_topology_creation() {
        let fallback = NumaFallback::new(FallbackStrategy::SingleNode);
        let config = NumaConfig::default();

        let topology = fallback.create_simulated_topology(4, &config).unwrap();

        assert_eq!(topology.nodes.len(), 4);
        assert!(!topology.nodes.is_empty());
        assert!(!topology.is_numa_available);

        // Check that nodes have reasonable properties
        for (i, node) in topology.nodes.iter().enumerate() {
            assert_eq!(node.id.0, i as u32);
            assert!(!node.cpus.is_empty());
            assert!(node.memory_size.inner() > 0);
            assert_eq!(node.distance_matrix.len(), 4);
        }
    }

    #[test]
    fn test_invalid_simulated_node_count() {
        let fallback = NumaFallback::new(FallbackStrategy::SingleNode);
        let config = NumaConfig::default();

        let result = fallback.create_simulated_topology(0, &config);
        assert!(matches!(result, Err(NumaError::InvalidConfig(_))));
    }
}
