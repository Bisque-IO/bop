use std::io;

/// Direct I/O requirements for reads/writes.
///
/// These rules are typically enforced by the OS when a file is opened with a
/// direct I/O / no-buffering mode (e.g. Linux `O_DIRECT`, Windows `FILE_FLAG_NO_BUFFERING`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectIoRequirements {
    /// Alignment in bytes for buffer pointer, offsets, and lengths.
    pub alignment: usize,
    pub requires_aligned_buffer: bool,
    pub requires_aligned_offset: bool,
    pub requires_aligned_len: bool,
}

impl DirectIoRequirements {
    /// A conservative default commonly valid on modern systems (4KiB).
    pub const fn conservative_4k() -> Self {
        Self {
            alignment: 4096,
            requires_aligned_buffer: true,
            requires_aligned_offset: true,
            requires_aligned_len: true,
        }
    }

    pub fn validate(&self, offset: u64, len: usize, ptr: *const u8) -> io::Result<()> {
        let a = self.alignment;
        if self.requires_aligned_offset && (offset as usize) % a != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("direct I/O requires offset aligned to {} bytes", a),
            ));
        }
        if self.requires_aligned_len && len % a != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("direct I/O requires length aligned to {} bytes", a),
            ));
        }
        if self.requires_aligned_buffer && (ptr as usize) % a != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("direct I/O requires buffer aligned to {} bytes", a),
            ));
        }
        Ok(())
    }
}

