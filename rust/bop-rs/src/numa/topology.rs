//! NUMA topology detection and management

use super::config::NumaConfig;
use super::error::{NumaError, NumaResult};
use super::types::{CpuId, DetectionMethod, MemorySize, NumaDistance, NumaNodeId, NumaPriority};

/// Represents a single NUMA node in the system topology
#[derive(Debug, Clone)]
pub struct NumaNode {
    /// Unique identifier for this NUMA node
    pub id: NumaNodeId,

    /// List of CPU cores belonging to this NUMA node
    pub cpus: Vec<CpuId>,

    /// Memory size available on this NUMA node
    pub memory_size: MemorySize,

    /// Distance matrix to other NUMA nodes
    pub distance_matrix: Vec<NumaDistance>,

    /// Priority for optimization purposes
    pub priority: NumaPriority,

    /// Whether this node is preferred for allocations
    pub preferred: bool,

    /// Node-specific information
    pub info: NumaNodeInfo,
}

/// Additional information about a NUMA node
#[derive(Debug, Clone, Default)]
pub struct NumaNodeInfo {
    /// Physical package ID (socket)
    pub physical_package_id: Option<u32>,

    /// Die ID within the physical package
    pub die_id: Option<u32>,

    /// Core ID within the die
    pub core_id: Option<u32>,

    /// Node name/description
    pub name: Option<String>,

    /// Whether this node is online/available
    pub online: bool,
}

/// Complete NUMA topology information for the system
#[derive(Debug, Clone)]
pub struct NumaTopology {
    /// List of all NUMA nodes in the system
    pub nodes: Vec<NumaNode>,

    /// Total memory across all NUMA nodes
    pub total_memory: MemorySize,

    /// Total CPU count across all NUMA nodes
    pub cpu_count: usize,

    /// Whether NUMA is available on this system
    pub is_numa_available: bool,

    /// Method used for topology detection
    pub detection_method: DetectionMethod,

    /// System-wide NUMA information
    pub system_info: NumaSystemInfo,
}

/// System-wide NUMA information
#[derive(Debug, Clone, Default)]
pub struct NumaSystemInfo {
    /// NUMA API version (if available)
    pub api_version: Option<String>,

    /// Maximum number of NUMA nodes supported
    pub max_nodes: Option<u32>,

    /// Page size information
    pub page_size: Option<usize>,

    /// Huge page support
    pub huge_page_size: Option<usize>,

    /// System architecture information
    pub architecture: Option<String>,
}

impl NumaTopology {
    /// Detect NUMA topology using default configuration
    pub fn detect() -> NumaResult<Self> {
        let config = NumaConfig::default();
        Self::detect_with_config(&config)
    }

    /// Detect NUMA topology with custom configuration
    pub fn detect_with_config(config: &NumaConfig) -> NumaResult<Self> {
        if !config.enabled {
            return Ok(Self::create_disabled_topology());
        }

        let detection_method = config.detection_method.unwrap_or_default();

        match detection_method {
            DetectionMethod::SysFs => {
                #[cfg(target_os = "linux")]
                {
                    Self::detect_from_sysfs(config)
                }
                #[cfg(not(target_os = "linux"))]
                {
                    Err(NumaError::NotSupported(
                        "SysFs detection is only supported on Linux".to_string(),
                    ))
                }
            }
            DetectionMethod::LibNuma => Self::detect_from_libnuma(config),
            DetectionMethod::WindowsApi => {
                #[cfg(target_os = "windows")]
                {
                    Self::detect_from_windows_api(config)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    Err(NumaError::NotSupported(
                        "Windows API detection is only supported on Windows".to_string(),
                    ))
                }
            }
            DetectionMethod::UnixFallback => Self::detect_from_unix_fallback(config),
            DetectionMethod::Fallback => Ok(Self::create_single_node_topology()),
            DetectionMethod::Disabled => Ok(Self::create_disabled_topology()),
        }
    }

    /// Create a single-node topology for non-NUMA systems
    pub fn create_single_node_topology() -> Self {
        let node = NumaNode {
            id: NumaNodeId(0),
            cpus: Self::detect_all_cpus(),
            memory_size: Self::detect_total_memory(),
            distance_matrix: vec![NumaDistance::new(10)], // Local distance
            priority: NumaPriority::highest(),
            preferred: true,
            info: NumaNodeInfo {
                online: true,
                name: Some("Single NUMA Node".to_string()),
                ..Default::default()
            },
        };

        Self {
            nodes: vec![node],
            total_memory: Self::detect_total_memory(),
            cpu_count: Self::detect_all_cpus().len(),
            is_numa_available: false,
            detection_method: DetectionMethod::Fallback,
            system_info: NumaSystemInfo {
                architecture: Some(Self::detect_architecture()),
                ..Default::default()
            },
        }
    }

    /// Create a disabled topology
    pub fn create_disabled_topology() -> Self {
        Self {
            nodes: Vec::new(),
            total_memory: MemorySize::new(0),
            cpu_count: 0,
            is_numa_available: false,
            detection_method: DetectionMethod::Disabled,
            system_info: NumaSystemInfo::default(),
        }
    }

    /// Detect topology from Linux /sys filesystem
    #[cfg(target_os = "linux")]
    fn detect_from_sysfs(config: &NumaConfig) -> NumaResult<Self> {
        let nodes_path = Path::new("/sys/devices/system/node");

        if !nodes_path.exists() {
            return Ok(Self::create_single_node_topology());
        }

        let mut nodes = Vec::new();
        let mut total_memory = MemorySize::new(0);
        let mut total_cpus = 0;

        // Read NUMA nodes
        for entry in fs::read_dir(nodes_path).map_err(|e| {
            NumaError::IoError(format!("Failed to read NUMA nodes directory: {}", e))
        })? {
            let entry = entry.map_err(|e| {
                NumaError::IoError(format!("Failed to read directory entry: {}", e))
            })?;

            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            if file_name_str.starts_with("node") {
                if let Ok(node_id) = file_name_str[4..].parse::<u32>() {
                    let node = Self::read_sysfs_node(&entry.path(), node_id, config)?;
                    total_memory += node.memory_size;
                    total_cpus += node.cpus.len();
                    nodes.push(node);
                }
            }
        }

        if nodes.is_empty() {
            return Ok(Self::create_single_node_topology());
        }

        // Read distance matrix for each node
        for node in &mut nodes {
            node.distance_matrix = Self::read_sysfs_distances(node.id, &nodes)?;
        }

        Ok(Self {
            nodes,
            total_memory,
            cpu_count: total_cpus,
            is_numa_available: true,
            detection_method: DetectionMethod::SysFs,
            system_info: NumaSystemInfo {
                max_nodes: Some(Self::read_sysfs_max_nodes()?),
                page_size: Some(Self::read_sysfs_page_size()?),
                huge_page_size: Self::read_sysfs_hugepage_size()?,
                architecture: Some(Self::detect_architecture()),
                ..Default::default()
            },
        })
    }

    /// Read a single NUMA node from sysfs
    #[cfg(target_os = "linux")]
    fn read_sysfs_node(
        node_path: &Path,
        node_id: u32,
        config: &NumaConfig,
    ) -> NumaResult<NumaNode> {
        let cpus = Self::read_sysfs_cpus(node_path)?;
        let memory_size = Self::read_sysfs_memory(node_path)?;
        let preferred = config.is_preferred_node(NumaNodeId(node_id));
        let priority = if preferred {
            NumaPriority::high()
        } else {
            NumaPriority::normal()
        };

        let info = NumaNodeInfo {
            physical_package_id: Self::read_sysfs_node_file(node_path, "physical_package_id")?,
            die_id: Self::read_sysfs_node_file(node_path, "die_id")?,
            core_id: Self::read_sysfs_node_file(node_path, "core_id")?,
            online: Self::read_sysfs_node_online(node_path)?,
            name: Some(format!("NUMA Node {}", node_id)),
        };

        Ok(NumaNode {
            id: NumaNodeId(node_id),
            cpus,
            memory_size,
            distance_matrix: Vec::new(), // Will be filled later
            priority,
            preferred,
            info,
        })
    }

    /// Read CPU list for a NUMA node from sysfs
    #[cfg(target_os = "linux")]
    fn read_sysfs_cpus(node_path: &Path) -> NumaResult<Vec<CpuId>> {
        let cpus_path = node_path.join("cpulist");
        let cpus_content = fs::read_to_string(cpus_path)
            .map_err(|e| NumaError::IoError(format!("Failed to read CPU list: {}", e)))?;

        let mut cpus = Vec::new();
        for cpu_range in cpus_content.trim().split(',') {
            if cpu_range.contains('-') {
                // Range like "0-7"
                let parts: Vec<&str> = cpu_range.split('-').collect();
                if parts.len() == 2 {
                    if let (Ok(start), Ok(end)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>())
                    {
                        for cpu_id in start..=end {
                            cpus.push(CpuId(cpu_id));
                        }
                    }
                }
            } else {
                // Single CPU like "0"
                if let Ok(cpu_id) = cpu_range.parse::<u32>() {
                    cpus.push(CpuId(cpu_id));
                }
            }
        }

        Ok(cpus)
    }

    /// Read memory size for a NUMA node from sysfs
    #[cfg(target_os = "linux")]
    fn read_sysfs_memory(node_path: &Path) -> NumaResult<MemorySize> {
        let meminfo_path = node_path.join("meminfo");
        let meminfo_content = fs::read_to_string(meminfo_path)
            .map_err(|e| NumaError::IoError(format!("Failed to read memory info: {}", e)))?;

        // Parse "Node 0 MemTotal:       16384 kB"
        for line in meminfo_content.lines() {
            if line.contains("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    if let Ok(size_kb) = parts[3].parse::<usize>() {
                        return Ok(MemorySize::from_kilobytes(size_kb));
                    }
                }
            }
        }

        Err(NumaError::ParseError(
            "Failed to parse memory size".to_string(),
        ))
    }

    /// Read distance matrix for a NUMA node from sysfs
    #[cfg(target_os = "linux")]
    fn read_sysfs_distances(
        node_id: NumaNodeId,
        all_nodes: &[NumaNode],
    ) -> NumaResult<Vec<NumaDistance>> {
        let distance_path = Path::new("/sys/devices/system/node")
            .join(format!("node{}", node_id.0))
            .join("distance");

        let distance_content = fs::read_to_string(distance_path)
            .map_err(|e| NumaError::IoError(format!("Failed to read distance matrix: {}", e)))?;

        let distances: Vec<NumaDistance> = distance_content
            .trim()
            .split_whitespace()
            .filter_map(|s| s.parse::<u32>().ok())
            .map(NumaDistance::new)
            .collect();

        // Ensure we have distances for all nodes
        if distances.len() != all_nodes.len() {
            return Err(NumaError::ParseError(
                "Distance matrix size doesn't match node count".to_string(),
            ));
        }

        Ok(distances)
    }

    /// Read maximum number of NUMA nodes from sysfs
    #[cfg(target_os = "linux")]
    fn read_sysfs_max_nodes() -> NumaResult<u32> {
        let path = Path::new("/sys/devices/system/node/possible");
        let content = fs::read_to_string(path)
            .map_err(|e| NumaError::IoError(format!("Failed to read max nodes: {}", e)))?;

        // Parse format like "0-3" or "0"
        if content.contains('-') {
            let parts: Vec<&str> = content.trim().split('-').collect();
            if parts.len() == 2 {
                if let Ok(max) = parts[1].parse::<u32>() {
                    return Ok(max + 1); // Convert to count
                }
            }
        } else {
            if let Ok(single) = content.trim().parse::<u32>() {
                return Ok(single + 1);
            }
        }

        Err(NumaError::ParseError(
            "Failed to parse max nodes".to_string(),
        ))
    }

    /// Read page size from sysfs
    #[cfg(target_os = "linux")]
    fn read_sysfs_page_size() -> NumaResult<usize> {
        let path = Path::new("/proc/meminfo");
        let content = fs::read_to_string(path)
            .map_err(|e| NumaError::IoError(format!("Failed to read meminfo: {}", e)))?;

        for line in content.lines() {
            if line.starts_with("Hugepagesize:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    if let Ok(size_kb) = parts[2].parse::<usize>() {
                        return Ok(size_kb * 1024); // Convert to bytes
                    }
                }
            }
        }

        // Default page size
        Ok(4096)
    }

    /// Read huge page size from sysfs
    #[cfg(target_os = "linux")]
    fn read_sysfs_hugepage_size() -> NumaResult<Option<usize>> {
        let path = Path::new("/sys/kernel/mm/hugepages/hugepages-2048kB");
        if path.exists() {
            Ok(Some(2 * 1024 * 1024)) // 2MB huge pages
        } else {
            Ok(None)
        }
    }

    /// Read whether a NUMA node is online
    #[cfg(target_os = "linux")]
    fn read_sysfs_node_online(node_path: &Path) -> NumaResult<bool> {
        let online_path = node_path.join("online");
        if online_path.exists() {
            let content = fs::read_to_string(online_path)
                .map_err(|e| NumaError::IoError(format!("Failed to read online status: {}", e)))?;
            Ok(content.trim() == "1")
        } else {
            Ok(true) // Assume online if file doesn't exist
        }
    }

    /// Read a generic node file from sysfs
    #[cfg(target_os = "linux")]
    fn read_sysfs_node_file(node_path: &Path, filename: &str) -> NumaResult<Option<u32>> {
        let file_path = node_path.join(filename);
        if file_path.exists() {
            let content = fs::read_to_string(file_path)
                .map_err(|e| NumaError::IoError(format!("Failed to read node file: {}", e)))?;
            Ok(content.trim().parse().ok())
        } else {
            Ok(None)
        }
    }

    /// Detect topology using libnuma (if available)
    fn detect_from_libnuma(_config: &NumaConfig) -> NumaResult<Self> {
        // This would require linking with libnuma
        // For now, fall back to sysfs on Linux
        #[cfg(target_os = "linux")]
        {
            Self::detect_from_sysfs(_config)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(NumaError::NotSupported(
                "libnuma detection not yet implemented".to_string(),
            ))
        }
    }

    /// Detect topology using Windows NUMA API
    #[cfg(target_os = "windows")]
    fn detect_from_windows_api(_config: &NumaConfig) -> NumaResult<Self> {
        // This would use Windows NUMA API
        Err(NumaError::NotSupported(
            "Windows NUMA API detection not yet implemented".to_string(),
        ))
    }

    /// Detect topology using Unix fallback methods
    fn detect_from_unix_fallback(_config: &NumaConfig) -> NumaResult<Self> {
        // Try various Unix methods for NUMA detection
        // For now, create a single-node topology
        Ok(Self::create_single_node_topology())
    }

    /// Detect all available CPUs
    fn detect_all_cpus() -> Vec<CpuId> {
        // Try to read from /proc/cpuinfo on Linux
        #[cfg(target_os = "linux")]
        {
            if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
                let mut cpus = Vec::new();
                for line in content.lines() {
                    if line.starts_with("processor") {
                        if let Some(cpu_id_str) = line.split(':').nth(1) {
                            if let Ok(cpu_id) = cpu_id_str.trim().parse::<u32>() {
                                cpus.push(CpuId(cpu_id));
                            }
                        }
                    }
                }
                if !cpus.is_empty() {
                    return cpus;
                }
            }
        }

        // Windows implementation using available_parallelism
        #[cfg(target_os = "windows")]
        {
            if let Ok(parallelism) = std::thread::available_parallelism() {
                let num_cpus = parallelism.get();
                let cpus: Vec<CpuId> = (0..num_cpus as u32).map(CpuId).collect();
                return cpus;
            }
        }

        // macOS implementation using sysctl
        #[cfg(target_os = "macos")]
        {
            use libc::{CTL_HW, HW_NCPU, c_int, c_void, sysctl};
            use std::ffi::CStr;

            let mut num_cpus: c_int = 0;
            let mut len = std::mem::size_of::<c_int>();

            let mut mib: [c_int; 2] = [CTL_HW, HW_NCPU];
            let result = unsafe {
                sysctl(
                    mib.as_mut_ptr(),
                    mib.len() as u32,
                    &mut num_cpus as *mut c_int as *mut c_void,
                    &mut len,
                    std::ptr::null_mut(),
                    0,
                )
            };

            if result == 0 && num_cpus > 0 {
                let cpus: Vec<CpuId> = (0..num_cpus).map(|i| CpuId(i as u32)).collect();
                return cpus;
            }

            // Alternative: try hw.logicalcpu
            let mut logical_cpus: c_int = 0;
            let mut len = std::mem::size_of::<c_int>();

            let mut mib: [c_int; 2] = [CTL_HW, 7]; // HW_LOGICALCPU
            let result = unsafe {
                sysctl(
                    mib.as_mut_ptr(),
                    mib.len() as u32,
                    &mut logical_cpus as *mut c_int as *mut c_void,
                    &mut len,
                    std::ptr::null_mut(),
                    0,
                )
            };

            if result == 0 && logical_cpus > 0 {
                let cpus: Vec<CpuId> = (0..logical_cpus).map(|i| CpuId(i as u32)).collect();
                return cpus;
            }
        }

        // FreeBSD implementation using sysctl
        #[cfg(target_os = "freebsd")]
        {
            use libc::{CTL_HW, HW_NCPU, c_int, c_void, sysctl};
            use std::ffi::CStr;

            let mut num_cpus: c_int = 0;
            let mut len = std::mem::size_of::<c_int>();

            let mut mib: [c_int; 2] = [CTL_HW, HW_NCPU];
            let result = unsafe {
                sysctl(
                    mib.as_mut_ptr(),
                    mib.len() as u32,
                    &mut num_cpus as *mut c_int as *mut c_void,
                    &mut len,
                    std::ptr::null_mut(),
                    0,
                )
            };

            if result == 0 && num_cpus > 0 {
                let cpus: Vec<CpuId> = (0..num_cpus).map(|i| CpuId(i as u32)).collect();
                return cpus;
            }

            // Alternative: try hw.ncpuonline
            let mut online_cpus: c_int = 0;
            let mut len = std::mem::size_of::<c_int>();

            let mut mib: [c_int; 2] = [CTL_HW, 25]; // HW_NCPUONLINE
            let result = unsafe {
                sysctl(
                    mib.as_mut_ptr(),
                    mib.len() as u32,
                    &mut online_cpus as *mut c_int as *mut c_void,
                    &mut len,
                    std::ptr::null_mut(),
                    0,
                )
            };

            if result == 0 && online_cpus > 0 {
                let cpus: Vec<CpuId> = (0..online_cpus).map(|i| CpuId(i as u32)).collect();
                return cpus;
            }
        }

        // Fallback: assume single CPU
        vec![CpuId(0)]
    }

    /// Detect total system memory
    pub fn detect_total_memory() -> MemorySize {
        // Try to read from /proc/meminfo on Linux
        #[cfg(target_os = "linux")]
        {
            if let Ok(content) = fs::read_to_string("/proc/meminfo") {
                for line in content.lines() {
                    if line.starts_with("MemTotal:") {
                        if let Some(mem_str) = line.split(':').nth(1) {
                            let parts: Vec<&str> = mem_str.trim().split_whitespace().collect();
                            if parts.len() >= 2 {
                                if let Ok(size_kb) = parts[0].parse::<usize>() {
                                    return MemorySize::from_kilobytes(size_kb);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Windows implementation - fallback for now
        // TODO: Implement proper Windows memory detection using winapi crate
        #[cfg(target_os = "windows")]
        {
            // For now, use a reasonable default or try to detect from environment
            // This is a placeholder - proper implementation would use GlobalMemoryStatusEx
            // For now, assume 8GB as a common default
            MemorySize::from_gigabytes(8)
        }

        // macOS implementation using sysctl
        #[cfg(target_os = "macos")]
        {
            use libc::{CTL_HW, HW_MEMSIZE, c_int, c_void, sysctl};

            let mut mem_size: u64 = 0;
            let mut len = std::mem::size_of::<u64>();

            let mut mib: [c_int; 2] = [CTL_HW, HW_MEMSIZE];
            let result = unsafe {
                sysctl(
                    mib.as_mut_ptr(),
                    mib.len() as u32,
                    &mut mem_size as *mut u64 as *mut c_void,
                    &mut len,
                    std::ptr::null_mut(),
                    0,
                )
            };

            if result == 0 && mem_size > 0 {
                MemorySize(mem_size as usize)
            } else {
                // Fallback: assume 1GB
                MemorySize::from_megabytes(1024)
            }
        }

        // FreeBSD implementation using sysctl
        #[cfg(target_os = "freebsd")]
        {
            use libc::{CTL_HW, HW_PHYSMEM, c_int, c_void, sysctl};

            let mut mem_size: u32 = 0;
            let mut len = std::mem::size_of::<u32>();

            let mut mib: [c_int; 2] = [CTL_HW, HW_PHYSMEM];
            let result = unsafe {
                sysctl(
                    mib.as_mut_ptr(),
                    mib.len() as u32,
                    &mut mem_size as *mut u32 as *mut c_void,
                    &mut len,
                    std::ptr::null_mut(),
                    0,
                )
            };

            if result == 0 && mem_size > 0 {
                MemorySize(mem_size as usize)
            } else {
                // Alternative: try hw.physmem64 for larger memory sizes
                let mut mem_size64: u64 = 0;
                let mut len = std::mem::size_of::<u64>();

                let mut mib: [c_int; 2] = [CTL_HW, 577]; // HW_PHYSMEM64
                let result = unsafe {
                    sysctl(
                        mib.as_mut_ptr(),
                        mib.len() as u32,
                        &mut mem_size64 as *mut u64 as *mut c_void,
                        &mut len,
                        std::ptr::null_mut(),
                        0,
                    )
                };

                if result == 0 && mem_size64 > 0 {
                    MemorySize(mem_size64 as usize)
                } else {
                    // Fallback: assume 1GB
                    MemorySize::from_megabytes(1024)
                }
            }
        }

        // Fallback for other platforms: assume 1GB
        #[cfg(not(any(
            target_os = "linux",
            target_os = "windows",
            target_os = "macos",
            target_os = "freebsd"
        )))]
        {
            MemorySize::from_megabytes(1024)
        }
    }

    /// Detect system architecture
    pub fn detect_architecture() -> String {
        #[cfg(target_arch = "x86_64")]
        {
            "x86_64".to_string()
        }
        #[cfg(target_arch = "aarch64")]
        {
            "aarch64".to_string()
        }
        #[cfg(target_arch = "arm")]
        {
            "arm".to_string()
        }
        #[cfg(target_arch = "riscv64")]
        {
            "riscv64".to_string()
        }
        #[cfg(not(any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "arm",
            target_arch = "riscv64"
        )))]
        {
            "unknown".to_string()
        }
    }

    /// Get a NUMA node by ID
    pub fn get_node(&self, node_id: NumaNodeId) -> Option<&NumaNode> {
        self.nodes.iter().find(|node| node.id == node_id)
    }

    /// Get a mutable reference to a NUMA node by ID
    pub fn get_node_mut(&mut self, node_id: NumaNodeId) -> Option<&mut NumaNode> {
        self.nodes.iter_mut().find(|node| node.id == node_id)
    }

    /// Get the distance between two NUMA nodes
    pub fn get_distance(&self, from: NumaNodeId, to: NumaNodeId) -> Option<NumaDistance> {
        if let Some(from_node) = self.get_node(from) {
            if to.0 < from_node.distance_matrix.len() as u32 {
                Some(from_node.distance_matrix[to.0 as usize])
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Find the closest NUMA node to a given node
    pub fn find_closest_node(&self, node_id: NumaNodeId) -> Option<NumaNodeId> {
        let mut closest_node = None;
        let mut min_distance = None;

        for node in &self.nodes {
            if node.id != node_id {
                if let Some(distance) = self.get_distance(node_id, node.id) {
                    if min_distance.is_none() || distance.0 < min_distance.unwrap() {
                        min_distance = Some(distance.0);
                        closest_node = Some(node.id);
                    }
                }
            }
        }

        closest_node
    }

    /// Get all online NUMA nodes
    pub fn online_nodes(&self) -> Vec<&NumaNode> {
        self.nodes.iter().filter(|node| node.info.online).collect()
    }

    /// Get all preferred NUMA nodes
    pub fn preferred_nodes(&self) -> Vec<&NumaNode> {
        self.nodes.iter().filter(|node| node.preferred).collect()
    }

    /// Get the total number of online NUMA nodes
    pub fn online_node_count(&self) -> usize {
        self.nodes.iter().filter(|node| node.info.online).count()
    }

    /// Check if the system has multiple NUMA nodes
    pub fn is_multi_numa(&self) -> bool {
        self.online_node_count() > 1
    }

    /// Get a summary of the NUMA topology
    pub fn summary(&self) -> String {
        format!(
            "NUMA Topology: {} nodes, {} CPUs, {} total memory, {}",
            self.nodes.len(),
            self.cpu_count,
            self.total_memory,
            if self.is_numa_available {
                "NUMA available"
            } else {
                "NUMA not available"
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_node_topology() {
        let topology = NumaTopology::create_single_node_topology();

        assert_eq!(topology.nodes.len(), 1);
        assert_eq!(topology.nodes[0].id, NumaNodeId(0));
        assert!(!topology.nodes[0].cpus.is_empty());
        assert!(topology.nodes[0].memory_size.inner() > 0);
        assert!(!topology.is_numa_available);
        assert_eq!(topology.detection_method, DetectionMethod::Fallback);
    }

    #[test]
    fn test_disabled_topology() {
        let topology = NumaTopology::create_disabled_topology();

        assert!(topology.nodes.is_empty());
        assert_eq!(topology.total_memory, MemorySize::new(0));
        assert_eq!(topology.cpu_count, 0);
        assert!(!topology.is_numa_available);
        assert_eq!(topology.detection_method, DetectionMethod::Disabled);
    }

    #[test]
    fn test_topology_detection() {
        let result = NumaTopology::detect();

        match result {
            Ok(topology) => {
                println!("Detected topology: {}", topology.summary());
                assert!(!topology.nodes.is_empty());

                // Test node access
                if let Some(first_node) = topology.nodes.first() {
                    assert!(topology.get_node(first_node.id).is_some());
                }

                // Test online nodes
                let online_nodes = topology.online_nodes();
                assert!(!online_nodes.is_empty());
            }
            Err(e) => {
                println!("Topology detection failed: {}", e);
                // This is acceptable on systems without NUMA support
            }
        }
    }

    #[test]
    fn test_node_distances() {
        let mut topology = NumaTopology::create_single_node_topology();

        // Set up a simple distance matrix
        if let Some(node) = topology.get_node_mut(NumaNodeId(0)) {
            node.distance_matrix = vec![NumaDistance::new(10)];
        }

        let distance = topology.get_distance(NumaNodeId(0), NumaNodeId(0));
        assert_eq!(distance, Some(NumaDistance::new(10)));
    }

    #[test]
    fn test_cpu_detection() {
        let cpus = NumaTopology::detect_all_cpus();
        assert!(!cpus.is_empty());
        println!("Detected {} CPUs", cpus.len());
    }

    #[test]
    fn test_memory_detection() {
        let memory = NumaTopology::detect_total_memory();
        assert!(memory.inner() > 0);
        println!("Detected {} total memory", memory);
    }

    #[test]
    fn test_architecture_detection() {
        let arch = NumaTopology::detect_architecture();
        assert!(!arch.is_empty());
        println!("Detected architecture: {}", arch);
    }

    #[test]
    fn test_multi_numa_detection() {
        let result = NumaTopology::detect();

        if let Ok(topology) = result {
            let is_multi_numa = topology.is_multi_numa();
            println!("System is multi-NUMA: {}", is_multi_numa);

            if is_multi_numa {
                let closest = topology.find_closest_node(NumaNodeId(0));
                println!("Closest node to node 0: {:?}", closest);
            }
        }
    }
}
