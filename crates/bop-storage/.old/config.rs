//! Defines the storage flavor and validated geometry (pages, extents, slabs) used by bop-storage.
use std::error::Error;
use std::fmt;
use std::num::NonZeroU64;

/// Fixed 4 KiB page size used throughout the paged file design.
pub const PAGE_SIZE: u64 = 4 * 1024;
/// Minimum supported extent size (64 KiB).
pub const MIN_EXTENT_SIZE: u64 = 64 * 1024;
/// Maximum supported extent size (2 MiB).
pub const MAX_EXTENT_SIZE: u64 = 2 * 1024 * 1024;
/// Minimum supported slab size (2 MiB).
pub const MIN_SLAB_SIZE: u64 = 2 * 1024 * 1024;
/// Maximum supported slab size (128 MiB).
pub const MAX_SLAB_SIZE: u64 = 128 * 1024 * 1024;

/// Enumerates the storage flavor, matching the design document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageFlavor {
    /// Append-only log with front/back truncation semantics.
    Aof,
    /// Sparse page file with arbitrary page deletes.
    Standard,
}

/// Errors produced when validating configuration inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    ExtentOutOfRange { bytes: u64 },
    ExtentNotAligned { bytes: u64 },
    SlabOutOfRange { bytes: u64 },
    SlabNotAligned { bytes: u64 },
    SlabNotMultipleOfExtent { slab: u64, extent: u64 },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExtentOutOfRange { bytes } => write!(
                f,
                "extent size {bytes} must be between {MIN_EXTENT_SIZE} and {MAX_EXTENT_SIZE} bytes"
            ),
            Self::ExtentNotAligned { bytes } => write!(
                f,
                "extent size {bytes} must be a multiple of {PAGE_SIZE} bytes"
            ),
            Self::SlabOutOfRange { bytes } => write!(
                f,
                "slab size {bytes} must be between {MIN_SLAB_SIZE} and {MAX_SLAB_SIZE} bytes"
            ),
            Self::SlabNotAligned { bytes } => write!(
                f,
                "slab size {bytes} must be a multiple of {PAGE_SIZE} bytes"
            ),
            Self::SlabNotMultipleOfExtent { slab, extent } => write!(
                f,
                "slab size {slab} must be an integer multiple of extent size {extent}"
            ),
        }
    }
}

impl Error for ConfigError {}

/// Wrapper that guarantees a valid extent size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExtentSize(NonZeroU64);

impl ExtentSize {
    pub fn new(bytes: u64) -> Result<Self, ConfigError> {
        if !(MIN_EXTENT_SIZE..=MAX_EXTENT_SIZE).contains(&bytes) {
            return Err(ConfigError::ExtentOutOfRange { bytes });
        }
        if bytes % PAGE_SIZE != 0 {
            return Err(ConfigError::ExtentNotAligned { bytes });
        }
        Ok(Self(
            NonZeroU64::new(bytes).expect("extent bounds ensure non-zero"),
        ))
    }

    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl Default for ExtentSize {
    fn default() -> Self {
        Self::new(MIN_EXTENT_SIZE).expect("default extent is valid")
    }
}

/// Wrapper that guarantees a valid slab size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlabSize(NonZeroU64);

impl SlabSize {
    pub fn new(bytes: u64, extent: ExtentSize) -> Result<Self, ConfigError> {
        if !(MIN_SLAB_SIZE..=MAX_SLAB_SIZE).contains(&bytes) {
            return Err(ConfigError::SlabOutOfRange { bytes });
        }
        if bytes % PAGE_SIZE != 0 {
            return Err(ConfigError::SlabNotAligned { bytes });
        }
        let extent_bytes = extent.get();
        if bytes % extent_bytes != 0 {
            return Err(ConfigError::SlabNotMultipleOfExtent {
                slab: bytes,
                extent: extent_bytes,
            });
        }
        Ok(Self(
            NonZeroU64::new(bytes).expect("slab bounds ensure non-zero"),
        ))
    }

    pub fn get(self) -> u64 {
        self.0.get()
    }
}

/// Allocator selection. Currently only the system allocator is supported but the enum keeps the API future proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SlabAllocatorKind {
    #[default]
    System,
}

/// Aggregate storage configuration used by the slab allocator and higher layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StorageConfig {
    pub flavor: StorageFlavor,
    pub extent_size: ExtentSize,
    pub slab_size: SlabSize,
    pub allocator: SlabAllocatorKind,
    /// Number of slabs each reader should prefetch ahead of its cursor.
    pub readahead_slabs: u16,
}

impl StorageConfig {
    pub const DEFAULT_READAHEAD_SLABS: u16 = 2;

    pub fn new(
        flavor: StorageFlavor,
        extent_bytes: u64,
        slab_bytes: u64,
        readahead_slabs: u16,
    ) -> Result<Self, ConfigError> {
        let extent_size = ExtentSize::new(extent_bytes)?;
        let slab_size = SlabSize::new(slab_bytes, extent_size)?;
        Ok(Self {
            flavor,
            extent_size,
            slab_size,
            allocator: SlabAllocatorKind::System,
            readahead_slabs: readahead_slabs.max(1),
        })
    }

    pub fn extent_size(&self) -> ExtentSize {
        self.extent_size
    }

    pub fn slab_size(&self) -> SlabSize {
        self.slab_size
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self::new(
            StorageFlavor::Aof,
            MIN_EXTENT_SIZE,
            16 * 1024 * 1024,
            Self::DEFAULT_READAHEAD_SLABS,
        )
        .expect("default config must be valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let cfg = StorageConfig::default();
        assert_eq!(cfg.extent_size().get(), MIN_EXTENT_SIZE);
        assert_eq!(cfg.slab_size().get(), 16 * 1024 * 1024);
        assert_eq!(cfg.allocator, SlabAllocatorKind::System);
    }

    #[test]
    fn rejects_extent_out_of_range() {
        assert!(matches!(
            StorageConfig::new(StorageFlavor::Aof, MIN_EXTENT_SIZE / 2, MIN_SLAB_SIZE, 1),
            Err(ConfigError::ExtentOutOfRange { .. })
        ));
    }

    #[test]
    fn rejects_slab_not_multiple_of_extent() {
        let res = StorageConfig::new(
            StorageFlavor::Aof,
            MIN_EXTENT_SIZE,
            MIN_SLAB_SIZE + PAGE_SIZE,
            1,
        );
        assert!(matches!(
            res,
            Err(ConfigError::SlabNotMultipleOfExtent { .. })
        ));
    }

    #[test]
    fn accepts_maximum_values() {
        let cfg = StorageConfig::new(
            StorageFlavor::Standard,
            MAX_EXTENT_SIZE,
            MAX_SLAB_SIZE,
            StorageConfig::DEFAULT_READAHEAD_SLABS,
        )
        .expect("should accept upper bounds");
        assert_eq!(cfg.extent_size().get(), MAX_EXTENT_SIZE);
        assert_eq!(cfg.slab_size().get(), MAX_SLAB_SIZE);
    }
}
