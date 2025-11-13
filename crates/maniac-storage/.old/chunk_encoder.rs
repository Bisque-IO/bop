use crate::chunk::{LibsqlFrameHeader, RaftFrameMeta};
use crate::directory::DirectoryValue;
use crc64fast_nvme::Digest;

/// Errors produced while encoding chunk records.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChunkEncodeError {
    #[error("payload length {len} exceeds directory value limit")]
    PayloadTooLarge { len: usize },
}

/// Helper responsible for constructing frame headers and directory metadata.
#[derive(Debug, Default)]
pub struct ChunkRecordEncoder;

impl ChunkRecordEncoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a libsql-compatible frame header and directory value for a page payload.
    pub fn encode_libsql(
        &mut self,
        page_no: u32,
        db_size_after_frame: u32,
        payload: &[u8],
        salt: (u32, u32),
        frame_checksum: Option<(u32, u32)>,
        raft: RaftFrameMeta,
    ) -> Result<LibsqlFrameHeader, ChunkEncodeError> {
        let payload_len = payload.len();
        if payload_len > DirectoryValue::MAX_LENGTH as usize {
            return Err(ChunkEncodeError::PayloadTooLarge { len: payload_len });
        }
        let mut hasher = Digest::new();
        hasher.write(payload);
        let crc64 = hasher.sum64();
        let checksum_hi = (crc64 >> 32) as u32;
        let checksum_lo = crc64 as u32;
        let crc32 = checksum_hi ^ checksum_lo;
        let directory_value = DirectoryValue::new(payload_len as u32, crc32, false, 0)
            .expect("validated length fits directory value");
        let checksum_pair = frame_checksum.unwrap_or((checksum_hi, checksum_lo));
        Ok(LibsqlFrameHeader {
            page_no,
            directory_value,
            db_size_after_frame,
            salt: [salt.0, salt.1],
            frame_checksum: [checksum_pair.0, checksum_pair.1],
            payload_len: payload_len as u32,
            raft,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::RaftFrameMeta;

    #[test]
    fn encode_libsql_sets_directory_value() {
        let mut encoder = ChunkRecordEncoder::new();
        let payload = vec![1u8; 32];
        let header = encoder
            .encode_libsql(
                5,
                10,
                &payload,
                (1, 2),
                None,
                RaftFrameMeta { term: 1, index: 2 },
            )
            .expect("encode");
        assert_eq!(header.page_no, 5);
        assert_eq!(header.directory_value.length(), payload.len() as u32);
        assert_eq!(header.payload_len, payload.len() as u32);
    }
}
