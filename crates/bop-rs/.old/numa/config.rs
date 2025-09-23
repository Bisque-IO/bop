//! NUMA configuration system with runtime support

use std::env;
use std::path::PathBuf;
use std::str::FromStr;

use super::error::{NumaError, NumaResult};
use super::types::{DetectionMethod, NumaNodeId};
use std::fmt;

/// NUMA configuration options
#[derive(Debug, Clone)]
pub struct NumaConfig {
    /// Whether NUMA optimizations are enabled
    pub enabled: bool,

    /// Method to use for NUMA detection
    pub detection_method: Option<DetectionMethod>,

    /// Preferred NUMA nodes for thread placement
    pub preferred_nodes: Vec<NumaNodeId>,

    /// Thread affinity policy
    pub affinity_policy: AffinityPolicy,

    /// Memory allocation policy
    pub memory_policy: MemoryPolicy,

    /// Fallback strategy for non-NUMA systems
    pub fallback_strategy: FallbackStrategy,

    /// Whether to auto-detect optimal settings
    pub auto_detect: bool,

    /// Configuration file path (if any)
    pub config_file: Option<PathBuf>,

    /// Environment variable prefix
    pub env_prefix: String,

    /// Whether to enable NUMA metrics collection
    pub enable_metrics: bool,

    /// Metrics collection interval in milliseconds
    pub metrics_interval_ms: u64,

    /// Whether to log cross-NUMA operations
    pub log_cross_numa: bool,

    /// Threshold for considering NUMA distance as local
    pub local_distance_threshold: u32,

    /// Threshold for considering NUMA distance as remote
    pub remote_distance_threshold: u32,
    pub simulated_node_count: Option<i32>,
}

impl Default for NumaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            detection_method: Some(DetectionMethod::default()),
            preferred_nodes: Vec::new(),
            affinity_policy: AffinityPolicy::default(),
            memory_policy: MemoryPolicy::default(),
            fallback_strategy: FallbackStrategy::default(),
            auto_detect: true,
            config_file: None,
            env_prefix: "BOP_NUMA".to_string(),
            enable_metrics: true,
            metrics_interval_ms: 1000,
            log_cross_numa: false,
            local_distance_threshold: 20,
            remote_distance_threshold: 40,
            simulated_node_count: None,
        }
    }
}

impl NumaConfig {
    /// Create a new default configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a configuration with NUMA disabled
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            detection_method: Some(DetectionMethod::Disabled),
            ..Self::default()
        }
    }

    /// Enable NUMA optimizations
    pub fn enable(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// Disable NUMA optimizations
    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self.detection_method = Some(DetectionMethod::Disabled);
        self
    }

    /// Set the detection method
    pub fn with_detection_method(mut self, method: DetectionMethod) -> Self {
        self.detection_method = Some(method);
        self
    }

    /// Add a preferred NUMA node
    pub fn with_preferred_node(mut self, node_id: NumaNodeId) -> Self {
        self.preferred_nodes.push(node_id);
        self
    }

    /// Set preferred NUMA nodes
    pub fn with_preferred_nodes(mut self, nodes: Vec<NumaNodeId>) -> Self {
        self.preferred_nodes = nodes;
        self
    }

    /// Set the affinity policy
    pub fn with_affinity_policy(mut self, policy: AffinityPolicy) -> Self {
        self.affinity_policy = policy;
        self
    }

    /// Set the memory policy
    pub fn with_memory_policy(mut self, policy: MemoryPolicy) -> Self {
        self.memory_policy = policy;
        self
    }

    /// Set the fallback strategy
    pub fn with_fallback_strategy(mut self, strategy: FallbackStrategy) -> Self {
        self.fallback_strategy = strategy;
        self
    }

    /// Enable or disable auto-detection
    pub fn with_auto_detect(mut self, enabled: bool) -> Self {
        self.auto_detect = enabled;
        self
    }

    /// Set the configuration file path
    pub fn with_config_file(mut self, path: PathBuf) -> Self {
        self.config_file = Some(path);
        self
    }

    /// Set the environment variable prefix
    pub fn with_env_prefix(mut self, prefix: String) -> Self {
        self.env_prefix = prefix;
        self
    }

    /// Enable or disable metrics collection
    pub fn with_metrics(mut self, enabled: bool) -> Self {
        self.enable_metrics = enabled;
        self
    }

    /// Set the metrics collection interval
    pub fn with_metrics_interval(mut self, interval_ms: u64) -> Self {
        self.metrics_interval_ms = interval_ms;
        self
    }

    /// Enable or disable cross-NUMA operation logging
    pub fn with_cross_numa_logging(mut self, enabled: bool) -> Self {
        self.log_cross_numa = enabled;
        self
    }

    /// Set distance thresholds
    pub fn with_distance_thresholds(mut self, local: u32, remote: u32) -> Self {
        self.local_distance_threshold = local;
        self.remote_distance_threshold = remote;
        self
    }

    /// Load configuration from environment variables
    pub fn load_from_env(&mut self) -> NumaResult<()> {
        // Check if NUMA is enabled
        if let Ok(enabled_str) = env::var(format!("{}_ENABLED", self.env_prefix)) {
            self.enabled = enabled_str.parse().unwrap_or(true);
        }

        // Check detection method
        if let Ok(method_str) = env::var(format!("{}_DETECTION_METHOD", self.env_prefix)) {
            self.detection_method = Some(method_str.parse()?);
        }

        // Check preferred nodes
        if let Ok(nodes_str) = env::var(format!("{}_PREFERRED_NODES", self.env_prefix)) {
            self.preferred_nodes = nodes_str
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
        }

        // Check affinity policy
        if let Ok(policy_str) = env::var(format!("{}_AFFINITY_POLICY", self.env_prefix)) {
            self.affinity_policy = policy_str.parse()?;
        }

        // Check memory policy
        if let Ok(policy_str) = env::var(format!("{}_MEMORY_POLICY", self.env_prefix)) {
            self.memory_policy = policy_str.parse()?;
        }

        // Check fallback strategy
        if let Ok(strategy_str) = env::var(format!("{}_FALLBACK_STRATEGY", self.env_prefix)) {
            self.fallback_strategy = strategy_str.parse()?;
        }

        // Check auto-detection
        if let Ok(auto_detect_str) = env::var(format!("{}_AUTO_DETECT", self.env_prefix)) {
            self.auto_detect = auto_detect_str.parse().unwrap_or(true);
        }

        // Check metrics
        if let Ok(metrics_str) = env::var(format!("{}_ENABLE_METRICS", self.env_prefix)) {
            self.enable_metrics = metrics_str.parse().unwrap_or(true);
        }

        // Check metrics interval
        if let Ok(interval_str) = env::var(format!("{}_METRICS_INTERVAL_MS", self.env_prefix)) {
            self.metrics_interval_ms = interval_str.parse().unwrap_or(1000);
        }

        // Check cross-NUMA logging
        if let Ok(log_str) = env::var(format!("{}_LOG_CROSS_NUMA", self.env_prefix)) {
            self.log_cross_numa = log_str.parse().unwrap_or(false);
        }

        Ok(())
    }

    /// Validate the configuration
    pub fn validate(&self) -> NumaResult<()> {
        if self.enabled && self.detection_method.is_none() {
            return Err(NumaError::InvalidConfig(
                "Detection method required when NUMA is enabled".to_string(),
            ));
        }

        if let Some(method) = self.detection_method {
            if method == DetectionMethod::Disabled && self.enabled {
                return Err(NumaError::InvalidConfig(
                    "NUMA cannot be enabled with Disabled detection method".to_string(),
                ));
            }
        }

        for node_id in &self.preferred_nodes {
            if !node_id.is_valid() {
                return Err(NumaError::InvalidConfig(format!(
                    "Invalid NUMA node ID: {}",
                    node_id.0
                )));
            }
        }

        if self.local_distance_threshold >= self.remote_distance_threshold {
            return Err(NumaError::InvalidConfig(
                "Local distance threshold must be less than remote distance threshold".to_string(),
            ));
        }

        if self.metrics_interval_ms == 0 {
            return Err(NumaError::InvalidConfig(
                "Metrics interval must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }

    /// Check if a specific NUMA node is preferred
    pub fn is_preferred_node(&self, node_id: NumaNodeId) -> bool {
        self.preferred_nodes.contains(&node_id)
    }

    /// Get the first preferred NUMA node
    pub fn first_preferred_node(&self) -> Option<NumaNodeId> {
        self.preferred_nodes.first().copied()
    }

    /// Check if auto-detection is enabled
    pub fn is_auto_detect_enabled(&self) -> bool {
        self.auto_detect && self.enabled
    }
}

/// Thread affinity policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AffinityPolicy {
    /// Strictly pin threads to specific NUMA nodes
    Strict,

    /// Prefer NUMA nodes but allow migration if needed
    Preferred,

    /// Distribute threads evenly across NUMA nodes
    Balanced,

    /// Use custom placement rules
    Custom,
}

impl Default for AffinityPolicy {
    fn default() -> Self {
        AffinityPolicy::Preferred
    }
}

impl fmt::Display for AffinityPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AffinityPolicy::Strict => write!(f, "Strict"),
            AffinityPolicy::Preferred => write!(f, "Preferred"),
            AffinityPolicy::Balanced => write!(f, "Balanced"),
            AffinityPolicy::Custom => write!(f, "Custom"),
        }
    }
}

impl FromStr for AffinityPolicy {
    type Err = NumaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "strict" => Ok(AffinityPolicy::Strict),
            "preferred" => Ok(AffinityPolicy::Preferred),
            "balanced" => Ok(AffinityPolicy::Balanced),
            "custom" => Ok(AffinityPolicy::Custom),
            _ => Err(NumaError::ParseError(format!(
                "Invalid affinity policy: {}",
                s
            ))),
        }
    }
}

/// Memory allocation policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPolicy {
    /// Allocate memory on the local NUMA node
    Local,

    /// Interleave allocations across all NUMA nodes
    Interleave,

    /// Prefer specific NUMA nodes for allocation
    Preferred,

    /// Bind allocations to specific NUMA nodes
    Bind,
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        MemoryPolicy::Local
    }
}

impl fmt::Display for MemoryPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemoryPolicy::Local => write!(f, "Local"),
            MemoryPolicy::Interleave => write!(f, "Interleave"),
            MemoryPolicy::Preferred => write!(f, "Preferred"),
            MemoryPolicy::Bind => write!(f, "Bind"),
        }
    }
}

impl FromStr for MemoryPolicy {
    type Err = NumaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" => Ok(MemoryPolicy::Local),
            "interleave" => Ok(MemoryPolicy::Interleave),
            "preferred" => Ok(MemoryPolicy::Preferred),
            "bind" => Ok(MemoryPolicy::Bind),
            _ => Err(NumaError::ParseError(format!(
                "Invalid memory policy: {}",
                s
            ))),
        }
    }
}

/// Fallback strategy for non-NUMA systems
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackStrategy {
    /// Treat the system as a single NUMA node
    SingleNode,

    /// Simulate a NUMA topology for testing purposes
    Simulated,

    /// Disable all NUMA optimizations
    Disabled,

    /// Use NUMA where available, fallback to single-node elsewhere
    Hybrid,
}

impl Default for FallbackStrategy {
    fn default() -> Self {
        FallbackStrategy::Hybrid
    }
}

impl fmt::Display for FallbackStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FallbackStrategy::SingleNode => write!(f, "SingleNode"),
            FallbackStrategy::Simulated => write!(f, "Simulated"),
            FallbackStrategy::Disabled => write!(f, "Disabled"),
            FallbackStrategy::Hybrid => write!(f, "Hybrid"),
        }
    }
}

impl FromStr for FallbackStrategy {
    type Err = NumaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "single" | "single_node" => Ok(FallbackStrategy::SingleNode),
            "simulated" => Ok(FallbackStrategy::Simulated),
            "disabled" => Ok(FallbackStrategy::Disabled),
            "hybrid" => Ok(FallbackStrategy::Hybrid),
            _ => Err(NumaError::ParseError(format!(
                "Invalid fallback strategy: {}",
                s
            ))),
        }
    }
}

/// Configuration loader for loading from various sources
pub struct NumaConfigLoader {
    env_prefix: String,
    config_file: Option<PathBuf>,
}

impl NumaConfigLoader {
    /// Create a new configuration loader
    pub fn new() -> Self {
        Self {
            env_prefix: "BOP_NUMA".to_string(),
            config_file: None,
        }
    }

    /// Set the environment variable prefix
    pub fn with_env_prefix(mut self, prefix: String) -> Self {
        self.env_prefix = prefix;
        self
    }

    /// Set the configuration file path
    pub fn with_config_file(mut self, path: PathBuf) -> Self {
        self.config_file = Some(path);
        self
    }

    /// Load configuration from all sources
    pub fn load(&self) -> NumaResult<NumaConfig> {
        let mut config = NumaConfig::default();
        config.env_prefix = self.env_prefix.clone();
        config.config_file = self.config_file.clone();

        // Load from environment variables first
        config.load_from_env()?;

        // Load from config file if specified
        if let Some(ref file_path) = self.config_file {
            self.load_from_file(file_path, &mut config)?;
        }

        // Validate the final configuration
        config.validate()?;

        Ok(config)
    }

    /// Load configuration from a file
    fn load_from_file(&self, file_path: &PathBuf, config: &mut NumaConfig) -> NumaResult<()> {
        use std::fs::File;
        use std::io::Read;

        let mut file = File::open(file_path)
            .map_err(|e| NumaError::IoError(format!("Failed to open config file: {}", e)))?;

        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(|e| NumaError::IoError(format!("Failed to read config file: {}", e)))?;

        // Parse TOML format (basic parsing for now)
        self.parse_toml_config(&contents, config)?;

        Ok(())
    }

    /// Parse TOML configuration (simplified parser)
    fn parse_toml_config(&self, contents: &str, config: &mut NumaConfig) -> NumaResult<()> {
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = self.parse_toml_line(line) {
                match key.as_str() {
                    "enabled" => {
                        if let Ok(enabled) = value.parse::<bool>() {
                            config.enabled = enabled;
                        }
                    }
                    "detection_method" => {
                        if let Ok(method) = value.parse::<DetectionMethod>() {
                            config.detection_method = Some(method);
                        }
                    }
                    "affinity_policy" => {
                        if let Ok(policy) = value.parse::<AffinityPolicy>() {
                            config.affinity_policy = policy;
                        }
                    }
                    "memory_policy" => {
                        if let Ok(policy) = value.parse::<MemoryPolicy>() {
                            config.memory_policy = policy;
                        }
                    }
                    "fallback_strategy" => {
                        if let Ok(strategy) = value.parse::<FallbackStrategy>() {
                            config.fallback_strategy = strategy;
                        }
                    }
                    "auto_detect" => {
                        if let Ok(auto_detect) = value.parse::<bool>() {
                            config.auto_detect = auto_detect;
                        }
                    }
                    "enable_metrics" => {
                        if let Ok(enable_metrics) = value.parse::<bool>() {
                            config.enable_metrics = enable_metrics;
                        }
                    }
                    "metrics_interval_ms" => {
                        if let Ok(interval) = value.parse::<u64>() {
                            config.metrics_interval_ms = interval;
                        }
                    }
                    "log_cross_numa" => {
                        if let Ok(log_cross_numa) = value.parse::<bool>() {
                            config.log_cross_numa = log_cross_numa;
                        }
                    }
                    _ => {
                        // Unknown key, ignore
                    }
                }
            }
        }

        Ok(())
    }

    /// Parse a single TOML line
    fn parse_toml_line(&self, line: &str) -> Option<(String, String)> {
        let parts: Vec<&str> = line.splitn(2, '=').collect();
        if parts.len() != 2 {
            return None;
        }

        let key = parts[0].trim().trim_matches('"');
        let value = parts[1].trim().trim_matches('"');

        Some((key.to_string(), value.to_string()))
    }
}

impl Default for NumaConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = NumaConfig::default();
        assert!(config.enabled);
        assert!(config.detection_method.is_some());
        assert!(config.preferred_nodes.is_empty());
        assert_eq!(config.affinity_policy, AffinityPolicy::Preferred);
        assert_eq!(config.memory_policy, MemoryPolicy::Local);
        assert_eq!(config.fallback_strategy, FallbackStrategy::Hybrid);
        assert!(config.auto_detect);
    }

    #[test]
    fn test_config_builder() {
        let config = NumaConfig::new()
            .disable()
            .with_detection_method(DetectionMethod::Fallback)
            .with_preferred_node(NumaNodeId(0))
            .with_affinity_policy(AffinityPolicy::Strict)
            .with_memory_policy(MemoryPolicy::Interleave)
            .with_fallback_strategy(FallbackStrategy::SingleNode)
            .with_auto_detect(false)
            .with_metrics(false)
            .with_cross_numa_logging(true);

        assert!(!config.enabled);
        assert_eq!(config.detection_method, Some(DetectionMethod::Fallback));
        assert_eq!(config.preferred_nodes, vec![NumaNodeId(0)]);
        assert_eq!(config.affinity_policy, AffinityPolicy::Strict);
        assert_eq!(config.memory_policy, MemoryPolicy::Interleave);
        assert_eq!(config.fallback_strategy, FallbackStrategy::SingleNode);
        assert!(!config.auto_detect);
        assert!(!config.enable_metrics);
        assert!(config.log_cross_numa);
    }

    #[test]
    fn test_config_validation() {
        let mut config = NumaConfig::default();
        assert!(config.validate().is_ok());

        // Test invalid preferred node
        config.preferred_nodes.push(NumaNodeId(999));
        assert!(config.validate().is_err());

        // Test invalid distance thresholds
        let mut config = NumaConfig::default();
        config.local_distance_threshold = 50;
        config.remote_distance_threshold = 40;
        assert!(config.validate().is_err());

        // Test invalid metrics interval
        let mut config = NumaConfig::default();
        config.metrics_interval_ms = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_policy_parsing() {
        assert_eq!(
            "strict".parse::<AffinityPolicy>().unwrap(),
            AffinityPolicy::Strict
        );
        assert_eq!(
            "preferred".parse::<AffinityPolicy>().unwrap(),
            AffinityPolicy::Preferred
        );
        assert_eq!(
            "local".parse::<MemoryPolicy>().unwrap(),
            MemoryPolicy::Local
        );
        assert_eq!(
            "interleave".parse::<MemoryPolicy>().unwrap(),
            MemoryPolicy::Interleave
        );
        assert_eq!(
            "single_node".parse::<FallbackStrategy>().unwrap(),
            FallbackStrategy::SingleNode
        );
        assert_eq!(
            "hybrid".parse::<FallbackStrategy>().unwrap(),
            FallbackStrategy::Hybrid
        );

        assert!("invalid".parse::<AffinityPolicy>().is_err());
    }

    #[test]
    fn test_config_loader() {
        let loader = NumaConfigLoader::new().with_env_prefix("TEST_NUMA".to_string());

        let config = loader.load().unwrap();
        assert_eq!(config.env_prefix, "TEST_NUMA");
    }

    #[test]
    fn test_toml_parsing() {
        let loader = NumaConfigLoader::new();
        let toml_content = r#"
enabled = true
detection_method = "SysFs"
affinity_policy = "Strict"
memory_policy = "Local"
auto_detect = true
enable_metrics = true
metrics_interval_ms = 2000
log_cross_numa = true
"#;

        let mut config = NumaConfig::default();
        loader.parse_toml_config(toml_content, &mut config).unwrap();

        assert!(config.enabled);
        assert_eq!(config.detection_method, Some(DetectionMethod::SysFs));
        assert_eq!(config.affinity_policy, AffinityPolicy::Strict);
        assert_eq!(config.memory_policy, MemoryPolicy::Local);
        assert!(config.auto_detect);
        assert!(config.enable_metrics);
        assert_eq!(config.metrics_interval_ms, 2000);
        assert!(config.log_cross_numa);
    }
}
