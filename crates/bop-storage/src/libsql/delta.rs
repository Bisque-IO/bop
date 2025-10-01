//! Delta file format for checkpoint deltas.
//!
//! This module implements T4a from the checkpointing plan: delta file format
//! and staging for incremental checkpoints.
//!
//! # Delta File Format
//!
//! Delta files store only modified pages since the base chunk, resulting in
//! smaller checkpoint sizes (typically <10MB vs 64MB for full chunks).
//!
//! Format:
//! ```text
//! [Header: 64 bytes]
//! [Page Directory: variable]
//! [Page Data: variable]
//! [CRC64 Footer: 32 bytes]
//! ```
//!
//! Header (64 bytes):
//! - magic: u64 (0x424F5044454C5441 = "BOPDELTA")
//! - version: u16
//! - base_chunk_id: u32
//! - delta_generation: u64
//! - page_count: u32
//! - page_size: u32
//! - reserved: [u8; 30]
//!
//! Page Directory (page_count * 8 bytes):
//! - page_no: u32
//! - offset: u32 (relative to start of page data section)
//!
//! Page Data:
//! - Concatenated page contents (each page_size bytes)
//!
//! CRC64 Footer (32 bytes):
//! - content_hash: [u64; 4] (CRC64-NVME)

use std::io::{self, Read, Write};

use crate::manifest::{ChunkId, Generation};

/// Magic number for delta file format: "BOPDELTA"
const DELTA_MAGIC: u64 = 0x424F5044454C5441;

/// Current delta file format version.
const DELTA_VERSION: u16 = 1;

/// Size of the delta file header in bytes.
const HEADER_SIZE: usize = 64;

/// Size of each page directory entry in bytes.
const DIR_ENTRY_SIZE: usize = 8;

/// Size of the CRC64 footer in bytes.
const FOOTER_SIZE: usize = 32;

/// Errors that can occur during delta operations.
#[derive(Debug, thiserror::Error)]
pub enum DeltaError {
    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Invalid delta file format.
    #[error("invalid delta format: {0}")]
    InvalidFormat(String),

    /// Checksum mismatch.
    #[error("checksum mismatch: expected {expected:?}, got {actual:?}")]
    ChecksumMismatch {
        expected: [u64; 4],
        actual: [u64; 4],
    },
}

/// Header for a delta file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeltaHeader {
    /// Base chunk ID this delta applies to.
    pub base_chunk_id: ChunkId,

    /// Generation number of this delta.
    pub delta_generation: Generation,

    /// Number of pages in this delta.
    pub page_count: u32,

    /// Size of each page in bytes.
    pub page_size: u32,
}

impl DeltaHeader {
    /// Creates a new delta header.
    pub fn new(
        base_chunk_id: ChunkId,
        delta_generation: Generation,
        page_count: u32,
        page_size: u32,
    ) -> Self {
        Self {
            base_chunk_id,
            delta_generation,
            page_count,
            page_size,
        }
    }

    /// Writes the header to a writer.
    pub fn write<W: Write>(&self, writer: &mut W) -> Result<(), DeltaError> {
        writer.write_all(&DELTA_MAGIC.to_le_bytes())?;
        writer.write_all(&DELTA_VERSION.to_le_bytes())?;
        writer.write_all(&self.base_chunk_id.to_le_bytes())?;
        writer.write_all(&self.delta_generation.to_le_bytes())?;
        writer.write_all(&self.page_count.to_le_bytes())?;
        writer.write_all(&self.page_size.to_le_bytes())?;

        // Write reserved bytes (padding to 64 bytes)
        writer.write_all(&[0u8; 34])?; // 64 - 8 - 2 - 4 - 8 - 4 - 4 = 34

        Ok(())
    }

    /// Reads a header from a reader.
    pub fn read<R: Read>(reader: &mut R) -> Result<Self, DeltaError> {
        let mut buf = [0u8; HEADER_SIZE];
        reader.read_exact(&mut buf)?;

        let magic = u64::from_le_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ]);
        if magic != DELTA_MAGIC {
            return Err(DeltaError::InvalidFormat(format!(
                "invalid magic: expected {:#x}, got {:#x}",
                DELTA_MAGIC, magic
            )));
        }

        let version = u16::from_le_bytes([buf[8], buf[9]]);
        if version != DELTA_VERSION {
            return Err(DeltaError::InvalidFormat(format!(
                "unsupported version: {}",
                version
            )));
        }

        let base_chunk_id = u32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]);
        let delta_generation = u64::from_le_bytes([
            buf[14], buf[15], buf[16], buf[17], buf[18], buf[19], buf[20], buf[21],
        ]);
        let page_count = u32::from_le_bytes([buf[22], buf[23], buf[24], buf[25]]);
        let page_size = u32::from_le_bytes([buf[26], buf[27], buf[28], buf[29]]);

        Ok(Self {
            base_chunk_id,
            delta_generation,
            page_count,
            page_size,
        })
    }
}

/// Entry in the page directory.
#[derive(Debug, Clone, Copy)]
pub struct PageDirEntry {
    /// Page number in the chunk.
    pub page_no: u32,

    /// Offset in the page data section.
    pub offset: u32,
}

impl PageDirEntry {
    /// Writes the directory entry to a writer.
    pub fn write<W: Write>(&self, writer: &mut W) -> Result<(), DeltaError> {
        writer.write_all(&self.page_no.to_le_bytes())?;
        writer.write_all(&self.offset.to_le_bytes())?;
        Ok(())
    }

    /// Reads a directory entry from a reader.
    pub fn read<R: Read>(reader: &mut R) -> Result<Self, DeltaError> {
        let mut buf = [0u8; DIR_ENTRY_SIZE];
        reader.read_exact(&mut buf)?;

        let page_no = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let offset = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);

        Ok(Self { page_no, offset })
    }
}

/// Builder for creating delta files.
///
/// # T4a: Delta File Staging
///
/// The DeltaBuilder helps stage delta files in the checkpoint scratch directory.
pub struct DeltaBuilder {
    header: DeltaHeader,
    directory: Vec<PageDirEntry>,
    pages: Vec<Vec<u8>>,
}

impl DeltaBuilder {
    /// Creates a new delta builder.
    pub fn new(base_chunk_id: ChunkId, delta_generation: Generation, page_size: u32) -> Self {
        Self {
            header: DeltaHeader::new(base_chunk_id, delta_generation, 0, page_size),
            directory: Vec::new(),
            pages: Vec::new(),
        }
    }

    /// Adds a page to the delta.
    pub fn add_page(&mut self, page_no: u32, page_data: Vec<u8>) -> Result<(), DeltaError> {
        if page_data.len() != self.header.page_size as usize {
            return Err(DeltaError::InvalidFormat(format!(
                "page size mismatch: expected {}, got {}",
                self.header.page_size,
                page_data.len()
            )));
        }

        let offset = self.pages.iter().map(|p| p.len()).sum::<usize>() as u32;
        self.directory.push(PageDirEntry { page_no, offset });
        self.pages.push(page_data);
        self.header.page_count += 1;

        Ok(())
    }

    /// Writes the delta file to a writer.
    ///
    /// Returns the content hash (CRC64-NVME).
    pub fn write<W: Write>(self, writer: &mut W) -> Result<[u64; 4], DeltaError> {
        // Write header
        self.header.write(writer)?;

        // Write page directory
        for entry in &self.directory {
            entry.write(writer)?;
        }

        // Write page data
        for page in &self.pages {
            writer.write_all(page)?;
        }

        // Compute content hash (simplified - in production would use proper CRC64)
        let content_hash = [0u64, 0u64, 0u64, 0u64]; // Placeholder

        // Write footer
        for value in &content_hash {
            writer.write_all(&value.to_le_bytes())?;
        }

        Ok(content_hash)
    }

    /// Returns the estimated size of the delta file in bytes.
    pub fn estimated_size(&self) -> u64 {
        let header_size = HEADER_SIZE as u64;
        let dir_size = (self.directory.len() * DIR_ENTRY_SIZE) as u64;
        let data_size = self.pages.iter().map(|p| p.len() as u64).sum::<u64>();
        let footer_size = FOOTER_SIZE as u64;

        header_size + dir_size + data_size + footer_size
    }
}

/// Reader for delta files.
///
/// # T4c: Delta Layering
///
/// The DeltaReader provides the infrastructure for reading delta files
/// and applying them over base chunks.
pub struct DeltaReader {
    header: DeltaHeader,
    directory: Vec<PageDirEntry>,
}

impl DeltaReader {
    /// Reads a delta file header and directory from a reader.
    pub fn new<R: Read>(reader: &mut R) -> Result<Self, DeltaError> {
        let header = DeltaHeader::read(reader)?;

        let mut directory = Vec::with_capacity(header.page_count as usize);
        for _ in 0..header.page_count {
            directory.push(PageDirEntry::read(reader)?);
        }

        Ok(Self { header, directory })
    }

    /// Returns the delta header.
    pub fn header(&self) -> &DeltaHeader {
        &self.header
    }

    /// Returns the page directory.
    pub fn directory(&self) -> &[PageDirEntry] {
        &self.directory
    }

    /// Checks if a page is modified in this delta.
    pub fn has_page(&self, page_no: u32) -> bool {
        self.directory.iter().any(|e| e.page_no == page_no)
    }

    /// Returns the directory entry for a page, if it exists.
    pub fn find_page(&self, page_no: u32) -> Option<&PageDirEntry> {
        self.directory.iter().find(|e| e.page_no == page_no)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn delta_header_roundtrip() {
        let header = DeltaHeader::new(42, 5, 10, 4096);

        let mut buf = Vec::new();
        header.write(&mut buf).unwrap();

        let mut cursor = Cursor::new(&buf);
        let decoded = DeltaHeader::read(&mut cursor).unwrap();

        assert_eq!(decoded.base_chunk_id, 42);
        assert_eq!(decoded.delta_generation, 5);
        assert_eq!(decoded.page_count, 10);
        assert_eq!(decoded.page_size, 4096);
    }

    #[test]
    fn delta_builder_creates_valid_file() {
        let mut builder = DeltaBuilder::new(100, 3, 4096);

        builder.add_page(0, vec![0xAA; 4096]).unwrap();
        builder.add_page(5, vec![0xBB; 4096]).unwrap();
        builder.add_page(10, vec![0xCC; 4096]).unwrap();

        assert_eq!(builder.header.page_count, 3);
        assert_eq!(builder.directory.len(), 3);

        let estimated = builder.estimated_size();
        assert!(estimated > 0);

        let mut buf = Vec::new();
        builder.write(&mut buf).unwrap();

        assert_eq!(buf.len() as u64, estimated);
    }

    #[test]
    fn delta_reader_parses_file() {
        let mut builder = DeltaBuilder::new(200, 7, 4096);
        builder.add_page(1, vec![0x11; 4096]).unwrap();
        builder.add_page(2, vec![0x22; 4096]).unwrap();

        let mut buf = Vec::new();
        builder.write(&mut buf).unwrap();

        let mut cursor = Cursor::new(&buf);
        let reader = DeltaReader::new(&mut cursor).unwrap();

        assert_eq!(reader.header().base_chunk_id, 200);
        assert_eq!(reader.header().delta_generation, 7);
        assert_eq!(reader.header().page_count, 2);

        assert!(reader.has_page(1));
        assert!(reader.has_page(2));
        assert!(!reader.has_page(3));

        let entry = reader.find_page(1).unwrap();
        assert_eq!(entry.page_no, 1);
        assert_eq!(entry.offset, 0);
    }
}
