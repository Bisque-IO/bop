//! Minimal page directory built on Roaring bitmaps with compact value metadata.
use roaring::RoaringTreemap;
use std::io::{self, Read, Write};

/// Packed metadata tracked per logical page.
///
/// Layout (high -> low bits):
/// - bits 63..32  : 32-bit checksum
/// - bits 31..4   : 28-bit payload length (bytes)
/// - bit 3        : zstd compression flag
/// - bits 2..0    : reserved for future use
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryValue(u64);

impl DirectoryValue {
    const LENGTH_MASK: u64 = (1 << 28) - 1;
    const LENGTH_SHIFT: u32 = 4;
    const COMPRESSED_BIT: u64 = 1 << 3;
    const RESERVED_MASK: u64 = 0b111;
    const RESERVED_MAX: u8 = 0b111;

    /// Upper bound for the 28-bit length field.
    pub const MAX_LENGTH: u32 = Self::LENGTH_MASK as u32;

    /// Creates a packed value, validating that the supplied length and reserved bits fit.
    pub fn new(
        length: u32,
        checksum: u32,
        compressed: bool,
        reserved: u8,
    ) -> Result<Self, DirectoryValueError> {
        if length > Self::MAX_LENGTH {
            return Err(DirectoryValueError::LengthOverflow { length });
        }
        if reserved > Self::RESERVED_MAX {
            return Err(DirectoryValueError::ReservedOverflow { reserved });
        }
        let mut packed = (checksum as u64) << 32;
        packed |= (length as u64) << Self::LENGTH_SHIFT;
        if compressed {
            packed |= Self::COMPRESSED_BIT;
        }
        packed |= (reserved as u64) & Self::RESERVED_MASK;
        Ok(Self(packed))
    }

    /// Returns the stored checksum.
    pub fn checksum(self) -> u32 {
        (self.0 >> 32) as u32
    }

    /// Returns the stored payload length in bytes.
    pub fn length(self) -> u32 {
        ((self.0 >> Self::LENGTH_SHIFT) & Self::LENGTH_MASK) as u32
    }

    /// Indicates whether the payload was compressed with zstd prior to persistence.
    pub fn is_compressed(self) -> bool {
        (self.0 & Self::COMPRESSED_BIT) != 0
    }

    /// Returns the reserved bits (currently unused).
    pub fn reserved(self) -> u8 {
        (self.0 & Self::RESERVED_MASK) as u8
    }

    /// Internal: exposes the packed representation for persistence.
    pub fn to_packed(self) -> u64 {
        self.0
    }

    /// Internal: constructs from a packed representation without validation.
    pub fn from_packed(packed: u64) -> Self {
        Self(packed)
    }
}

/// Errors surfaced while constructing packed directory values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryValueError {
    LengthOverflow { length: u32 },
    ReservedOverflow { reserved: u8 },
}

/// Memory-efficient page directory built on top of a Roaring bitmap.
#[derive(Debug, Clone, Default)]
pub struct PageDirectory {
    pages: RoaringTreemap,
    values: Vec<DirectoryValue>,
}

impl PageDirectory {
    /// Creates an empty directory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of live page entries.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns true when no pages are tracked.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Removes every entry.
    pub fn clear(&mut self) {
        self.pages.clear();
        self.values.clear();
    }

    /// Returns true when a page id exists.
    pub fn contains(&self, page_id: u64) -> bool {
        self.pages.contains(page_id)
    }

    /// Retrieves the stored value for `page_id`.
    pub fn get(&self, page_id: u64) -> Option<DirectoryValue> {
        self.index_of(page_id)
            .and_then(|idx| self.values.get(idx))
            .copied()
    }

    /// Inserts or replaces a page entry. Returns the previous value when present.
    pub fn upsert(&mut self, page_id: u64, value: DirectoryValue) -> Option<DirectoryValue> {
        if let Some(idx) = self.index_of(page_id) {
            return Some(std::mem::replace(&mut self.values[idx], value));
        }
        let inserted = self.pages.insert(page_id);
        debug_assert!(inserted, "inserted index must be new");
        let rank = self.pages.rank(page_id);
        debug_assert!(rank > 0, "rank must be at least one after insertion");
        let idx = (rank - 1) as usize;
        self.values.insert(idx, value);
        None
    }

    /// Removes a page entry, returning its metadata if it existed.
    pub fn remove(&mut self, page_id: u64) -> Option<DirectoryValue> {
        let idx = self.index_of(page_id)?;
        self.pages.remove(page_id);
        Some(self.values.remove(idx))
    }

    /// Iterator over `(page_id, value)` pairs in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = (u64, DirectoryValue)> + '_ {
        self.pages.iter().zip(self.values.iter().copied())
    }

    /// Writes the directory contents to a `Write` sink.
    pub fn persist_to<W: Write>(&self, mut writer: W) -> io::Result<()> {
        let len = self.len() as u64;
        writer.write_all(&len.to_le_bytes())?;
        for (page, value) in self.iter() {
            writer.write_all(&page.to_le_bytes())?;
            writer.write_all(&value.to_packed().to_le_bytes())?;
        }
        Ok(())
    }

    /// Reconstructs a directory from a `Read` source generated by `persist_to`.
    pub fn load_from<R: Read>(mut reader: R) -> io::Result<Self> {
        let mut len_bytes = [0u8; 8];
        reader.read_exact(&mut len_bytes)?;
        let len = u64::from_le_bytes(len_bytes) as usize;
        let mut pages = RoaringTreemap::new();
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            let mut page_bytes = [0u8; 8];
            reader.read_exact(&mut page_bytes)?;
            let page = u64::from_le_bytes(page_bytes);
            let mut value_bytes = [0u8; 8];
            reader.read_exact(&mut value_bytes)?;
            let value = DirectoryValue::from_packed(u64::from_le_bytes(value_bytes));
            let inserted = pages.insert(page);
            debug_assert!(inserted, "persisted directory contained duplicate page ids");
            values.push(value);
        }
        Ok(Self { pages, values })
    }

    /// Computes the index of `page_id` in the value array.
    fn index_of(&self, page_id: u64) -> Option<usize> {
        if !self.pages.contains(page_id) {
            return None;
        }
        let rank = self.pages.rank(page_id);
        debug_assert!(rank > 0, "rank of existing element must be > 0");
        Some((rank - 1) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_value_roundtrip() {
        let value = DirectoryValue::new(1024, 0xDEADBEEF, true, 0b101).unwrap();
        assert_eq!(value.length(), 1024);
        assert_eq!(value.checksum(), 0xDEADBEEF);
        assert!(value.is_compressed());
        assert_eq!(value.reserved(), 0b101);
        let packed = value.to_packed();
        assert_eq!(DirectoryValue::from_packed(packed), value);
    }

    #[test]
    fn upsert_and_get() {
        let mut dir = PageDirectory::new();
        let v0 = DirectoryValue::new(64, 1, false, 0).unwrap();
        let v1 = DirectoryValue::new(128, 2, true, 0).unwrap();
        assert!(dir.upsert(42, v0).is_none());
        assert_eq!(dir.get(42), Some(v0));
        assert_eq!(dir.len(), 1);
        assert_eq!(dir.upsert(42, v1), Some(v0));
        assert_eq!(dir.get(42), Some(v1));
    }

    #[test]
    fn remove_entry() {
        let mut dir = PageDirectory::new();
        let value = DirectoryValue::new(512, 3, false, 0).unwrap();
        dir.upsert(7, value);
        assert_eq!(dir.remove(7), Some(value));
        assert!(dir.get(7).is_none());
        assert!(dir.is_empty());
    }

    #[test]
    fn iteration_matches_order() {
        let mut dir = PageDirectory::new();
        for idx in [10u64, 5, 20] {
            let val = DirectoryValue::new(idx as u32, idx as u32, false, 0).unwrap();
            dir.upsert(idx, val);
        }
        let pages: Vec<_> = dir.iter().map(|(page, _)| page).collect();
        assert_eq!(pages, vec![5, 10, 20]);
    }

    #[test]
    fn persist_and_load_roundtrip() {
        let mut dir = PageDirectory::new();
        dir.upsert(1, DirectoryValue::new(32, 5, false, 0).unwrap());
        dir.upsert(8, DirectoryValue::new(64, 6, true, 0).unwrap());
        let mut bytes = Vec::new();
        dir.persist_to(&mut bytes).unwrap();
        let loaded = PageDirectory::load_from(&bytes[..]).unwrap();
        let original: Vec<_> = dir.iter().collect();
        let restored: Vec<_> = loaded.iter().collect();
        assert_eq!(original, restored);
    }
}
