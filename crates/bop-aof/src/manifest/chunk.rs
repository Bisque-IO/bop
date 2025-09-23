use byteorder::{ByteOrder, LittleEndian};
use crc64fast_nvme::Digest;

use crate::error::{AofError, AofResult};

pub const CHUNK_MAGIC: u64 = 0x414F46324D4C4F47; // "AOF2MLOG"
pub const CHUNK_VERSION: u32 = 1;
pub const CHUNK_HEADER_LEN: usize = 64;

#[derive(Debug, Clone)]
pub struct ChunkHeader {
    pub flags: u64,
    pub chunk_index: u64,
    pub base_record: u64,
    pub tail_offset: usize,
    pub committed_len: usize,
    pub chunk_crc64: u64,
}

pub fn crc64(bytes: &[u8]) -> u64 {
    if bytes.is_empty() {
        return 0;
    }
    let mut digest = Digest::new();
    digest.write(bytes);
    digest.sum64()
}

impl ChunkHeader {
    pub const FLAG_SEALED: u64 = 1;

    pub fn new(chunk_index: u64, base_record: u64) -> Self {
        Self {
            flags: 0,
            chunk_index,
            base_record,
            tail_offset: CHUNK_HEADER_LEN,
            committed_len: CHUNK_HEADER_LEN,
            chunk_crc64: 0,
        }
    }

    pub fn write_into(&self, dst: &mut [u8]) {
        LittleEndian::write_u64(&mut dst[0..8], CHUNK_MAGIC);
        LittleEndian::write_u32(&mut dst[8..12], CHUNK_VERSION);
        LittleEndian::write_u32(&mut dst[12..16], CHUNK_HEADER_LEN as u32);
        LittleEndian::write_u64(&mut dst[16..24], self.flags);
        LittleEndian::write_u64(&mut dst[24..32], self.chunk_index);
        LittleEndian::write_u64(&mut dst[32..40], self.base_record);
        LittleEndian::write_u64(&mut dst[40..48], self.tail_offset as u64);
        LittleEndian::write_u64(&mut dst[48..56], self.committed_len as u64);
        LittleEndian::write_u64(&mut dst[56..64], self.chunk_crc64);
    }

    pub fn read_from(src: &[u8]) -> AofResult<Self> {
        if src.len() < CHUNK_HEADER_LEN {
            return Err(AofError::Corruption(
                "manifest chunk header truncated".into(),
            ));
        }
        let magic = LittleEndian::read_u64(&src[0..8]);
        if magic != CHUNK_MAGIC {
            return Err(AofError::Corruption("invalid manifest chunk magic".into()));
        }
        let version = LittleEndian::read_u32(&src[8..12]);
        if version != CHUNK_VERSION {
            return Err(AofError::Corruption(
                "unsupported manifest chunk version".into(),
            ));
        }
        let header_len = LittleEndian::read_u32(&src[12..16]) as usize;
        if header_len != CHUNK_HEADER_LEN {
            return Err(AofError::Corruption(
                "unexpected manifest chunk header len".into(),
            ));
        }
        let flags = LittleEndian::read_u64(&src[16..24]);
        let chunk_index = LittleEndian::read_u64(&src[24..32]);
        let base_record = LittleEndian::read_u64(&src[32..40]);
        let tail_offset = LittleEndian::read_u64(&src[40..48]) as usize;
        let committed_len = LittleEndian::read_u64(&src[48..56]) as usize;
        let chunk_crc64 = LittleEndian::read_u64(&src[56..64]);

        Ok(Self {
            flags,
            chunk_index,
            base_record,
            tail_offset,
            committed_len,
            chunk_crc64,
        })
    }
}
