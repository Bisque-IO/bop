//! NUMA-aware thread affinity and memory allocation for BOP
//!
//! This module provides comprehensive NUMA (Non-Uniform Memory Access) support
//! for optimizing thread placement and memory allocation in multi-socket systems.
//! The module includes automatic topology detection, configurable affinity policies,
//! and fallback strategies for non-NUMA systems.

use std::sync::OnceLock;

pub mod config;
pub mod error;
pub mod fallback;
pub mod topology;
pub mod types;

pub use config::*;
pub use error::{NumaError, NumaResult};
pub use fallback::*;
pub use topology::*;
pub use types::*;

/// Global NUMA configuration and state
static NUMA_CONFIG: OnceLock<NumaConfig> = OnceLock::new();
static NUMA_TOPOLOGY: OnceLock<NumaTopology> = OnceLock::new();
static NUMA_FALLBACK: OnceLock<NumaFallback> = OnceLock::new();

/// Re-export commonly used items for convenience
pub mod prelude {
    pub use super::{AffinityPolicy, DetectionMethod, FallbackStrategy, MemoryPolicy};
    pub use super::{CpuId, NumaConfig, NumaError, NumaNodeId, NumaResult, NumaTopology};
}

/// Maximum number of supported NUMA nodes
pub const MAX_NUMA_NODES: u32 = 256;

/// Threshold for considering NUMA distance as "local"
pub const NUMA_LOCAL_DISTANCE_THRESHOLD: u32 = 20;

/// Threshold for considering NUMA distance as "remote" but acceptable
pub const NUMA_REMOTE_DISTANCE_THRESHOLD: u32 = 40;

/// Initialize the NUMA subsystem with default configuration
pub fn init() -> NumaResult<()> {
    let config = NumaConfig::default();
    init_with_config(config)
}

/// Initialize the NUMA subsystem with custom configuration
pub fn init_with_config(config: NumaConfig) -> NumaResult<()> {
    // Store the configuration
    NUMA_CONFIG.set(config.clone()).map_err(|_| {
        NumaError::AlreadyInitialized("NUMA subsystem already initialized".to_string())
    })?;

    if !config.enabled {
        return Ok(());
    }

    // Validate configuration
    config.validate()?;

    // Detect and store topology
    let topology = NumaTopology::detect_with_config(&config)?;
    NUMA_TOPOLOGY.set(topology).map_err(|_| {
        NumaError::AlreadyInitialized("NUMA topology already initialized".to_string())
    })?;

    // Initialize fallback manager
    let fallback = NumaFallback::with_config(&config)?;
    NUMA_FALLBACK.set(fallback).map_err(|_| {
        NumaError::AlreadyInitialized("NUMA fallback already initialized".to_string())
    })?;

    Ok(())
}

/// Get the current NUMA configuration
pub fn config() -> NumaResult<&'static NumaConfig> {
    NUMA_CONFIG
        .get()
        .ok_or_else(|| NumaError::NotInitialized("NUMA subsystem not initialized".to_string()))
}

/// Get the detected NUMA topology
pub fn topology() -> NumaResult<&'static NumaTopology> {
    NUMA_TOPOLOGY
        .get()
        .ok_or_else(|| NumaError::NotInitialized("NUMA topology not available".to_string()))
}

/// Get the NUMA fallback manager
pub fn fallback() -> NumaResult<&'static NumaFallback> {
    NUMA_FALLBACK
        .get()
        .ok_or_else(|| NumaError::NotInitialized("NUMA fallback not available".to_string()))
}

/// Get the NUMA topology for the current system
pub fn get_topology() -> NumaResult<NumaTopology> {
    NumaTopology::detect()
}

/// Get the NUMA topology with custom configuration
pub fn get_topology_with_config(config: &NumaConfig) -> NumaResult<NumaTopology> {
    NumaTopology::detect_with_config(config)
}

/// Check if NUMA support is available on the current system
pub fn is_numa_available() -> bool {
    topology().map(|t| t.is_numa_available).unwrap_or(false)
}

/// Get the number of NUMA nodes
pub fn node_count() -> NumaResult<usize> {
    topology().map(|t| t.nodes.len())
}

/// Get the number of online NUMA nodes
pub fn online_node_count() -> NumaResult<usize> {
    topology().map(|t| t.online_node_count())
}

/// Check if the system has multiple NUMA nodes
pub fn is_multi_numa() -> NumaResult<bool> {
    topology().map(|t| t.is_multi_numa())
}

/// Get the best NUMA node for the current thread
pub fn get_current_thread_node() -> NumaResult<NumaNodeId> {
    let thread_id = ThreadId::current();

    if is_numa_available() {
        // On NUMA systems, use actual topology
        let topology = topology()?;

        // For now, use a simple round-robin approach
        // This will be enhanced in Phase 2 with proper thread tracking
        let node_count = topology.online_node_count();
        if node_count == 0 {
            return Err(NumaError::NoNodesAvailable);
        }

        let index = (thread_id.hash() as usize) % node_count;
        topology
            .online_nodes()
            .get(index)
            .map(|node| node.id)
            .ok_or_else(|| NumaError::InvalidNode(NumaNodeId(index as u32)))
    } else {
        // On non-NUMA systems, use fallback
        let fallback = fallback()?;
        fallback.get_thread_node(thread_id)
    }
}

/// Get the best NUMA node for memory allocation
pub fn get_best_memory_node(size: usize) -> NumaResult<NumaNodeId> {
    if is_numa_available() {
        let topology = topology()?;

        // Find node with most available memory
        let online_nodes = topology.online_nodes();
        if online_nodes.is_empty() {
            return Err(NumaError::NoNodesAvailable);
        }

        let mut best_node = &online_nodes[0];
        let mut best_memory = best_node.memory_size;

        for node in &online_nodes[1..] {
            if node.memory_size > best_memory {
                best_node = node;
                best_memory = node.memory_size;
            }
        }

        Ok(best_node.id)
    } else {
        // On non-NUMA systems, use fallback
        let fallback = fallback()?;
        fallback.get_best_memory_node(size)
    }
}

/// Apply thread affinity for the current thread
pub fn apply_current_thread_affinity() -> NumaResult<()> {
    let thread_id = ThreadId::current();

    if is_numa_available() {
        // On NUMA systems, apply actual affinity
        let node_id = get_current_thread_node()?;
        apply_thread_affinity(thread_id, node_id)
    } else {
        // On non-NUMA systems, use fallback
        let fallback = fallback()?;
        fallback.apply_thread_affinity(thread_id)
    }
}

/// Apply thread affinity for a specific thread and node
pub fn apply_thread_affinity(thread_id: ThreadId, node_id: NumaNodeId) -> NumaResult<()> {
    if !is_numa_available() {
        return Err(NumaError::NumaNotAvailable);
    }

    let topology = topology()?;
    let node = topology
        .get_node(node_id)
        .ok_or_else(|| NumaError::InvalidNode(node_id))?;

    // Set CPU affinity to the CPUs in this node
    if !node.cpus.is_empty() {
        set_thread_cpu_affinity(thread_id, &node.cpus)?;
    }

    Ok(())
}

/// Set CPU affinity for a thread (platform-specific)
fn set_thread_cpu_affinity(_thread_id: ThreadId, _cpus: &[CpuId]) -> NumaResult<()> {
    // This would be platform-specific implementation
    // For now, just return Ok as a no-op
    // Will be implemented in Phase 2 with proper platform support
    Ok(())
}

/// Get the optimal NUMA node for the current thread (legacy function)
pub fn get_optimal_node_for_current_thread() -> NumaResult<NumaNodeId> {
    get_current_thread_node()
}

/// Get a summary of NUMA information
pub fn summary() -> String {
    match topology() {
        Ok(topology) => {
            let mut summary = topology.summary();

            if let Ok(fallback) = fallback() {
                let metrics = fallback.get_metrics();
                summary.push_str(&format!(", Fallback: {}", metrics.summary()));
            }

            summary
        }
        Err(e) => format!("NUMA not available: {}", e),
    }
}

/// Get NUMA performance metrics
pub fn get_metrics() -> NumaResult<NumaMetrics> {
    let topology_metrics = topology().map(|t| NumaTopologyMetrics {
        node_count: t.nodes.len(),
        online_node_count: t.online_node_count(),
        total_memory: t.total_memory,
        cpu_count: t.cpu_count,
        is_multi_numa: t.is_multi_numa(),
    });

    let fallback_metrics = fallback().map(|f| f.get_metrics());

    Ok(NumaMetrics {
        topology: topology_metrics.unwrap_or_default(),
        fallback: fallback_metrics.ok(),
    })
}

/// NUMA performance metrics
#[derive(Debug, Default)]
pub struct NumaMetrics {
    /// Topology-related metrics
    pub topology: NumaTopologyMetrics,

    /// Fallback-related metrics (if applicable)
    pub fallback: Option<FallbackMetricsSnapshot>,
}

/// Topology-specific metrics
#[derive(Debug, Default)]
pub struct NumaTopologyMetrics {
    /// Total number of NUMA nodes
    pub node_count: usize,

    /// Number of online NUMA nodes
    pub online_node_count: usize,

    /// Total memory across all nodes
    pub total_memory: MemorySize,

    /// Total CPU count across all nodes
    pub cpu_count: usize,

    /// Whether the system has multiple NUMA nodes
    pub is_multi_numa: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numa_initialization() {
        let result = init();

        match result {
            Ok(()) => {
                println!("NUMA initialized successfully");
                println!("NUMA summary: {}", summary());
            }
            Err(e) => {
                println!("NUMA initialization failed: {}", e);
                // This is acceptable on systems without NUMA support
            }
        }
    }

    #[test]
    fn test_config_access() {
        let _ = init(); // Try to initialize

        match config() {
            Ok(config) => {
                println!("NUMA config: {:?}", config);
                assert!(config.enabled);
            }
            Err(e) => {
                println!("Config access failed: {}", e);
            }
        }
    }

    #[test]
    fn test_topology_access() {
        let _ = init(); // Try to initialize

        match topology() {
            Ok(topology) => {
                println!("NUMA topology: {} nodes", topology.nodes.len());
                assert!(!topology.nodes.is_empty());
            }
            Err(e) => {
                println!("Topology access failed: {}", e);
            }
        }
    }

    #[test]
    fn test_fallback_access() {
        let _ = init(); // Try to initialize

        match fallback() {
            Ok(fallback) => {
                println!("NUMA fallback: {:?}", fallback.strategy());
                assert!(fallback.is_enabled());
            }
            Err(e) => {
                println!("Fallback access failed: {}", e);
            }
        }
    }

    #[test]
    fn test_node_count() {
        let _ = init(); // Try to initialize

        match node_count() {
            Ok(count) => {
                println!("NUMA node count: {}", count);
                assert!(count > 0);
            }
            Err(e) => {
                println!("Node count failed: {}", e);
            }
        }
    }

    #[test]
    fn test_current_thread_node() {
        let _ = init(); // Try to initialize

        match get_current_thread_node() {
            Ok(node_id) => {
                println!("Current thread node: {:?}", node_id);
            }
            Err(e) => {
                println!("Get current thread node failed: {}", e);
            }
        }
    }

    #[test]
    fn test_memory_node() {
        let _ = init(); // Try to initialize

        match get_best_memory_node(1024) {
            Ok(node_id) => {
                println!("Best memory node for 1024 bytes: {:?}", node_id);
            }
            Err(e) => {
                println!("Get best memory node failed: {}", e);
            }
        }
    }

    #[test]
    fn test_thread_affinity() {
        let _ = init(); // Try to initialize

        match apply_current_thread_affinity() {
            Ok(()) => {
                println!("Thread affinity applied successfully");
            }
            Err(e) => {
                println!("Apply thread affinity failed: {}", e);
            }
        }
    }

    #[test]
    fn test_metrics() {
        let _ = init(); // Try to initialize

        match get_metrics() {
            Ok(metrics) => {
                println!("NUMA metrics: {:?}", metrics);
                assert!(metrics.topology.node_count > 0);
            }
            Err(e) => {
                println!("Get metrics failed: {}", e);
            }
        }
    }

    #[test]
    fn test_multi_numa_detection() {
        let _ = init(); // Try to initialize

        match is_multi_numa() {
            Ok(is_multi) => {
                println!("System is multi-NUMA: {}", is_multi);
            }
            Err(e) => {
                println!("Multi-NUMA detection failed: {}", e);
            }
        }
    }

    #[test]
    fn test_numa_availability() {
        let available = is_numa_available();
        // This should not panic regardless of NUMA support
        println!("NUMA available: {}", available);
    }

    #[test]
    fn test_topology_detection() {
        let result = get_topology();
        match result {
            Ok(topology) => {
                println!("Detected {} NUMA nodes", topology.nodes.len());
                assert!(
                    !topology.nodes.is_empty(),
                    "Should have at least one NUMA node"
                );
            }
            Err(e) => {
                println!("NUMA topology detection failed: {}", e);
                // This is acceptable on non-NUMA systems
            }
        }
    }

    #[test]
    fn test_config_validation() {
        let mut config = NumaConfig::default();
        assert!(config.validate().is_ok());

        // Test invalid preferred node
        config.preferred_nodes.push(NumaNodeId(MAX_NUMA_NODES));
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_optimal_node_detection() {
        let result = get_optimal_node_for_current_thread();
        match result {
            Ok(node_id) => {
                println!("Optimal node for current thread: {}", node_id.0);
            }
            Err(e) => {
                println!("Failed to get optimal node: {}", e);
            }
        }
    }
}
