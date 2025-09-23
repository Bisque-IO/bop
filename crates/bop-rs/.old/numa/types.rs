//! Core NUMA types and identifiers

use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use super::error::NumaError;

/// NUMA node identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NumaNodeId(pub u32);

impl NumaNodeId {
    /// Create a new NUMA node ID
    pub const fn new(id: u32) -> Self {
        NumaNodeId(id)
    }

    /// Get the inner value
    pub const fn inner(self) -> u32 {
        self.0
    }

    /// Check if this is a valid node ID
    pub fn is_valid(self) -> bool {
        self.0 < crate::numa::MAX_NUMA_NODES
    }

    /// Get the next node ID (wraps around)
    pub const fn next(self) -> Self {
        NumaNodeId(self.0.wrapping_add(1))
    }

    /// Get the previous node ID (wraps around)
    pub const fn prev(self) -> Self {
        NumaNodeId(self.0.wrapping_sub(1))
    }
}

impl fmt::Display for NumaNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NUMA-{}", self.0)
    }
}

impl From<u32> for NumaNodeId {
    fn from(id: u32) -> Self {
        NumaNodeId(id)
    }
}

impl From<NumaNodeId> for u32 {
    fn from(id: NumaNodeId) -> Self {
        id.0
    }
}

impl FromStr for NumaNodeId {
    type Err = NumaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let id = s
            .parse::<u32>()
            .map_err(|e| NumaError::ParseError(format!("Invalid NUMA node ID: {}", e)))?;
        Ok(NumaNodeId(id))
    }
}

impl Default for NumaNodeId {
    fn default() -> Self {
        NumaNodeId(0)
    }
}

/// CPU identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CpuId(pub u32);

impl CpuId {
    /// Create a new CPU ID
    pub const fn new(id: u32) -> Self {
        CpuId(id)
    }

    /// Get the inner value
    pub const fn inner(self) -> u32 {
        self.0
    }

    /// Check if this is a valid CPU ID
    pub fn is_valid(self) -> bool {
        // This is a basic validation; actual max CPU count would be system-dependent
        self.0 < 4096
    }
}

impl fmt::Display for CpuId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CPU-{}", self.0)
    }
}

impl From<u32> for CpuId {
    fn from(id: u32) -> Self {
        CpuId(id)
    }
}

impl From<CpuId> for u32 {
    fn from(id: CpuId) -> Self {
        id.0
    }
}

impl Default for CpuId {
    fn default() -> Self {
        CpuId(0)
    }
}

/// Thread identifier wrapper for NUMA operations
#[derive(Debug, Clone, Copy)]
pub struct ThreadId {
    id: std::thread::ThreadId,
    hash: u64,
}

impl ThreadId {
    /// Create a new ThreadId from a std::thread::ThreadId
    pub fn new(id: std::thread::ThreadId) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        id.hash(&mut hasher);
        ThreadId {
            id,
            hash: hasher.finish(),
        }
    }

    /// Get the underlying std::thread::ThreadId
    pub fn inner(&self) -> &std::thread::ThreadId {
        &self.id
    }

    /// Get the hash value for this thread ID
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Get the current thread's ID
    pub fn current() -> Self {
        Self::new(std::thread::current().id())
    }
}

impl fmt::Display for ThreadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Thread-{:?}", self.id)
    }
}

impl From<std::thread::ThreadId> for ThreadId {
    fn from(id: std::thread::ThreadId) -> Self {
        ThreadId::new(id)
    }
}

impl Hash for ThreadId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl PartialEq for ThreadId {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ThreadId {}

/// NUMA detection method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DetectionMethod {
    /// Linux /sys filesystem detection
    SysFs,

    /// libnuma library detection
    LibNuma,

    /// Windows NUMA API detection
    WindowsApi,

    /// macOS/Unix fallback detection
    UnixFallback,

    /// Single-node fallback (non-NUMA systems)
    Fallback,

    /// NUMA explicitly disabled
    Disabled,
}

impl fmt::Display for DetectionMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DetectionMethod::SysFs => write!(f, "SysFs"),
            DetectionMethod::LibNuma => write!(f, "LibNuma"),
            DetectionMethod::WindowsApi => write!(f, "WindowsApi"),
            DetectionMethod::UnixFallback => write!(f, "UnixFallback"),
            DetectionMethod::Fallback => write!(f, "Fallback"),
            DetectionMethod::Disabled => write!(f, "Disabled"),
        }
    }
}

impl Default for DetectionMethod {
    fn default() -> Self {
        // Try to auto-detect the best method based on the platform
        #[cfg(target_os = "linux")]
        {
            DetectionMethod::SysFs
        }
        #[cfg(target_os = "windows")]
        {
            DetectionMethod::WindowsApi
        }
        #[cfg(target_os = "macos")]
        {
            DetectionMethod::UnixFallback
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            DetectionMethod::Fallback
        }
    }
}

impl FromStr for DetectionMethod {
    type Err = NumaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sysfs" => Ok(DetectionMethod::SysFs),
            "libnuma" => Ok(DetectionMethod::LibNuma),
            "windowsapi" | "windows_api" => Ok(DetectionMethod::WindowsApi),
            "unixfallback" | "unix_fallback" => Ok(DetectionMethod::UnixFallback),
            "fallback" => Ok(DetectionMethod::Fallback),
            "disabled" => Ok(DetectionMethod::Disabled),
            _ => Err(NumaError::ParseError(format!(
                "Invalid detection method: {}",
                s
            ))),
        }
    }
}

/// NUMA distance between nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NumaDistance(pub u32);

impl NumaDistance {
    /// Create a new NUMA distance
    pub const fn new(distance: u32) -> Self {
        NumaDistance(distance)
    }

    /// Get the inner value
    pub const fn inner(self) -> u32 {
        self.0
    }

    /// Check if this represents a local NUMA node (distance 10-20)
    pub fn is_local(self) -> bool {
        self.0 >= 10 && self.0 <= crate::numa::NUMA_LOCAL_DISTANCE_THRESHOLD
    }

    /// Check if this represents a remote but accessible NUMA node (distance 21-40)
    pub fn is_remote(self) -> bool {
        self.0 > crate::numa::NUMA_LOCAL_DISTANCE_THRESHOLD
            && self.0 <= crate::numa::NUMA_REMOTE_DISTANCE_THRESHOLD
    }

    /// Check if this represents a very distant NUMA node (distance > 40)
    pub fn is_distant(self) -> bool {
        self.0 > crate::numa::NUMA_REMOTE_DISTANCE_THRESHOLD
    }
}

impl fmt::Display for NumaDistance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u32> for NumaDistance {
    fn from(distance: u32) -> Self {
        NumaDistance(distance)
    }
}

impl From<NumaDistance> for u32 {
    fn from(distance: NumaDistance) -> Self {
        distance.0
    }
}

/// Memory size information for NUMA nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct MemorySize(pub usize);

impl MemorySize {
    /// Create a new memory size
    pub const fn new(size: usize) -> Self {
        MemorySize(size)
    }

    /// Get the inner value in bytes
    pub const fn inner(self) -> usize {
        self.0
    }

    /// Get the size in kilobytes
    pub const fn kilobytes(self) -> usize {
        self.0 / 1024
    }

    /// Get the size in megabytes
    pub const fn megabytes(self) -> usize {
        self.0 / (1024 * 1024)
    }

    /// Get the size in gigabytes
    pub const fn gigabytes(self) -> usize {
        self.0 / (1024 * 1024 * 1024)
    }

    /// Create from kilobytes
    pub const fn from_kilobytes(kb: usize) -> Self {
        MemorySize(kb * 1024)
    }

    /// Create from megabytes
    pub const fn from_megabytes(mb: usize) -> Self {
        MemorySize(mb * 1024 * 1024)
    }

    /// Create from gigabytes
    pub const fn from_gigabytes(gb: usize) -> Self {
        MemorySize(gb * 1024 * 1024 * 1024)
    }
}

impl fmt::Display for MemorySize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let gb = self.gigabytes();
        let mb = self.megabytes();
        let kb = self.kilobytes();

        if gb > 0 {
            write!(f, "{} GB", gb)
        } else if mb > 0 {
            write!(f, "{} MB", mb)
        } else if kb > 0 {
            write!(f, "{} KB", kb)
        } else {
            write!(f, "{} bytes", self.0)
        }
    }
}

impl From<usize> for MemorySize {
    fn from(size: usize) -> Self {
        MemorySize(size)
    }
}

impl From<MemorySize> for usize {
    fn from(size: MemorySize) -> Self {
        size.0
    }
}

/// NUMA node priority for optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NumaPriority(pub u8);

impl NumaPriority {
    /// Create a new priority (0-255, higher is better)
    pub const fn new(priority: u8) -> Self {
        NumaPriority(priority)
    }

    /// Get the inner value
    pub const fn inner(self) -> u8 {
        self.0
    }

    /// Highest priority
    pub const fn highest() -> Self {
        NumaPriority(255)
    }

    /// High priority
    pub const fn high() -> Self {
        NumaPriority(192)
    }

    /// Normal priority
    pub const fn normal() -> Self {
        NumaPriority(128)
    }

    /// Low priority
    pub const fn low() -> Self {
        NumaPriority(64)
    }

    /// Lowest priority
    pub const fn lowest() -> Self {
        NumaPriority(0)
    }
}

impl fmt::Display for NumaPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u8> for NumaPriority {
    fn from(priority: u8) -> Self {
        NumaPriority(priority)
    }
}

impl From<NumaPriority> for u8 {
    fn from(priority: NumaPriority) -> Self {
        priority.0
    }
}

impl Default for NumaPriority {
    fn default() -> Self {
        NumaPriority::normal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numa_node_id() {
        let node_id = NumaNodeId::new(1);
        assert_eq!(node_id.inner(), 1);
        assert!(node_id.is_valid());
        assert_eq!(node_id.next(), NumaNodeId(2));
        assert_eq!(node_id.prev(), NumaNodeId(0));
        assert_eq!(format!("{}", node_id), "NUMA-1");
    }

    #[test]
    fn test_cpu_id() {
        let cpu_id = CpuId::new(8);
        assert_eq!(cpu_id.inner(), 8);
        assert!(cpu_id.is_valid());
        assert_eq!(format!("{}", cpu_id), "CPU-8");
    }

    #[test]
    fn test_thread_id() {
        let thread_id = ThreadId::current();
        assert_eq!(thread_id.inner(), &std::thread::current().id());
        assert!(thread_id.hash() > 0);
    }

    #[test]
    fn test_detection_method() {
        let method = DetectionMethod::SysFs;
        assert_eq!(format!("{}", method), "SysFs");

        let default_method = DetectionMethod::default();
        // Should be appropriate for the current platform
        println!("Default detection method: {}", default_method);
    }

    #[test]
    fn test_numa_distance() {
        let local_distance = NumaDistance::new(10);
        assert!(local_distance.is_local());
        assert!(!local_distance.is_remote());
        assert!(!local_distance.is_distant());

        let remote_distance = NumaDistance::new(30);
        assert!(!remote_distance.is_local());
        assert!(remote_distance.is_remote());
        assert!(!remote_distance.is_distant());

        let distant_distance = NumaDistance::new(50);
        assert!(!distant_distance.is_local());
        assert!(!distant_distance.is_remote());
        assert!(distant_distance.is_distant());
    }

    #[test]
    fn test_memory_size() {
        let size = MemorySize::from_megabytes(1024);
        assert_eq!(size.inner(), 1024 * 1024 * 1024);
        assert_eq!(size.megabytes(), 1024);
        assert_eq!(size.gigabytes(), 1);
        assert_eq!(format!("{}", size), "1 GB");
    }

    #[test]
    fn test_numa_priority() {
        let priority = NumaPriority::high();
        assert_eq!(priority.inner(), 192);
        assert!(priority > NumaPriority::normal());
        assert!(priority < NumaPriority::highest());
    }

    #[test]
    fn test_conversions() {
        let node_id: NumaNodeId = 5u32.into();
        assert_eq!(node_id, NumaNodeId(5));

        let cpu_id: CpuId = 10u32.into();
        assert_eq!(cpu_id, CpuId(10));

        let distance: NumaDistance = 25u32.into();
        assert_eq!(distance, NumaDistance(25));

        let size: MemorySize = 2048usize.into();
        assert_eq!(size, MemorySize(2048));

        let priority: NumaPriority = 100u8.into();
        assert_eq!(priority, NumaPriority(100));
    }
}
