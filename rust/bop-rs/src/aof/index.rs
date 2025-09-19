use crc64fast_nvme::Digest as Crc64Digest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::aof::error::AofError;

/// Serializable index for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableIndex {
    pub version: u32,
    pub record_count: u64,
    pub entries: Vec<(u64, u64)>, // (record_id, offset)
    pub checksum: u64,
}

/// Segment footer for InSegment index storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentFooter {
    pub magic: u32,           // Magic number to identify footer
    pub version: u32,         // Footer version
    pub index_offset: u64,    // Offset where index data starts
    pub index_size: u64,      // Size of index data in bytes
    pub record_count: u64,    // Number of records in segment
    pub data_checksum: u64,   // Checksum of all record data
    pub index_checksum: u64,  // Checksum of index data
    pub footer_checksum: u64, // Checksum of footer data (excluding this field)
}

pub const SEGMENT_FOOTER_MAGIC: u32 = 0x5E6F07E5; // "SEgment Footer" in hex-ish
pub const SEGMENT_FOOTER_VERSION: u32 = 1;

impl SegmentFooter {
    pub fn new(
        index_offset: u64,
        index_size: u64,
        record_count: u64,
        data_checksum: u64,
        index_checksum: u64,
    ) -> Self {
        let mut footer = Self {
            magic: SEGMENT_FOOTER_MAGIC,
            version: SEGMENT_FOOTER_VERSION,
            index_offset,
            index_size,
            record_count,
            data_checksum,
            index_checksum,
            footer_checksum: 0, // Will be calculated
        };

        // Calculate footer checksum (excluding the checksum field itself)
        let mut hasher = Crc64Digest::new();
        hasher.write(&footer.magic.to_le_bytes());
        hasher.write(&footer.version.to_le_bytes());
        hasher.write(&footer.index_offset.to_le_bytes());
        hasher.write(&footer.index_size.to_le_bytes());
        hasher.write(&footer.record_count.to_le_bytes());
        hasher.write(&footer.data_checksum.to_le_bytes());
        hasher.write(&footer.index_checksum.to_le_bytes());
        footer.footer_checksum = hasher.sum64();

        footer
    }

    pub fn size() -> usize {
        std::mem::size_of::<Self>()
    }

    pub fn verify(&self) -> bool {
        if self.magic != SEGMENT_FOOTER_MAGIC || self.version != SEGMENT_FOOTER_VERSION {
            return false;
        }

        // Verify footer checksum
        let mut hasher = Crc64Digest::new();
        hasher.write(&self.magic.to_le_bytes());
        hasher.write(&self.version.to_le_bytes());
        hasher.write(&self.index_offset.to_le_bytes());
        hasher.write(&self.index_size.to_le_bytes());
        hasher.write(&self.record_count.to_le_bytes());
        hasher.write(&self.data_checksum.to_le_bytes());
        hasher.write(&self.index_checksum.to_le_bytes());

        hasher.sum64() == self.footer_checksum
    }

    pub fn serialize(&self) -> Result<Vec<u8>, AofError> {
        bincode::serialize(self)
            .map_err(|e| AofError::Serialization(format!("Failed to serialize footer: {}", e)))
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, AofError> {
        bincode::deserialize(data)
            .map_err(|e| AofError::Serialization(format!("Failed to deserialize footer: {}", e)))
    }

    /// Read footer from the end of a file/mmap
    pub fn read_from_end(data: &[u8]) -> Result<Self, AofError> {
        if data.len() < Self::size() {
            return Err(AofError::CorruptedRecord(
                "File too small for footer".to_string(),
            ));
        }

        let footer_start = data.len() - Self::size();
        let footer_bytes = &data[footer_start..];
        let footer = Self::deserialize(footer_bytes)?;

        if !footer.verify() {
            return Err(AofError::CorruptedRecord(
                "Footer verification failed".to_string(),
            ));
        }

        Ok(footer)
    }
}

impl SerializableIndex {
    pub fn new(index: &BTreeMap<u64, u64>) -> Self {
        let entries: Vec<(u64, u64)> = index.iter().map(|(&k, &v)| (k, v)).collect();
        let mut hasher = Crc64Digest::new();

        // Calculate checksum of the index data
        for (id, offset) in &entries {
            hasher.write(&id.to_le_bytes());
            hasher.write(&offset.to_le_bytes());
        }

        Self {
            version: 1,
            record_count: entries.len() as u64,
            entries,
            checksum: hasher.sum64(),
        }
    }

    pub fn to_btree_map(&self) -> Result<BTreeMap<u64, u64>, AofError> {
        // Verify checksum
        let mut hasher = Crc64Digest::new();
        for (id, offset) in &self.entries {
            hasher.write(&id.to_le_bytes());
            hasher.write(&offset.to_le_bytes());
        }

        if hasher.sum64() != self.checksum {
            return Err(AofError::CorruptedRecord(
                "Index checksum verification failed".to_string(),
            ));
        }

        Ok(self.entries.iter().cloned().collect())
    }

    pub fn serialize(&self) -> Result<Vec<u8>, AofError> {
        bincode::serialize(self)
            .map_err(|e| AofError::Serialization(format!("Failed to serialize index: {}", e)))
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, AofError> {
        bincode::deserialize(data)
            .map_err(|e| AofError::Serialization(format!("Failed to deserialize index: {}", e)))
    }
}
