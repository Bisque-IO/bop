//! # Vert.x Cluster Manager Wire Protocol
//!
//! This module defines the binary wire protocol for communicating with the Vert.x cluster manager.
//! The protocol is designed for efficiency and cross-platform compatibility.
//!
//! ## Wire Format
//!
//! Messages are framed with the following structure:
//!
//! ```text
//! +----------------+----------------+----------------+----------------+
//! |    checksum    |      size      | protocol_ver |     flags        |
//! |   (4 bytes)    |   (4 bytes)    |   (1 byte)   |   (1 byte)       |
//! +----------------+----------------+----------------+----------------+
//! |  message_kind  |   request_id   |         payload_data            |
//! |   (2 bytes)    |   (4 bytes)    |           (varies)              |
//! +----------------+----------------+---------------------------------+
//! ```
//!
//! - **checksum**: u32 (little-endian) - CRC32 checksum of the entire message (excluding checksum field).
//! - **size**: u32 (little-endian) - Size of the entire message including header.
//! - **protocol_version**: u8 - Current version is 1. Used for compatibility checks.
//! - **flags**: u8 - Message type flags (Request=0x01, Response=0x02, Ping=0x04, Pong=0x08).
//! - **message_kind**: u16 (little-endian) - Message kind identifier for the request/response.
//! - **request_id**: u32 (little-endian) - Unique ID for matching requests and responses.
//! - **payload**: Serialized data using custom binary format (little-endian).
//!
//! ## Serialization
//!
//! - Uses custom binary serialization with little-endian byte order.
//! - Strings are UTF-8 encoded with length prefix (u32).
//! - Vec<u8> uses length prefix (u32).
//! - Enums use discriminant (u8) followed by fields.
//!
//! ## Operations
//!
//! ### Cluster Management
//! - `Join { node_id: String }` -> `Ok` or `Error`
//! - `Leave { node_id: String }` -> `Ok` or `Error`
//! - `GetNodes` -> `Nodes(Vec<String>)`
//!
//! ### AsyncMap
//! - `CreateMap { name: String }` -> `MapId(u64)`
//! - `MapPut { map_id: u64, key: Vec<u8>, value: Vec<u8> }` -> `Ok` or `Error`
//! - `MapGet { map_id: u64, key: Vec<u8> }` -> `Value(Option<Vec<u8>>)` or `Error`
//! - `MapRemove { map_id: u64, key: Vec<u8> }` -> `Ok` or `Error`
//! - `MapSize { map_id: u64 }` -> `Size(usize)` or `Error`
//!
//! ### AsyncSet
//! - `CreateSet { name: String }` -> `SetId(u64)`
//! - `SetAdd { set_id: u64, value: Vec<u8> }` -> `Ok` or `Error`
//! - `SetRemove { set_id: u64, value: Vec<u8> }` -> `Ok` or `Error`
//! - `SetContains { set_id: u64, value: Vec<u8> }` -> `Contains(bool)` or `Error`
//!
//! ### AsyncCounter
//! - `CreateCounter { name: String }` -> `CounterId(u64)`
//! - `CounterGet { counter_id: u64 }` -> `CounterValue(i64)` or `Error`
//! - `CounterIncrement { counter_id: u64, delta: i64 }` -> `Ok` or `Error`
//!
//! ### AsyncLock
//! - `CreateLock { name: String }` -> `LockId(u64)`
//! - `LockAcquire { lock_id: u64 }` -> `Ok` or `Error`
//! - `LockRelease { lock_id: u64 }` -> `Ok` or `Error`
//!
//! ## Usage in Java
//!
//! 1. Serialize a `FramedMessage<Request>` to bytes using the custom format.
//! 2. Send over TCP.
//! 3. Receive response bytes.
//! 4. Deserialize to `FramedMessage<Response>`.
//! 5. Match `request_id` to correlate with the original request.
//!
//! Note: Ensure little-endian byte order when implementing in Java.

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use crc32fast::Hasher;
use std::io::{Cursor, Read};

/// Custom error for wire protocol serialization/deserialization.
#[derive(Debug)]
pub enum WireError {
    Io(std::io::Error),
    InvalidData(String),
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::Io(err) => write!(f, "IO error: {}", err),
            WireError::InvalidData(msg) => write!(f, "Invalid data: {}", msg),
        }
    }
}

impl std::error::Error for WireError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WireError::Io(err) => Some(err),
            WireError::InvalidData(_) => None,
        }
    }
}

impl From<std::io::Error> for WireError {
    fn from(err: std::io::Error) -> Self {
        WireError::Io(err)
    }
}

/// Trait for types that can be serialized to/deserialized from the wire format.
pub trait WireSerializable {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError>;
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError>
    where
        Self: Sized;
}

impl WireSerializable for u8 {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        buf.write_u8(*self)?;
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        Ok(cursor.read_u8()?)
    }
}

impl WireSerializable for u64 {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        buf.write_u64::<LittleEndian>(*self)?;
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        Ok(cursor.read_u64::<LittleEndian>()?)
    }
}

impl WireSerializable for i64 {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        buf.write_i64::<LittleEndian>(*self)?;
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        Ok(cursor.read_i64::<LittleEndian>()?)
    }
}

impl WireSerializable for usize {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        (*self as u64).serialize(buf)
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        Ok(u64::deserialize(cursor)? as usize)
    }
}

impl WireSerializable for String {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        let bytes = self.as_bytes();
        (bytes.len() as u32).serialize(buf)?;
        buf.extend_from_slice(bytes);
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        let len = u32::deserialize(cursor)? as usize;
        let mut bytes = vec![0u8; len];
        cursor.read_exact(&mut bytes)?;
        String::from_utf8(bytes).map_err(|e| WireError::InvalidData(e.to_string()))
    }
}

impl WireSerializable for Vec<String> {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        (self.len() as u32).serialize(buf)?;
        for item in self {
            item.serialize(buf)?;
        }
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        let len = u32::deserialize(cursor)? as usize;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(String::deserialize(cursor)?);
        }
        Ok(vec)
    }
}

impl WireSerializable for Vec<u64> {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        (self.len() as u32).serialize(buf)?;
        for item in self {
            item.serialize(buf)?;
        }
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        let len = u32::deserialize(cursor)? as usize;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(u64::deserialize(cursor)?);
        }
        Ok(vec)
    }
}

impl WireSerializable for Vec<u8> {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        (self.len() as u32).serialize(buf)?;
        buf.extend_from_slice(self);
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        let len = u32::deserialize(cursor)? as usize;
        let mut bytes = vec![0u8; len];
        cursor.read_exact(&mut bytes)?;
        Ok(bytes)
    }
}

impl WireSerializable for Option<Vec<u8>> {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        match self {
            Some(val) => {
                1u8.serialize(buf)?;
                val.serialize(buf)?;
            }
            None => {
                0u8.serialize(buf)?;
            }
        }
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        let tag = u8::deserialize(cursor)?;
        match tag {
            0 => Ok(None),
            1 => Ok(Some(Vec::<u8>::deserialize(cursor)?)),
            _ => Err(WireError::InvalidData("Invalid Option tag".to_string())),
        }
    }
}

impl WireSerializable for u16 {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        buf.write_u16::<LittleEndian>(*self)?;
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        Ok(cursor.read_u16::<LittleEndian>()?)
    }
}

impl WireSerializable for u32 {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        buf.write_u32::<LittleEndian>(*self)?;
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        Ok(cursor.read_u32::<LittleEndian>()?)
    }
}

impl WireSerializable for bool {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        let val = if *self { 1u8 } else { 0u8 };
        val.serialize(buf)
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        let val = u8::deserialize(cursor)?;
        match val {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WireError::InvalidData("Invalid bool value".to_string())),
        }
    }
}

impl WireSerializable for Vec<Vec<u8>> {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        (self.len() as u32).serialize(buf)?;
        for item in self {
            item.serialize(buf)?;
        }
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        let len = u32::deserialize(cursor)? as usize;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(Vec::<u8>::deserialize(cursor)?);
        }
        Ok(vec)
    }
}

impl WireSerializable for (Vec<u8>, Vec<u8>) {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        self.0.serialize(buf)?;
        self.1.serialize(buf)?;
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        Ok((Vec::<u8>::deserialize(cursor)?, Vec::<u8>::deserialize(cursor)?))
    }
}

impl WireSerializable for Vec<(Vec<u8>, Vec<u8>)> {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        (self.len() as u32).serialize(buf)?;
        for item in self {
            item.serialize(buf)?;
        }
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        let len = u32::deserialize(cursor)? as usize;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(<(Vec<u8>, Vec<u8>)>::deserialize(cursor)?);
        }
        Ok(vec)
    }
}

impl WireSerializable for std::collections::HashSet<Vec<u8>> {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        (self.len() as u32).serialize(buf)?;
        for item in self {
            item.serialize(buf)?;
        }
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        let len = u32::deserialize(cursor)? as usize;
        let mut set = std::collections::HashSet::new();
        for _ in 0..len {
            set.insert(Vec::<u8>::deserialize(cursor)?);
        }
        Ok(set)
    }
}

impl WireSerializable for Option<usize> {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        match self {
            Some(val) => {
                1u8.serialize(buf)?;
                val.serialize(buf)?;
            }
            None => {
                0u8.serialize(buf)?;
            }
        }
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        let has_value = u8::deserialize(cursor)?;
        match has_value {
            0 => Ok(None),
            1 => Ok(Some(usize::deserialize(cursor)?)),
            _ => Err(WireError::InvalidData("Invalid Option<usize> value".to_string())),
        }
    }
}

impl WireSerializable for StateSyncOperation {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        match self {
            StateSyncOperation::MapCreated { map_name } => {
                0u8.serialize(buf)?;
                map_name.serialize(buf)?;
            }
            StateSyncOperation::MapUpdated { map_name, key, value } => {
                1u8.serialize(buf)?;
                map_name.serialize(buf)?;
                key.serialize(buf)?;
                value.serialize(buf)?;
            }
            StateSyncOperation::MapDeleted { map_name } => {
                2u8.serialize(buf)?;
                map_name.serialize(buf)?;
            }
            StateSyncOperation::SetCreated { set_name } => {
                3u8.serialize(buf)?;
                set_name.serialize(buf)?;
            }
            StateSyncOperation::SetUpdated { set_name, value, added } => {
                4u8.serialize(buf)?;
                set_name.serialize(buf)?;
                value.serialize(buf)?;
                added.serialize(buf)?;
            }
            StateSyncOperation::SetDeleted { set_name } => {
                5u8.serialize(buf)?;
                set_name.serialize(buf)?;
            }
            StateSyncOperation::CounterCreated { counter_name } => {
                6u8.serialize(buf)?;
                counter_name.serialize(buf)?;
            }
            StateSyncOperation::CounterUpdated { counter_name, value } => {
                7u8.serialize(buf)?;
                counter_name.serialize(buf)?;
                value.serialize(buf)?;
            }
            StateSyncOperation::CounterDeleted { counter_name } => {
                8u8.serialize(buf)?;
                counter_name.serialize(buf)?;
            }
            StateSyncOperation::MultimapCreated { multimap_name } => {
                9u8.serialize(buf)?;
                multimap_name.serialize(buf)?;
            }
            StateSyncOperation::MultimapUpdated { multimap_name, key, value, added } => {
                10u8.serialize(buf)?;
                multimap_name.serialize(buf)?;
                key.serialize(buf)?;
                value.serialize(buf)?;
                added.serialize(buf)?;
            }
            StateSyncOperation::MultimapKeyCleared { multimap_name, key } => {
                11u8.serialize(buf)?;
                multimap_name.serialize(buf)?;
                key.serialize(buf)?;
            }
            StateSyncOperation::MultimapDeleted { multimap_name } => {
                12u8.serialize(buf)?;
                multimap_name.serialize(buf)?;
            }
            StateSyncOperation::LockCreated { lock_name } => {
                13u8.serialize(buf)?;
                lock_name.serialize(buf)?;
            }
            StateSyncOperation::LockAcquired { lock_name, node_id } => {
                14u8.serialize(buf)?;
                lock_name.serialize(buf)?;
                node_id.serialize(buf)?;
            }
            StateSyncOperation::LockReleased { lock_name, node_id } => {
                15u8.serialize(buf)?;
                lock_name.serialize(buf)?;
                node_id.serialize(buf)?;
            }
            StateSyncOperation::LockDeleted { lock_name } => {
                16u8.serialize(buf)?;
                lock_name.serialize(buf)?;
            }
            StateSyncOperation::NodeJoined { node_id } => {
                17u8.serialize(buf)?;
                node_id.serialize(buf)?;
            }
            StateSyncOperation::NodeLeft { node_id } => {
                18u8.serialize(buf)?;
                node_id.serialize(buf)?;
            }
            StateSyncOperation::NodeConnectionAdded { node_id, connection_id } => {
                19u8.serialize(buf)?;
                node_id.serialize(buf)?;
                connection_id.serialize(buf)?;
            }
            StateSyncOperation::NodeConnectionRemoved { node_id, connection_id } => {
                20u8.serialize(buf)?;
                node_id.serialize(buf)?;
                connection_id.serialize(buf)?;
            }
        }
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        let op_type = u8::deserialize(cursor)?;
        match op_type {
            0 => Ok(StateSyncOperation::MapCreated {
                map_name: String::deserialize(cursor)?,
            }),
            1 => Ok(StateSyncOperation::MapUpdated {
                map_name: String::deserialize(cursor)?,
                key: Vec::<u8>::deserialize(cursor)?,
                value: Option::<Vec<u8>>::deserialize(cursor)?,
            }),
            2 => Ok(StateSyncOperation::MapDeleted {
                map_name: String::deserialize(cursor)?,
            }),
            3 => Ok(StateSyncOperation::SetCreated {
                set_name: String::deserialize(cursor)?,
            }),
            4 => Ok(StateSyncOperation::SetUpdated {
                set_name: String::deserialize(cursor)?,
                value: Vec::<u8>::deserialize(cursor)?,
                added: bool::deserialize(cursor)?,
            }),
            5 => Ok(StateSyncOperation::SetDeleted {
                set_name: String::deserialize(cursor)?,
            }),
            6 => Ok(StateSyncOperation::CounterCreated {
                counter_name: String::deserialize(cursor)?,
            }),
            7 => Ok(StateSyncOperation::CounterUpdated {
                counter_name: String::deserialize(cursor)?,
                value: i64::deserialize(cursor)?,
            }),
            8 => Ok(StateSyncOperation::CounterDeleted {
                counter_name: String::deserialize(cursor)?,
            }),
            9 => Ok(StateSyncOperation::MultimapCreated {
                multimap_name: String::deserialize(cursor)?,
            }),
            10 => Ok(StateSyncOperation::MultimapUpdated {
                multimap_name: String::deserialize(cursor)?,
                key: Vec::<u8>::deserialize(cursor)?,
                value: Vec::<u8>::deserialize(cursor)?,
                added: bool::deserialize(cursor)?,
            }),
            11 => Ok(StateSyncOperation::MultimapKeyCleared {
                multimap_name: String::deserialize(cursor)?,
                key: Vec::<u8>::deserialize(cursor)?,
            }),
            12 => Ok(StateSyncOperation::MultimapDeleted {
                multimap_name: String::deserialize(cursor)?,
            }),
            13 => Ok(StateSyncOperation::LockCreated {
                lock_name: String::deserialize(cursor)?,
            }),
            14 => Ok(StateSyncOperation::LockAcquired {
                lock_name: String::deserialize(cursor)?,
                node_id: String::deserialize(cursor)?,
            }),
            15 => Ok(StateSyncOperation::LockReleased {
                lock_name: String::deserialize(cursor)?,
                node_id: String::deserialize(cursor)?,
            }),
            16 => Ok(StateSyncOperation::LockDeleted {
                lock_name: String::deserialize(cursor)?,
            }),
            17 => Ok(StateSyncOperation::NodeJoined {
                node_id: String::deserialize(cursor)?,
            }),
            18 => Ok(StateSyncOperation::NodeLeft {
                node_id: String::deserialize(cursor)?,
            }),
            19 => Ok(StateSyncOperation::NodeConnectionAdded {
                node_id: String::deserialize(cursor)?,
                connection_id: u64::deserialize(cursor)?,
            }),
            20 => Ok(StateSyncOperation::NodeConnectionRemoved {
                node_id: String::deserialize(cursor)?,
                connection_id: u64::deserialize(cursor)?,
            }),
            _ => Err(WireError::InvalidData(format!("Unknown StateSyncOperation type: {}", op_type))),
        }
    }
}

impl WireSerializable for Vec<StateSyncOperation> {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        (self.len() as u32).serialize(buf)?;
        for item in self {
            item.serialize(buf)?;
        }
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        let len = u32::deserialize(cursor)? as usize;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(StateSyncOperation::deserialize(cursor)?);
        }
        Ok(vec)
    }
}

impl WireSerializable for std::collections::HashMap<Vec<u8>, std::collections::HashSet<Vec<u8>>> {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        (self.len() as u32).serialize(buf)?;
        for (key, value_set) in self {
            key.serialize(buf)?;
            value_set.serialize(buf)?;
        }
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        let len = u32::deserialize(cursor)? as usize;
        let mut map = std::collections::HashMap::new();
        for _ in 0..len {
            let key = Vec::<u8>::deserialize(cursor)?;
            let value_set = std::collections::HashSet::<Vec<u8>>::deserialize(cursor)?;
            map.insert(key, value_set);
        }
        Ok(map)
    }
}

impl WireSerializable for ResponseCode {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        let val = match self {
            ResponseCode::OK => 0u8,
            ResponseCode::ERR => 1u8,
        };
        val.serialize(buf)
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        let val = u8::deserialize(cursor)?;
        match val {
            0 => Ok(ResponseCode::OK),
            1 => Ok(ResponseCode::ERR),
            _ => Err(WireError::InvalidData(format!("Invalid ResponseCode: {}", val))),
        }
    }
}

pub type RequestId = u32;
pub type MessageKind = u16;

/// Message type flags
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MessageFlags(u8);

impl MessageFlags {
    pub const REQUEST: MessageFlags = MessageFlags(0x01);
    pub const RESPONSE: MessageFlags = MessageFlags(0x02);
    pub const PING: MessageFlags = MessageFlags(0x04);
    pub const PONG: MessageFlags = MessageFlags(0x08);
    
    pub fn new(value: u8) -> Self {
        MessageFlags(value)
    }
    
    pub fn as_u8(&self) -> u8 {
        self.0
    }
    
    pub fn is_request(&self) -> bool {
        self.0 & Self::REQUEST.0 != 0
    }
    
    pub fn is_response(&self) -> bool {
        self.0 & Self::RESPONSE.0 != 0
    }
    
    pub fn is_ping(&self) -> bool {
        self.0 & Self::PING.0 != 0
    }
    
    pub fn is_pong(&self) -> bool {
        self.0 & Self::PONG.0 != 0
    }
    
    pub fn contains(&self, flag: MessageFlags) -> bool {
        self.0 & flag.0 != 0
    }
}

/// Protocol version for compatibility.
/// Increment when making breaking changes.
pub const PROTOCOL_VERSION: u8 = 1;

// Message-based message kind constants
pub const MSG_JOIN_REQUEST: MessageKind = 1;
pub const MSG_JOIN_RESPONSE: MessageKind = 2;
pub const MSG_LEAVE_REQUEST: MessageKind = 3;
pub const MSG_LEAVE_RESPONSE: MessageKind = 4;
pub const MSG_GET_NODES_REQUEST: MessageKind = 5;
pub const MSG_GET_NODES_RESPONSE: MessageKind = 6;
pub const MSG_CREATE_MAP_REQUEST: MessageKind = 7;
pub const MSG_CREATE_MAP_RESPONSE: MessageKind = 8;
pub const MSG_MAP_PUT_REQUEST: MessageKind = 9;
pub const MSG_MAP_PUT_RESPONSE: MessageKind = 10;
pub const MSG_MAP_GET_REQUEST: MessageKind = 11;
pub const MSG_MAP_GET_RESPONSE: MessageKind = 12;
pub const MSG_MAP_REMOVE_REQUEST: MessageKind = 13;
pub const MSG_MAP_REMOVE_RESPONSE: MessageKind = 14;
pub const MSG_MAP_SIZE_REQUEST: MessageKind = 15;
pub const MSG_MAP_SIZE_RESPONSE: MessageKind = 16;
pub const MSG_CREATE_SET_REQUEST: MessageKind = 17;
pub const MSG_CREATE_SET_RESPONSE: MessageKind = 18;
pub const MSG_SET_ADD_REQUEST: MessageKind = 19;
pub const MSG_SET_ADD_RESPONSE: MessageKind = 20;
pub const MSG_SET_REMOVE_REQUEST: MessageKind = 21;
pub const MSG_SET_REMOVE_RESPONSE: MessageKind = 22;
pub const MSG_SET_CONTAINS_REQUEST: MessageKind = 23;
pub const MSG_SET_CONTAINS_RESPONSE: MessageKind = 24;
pub const MSG_SET_SIZE_REQUEST: MessageKind = 25;
pub const MSG_SET_SIZE_RESPONSE: MessageKind = 26;
pub const MSG_CREATE_COUNTER_REQUEST: MessageKind = 27;
pub const MSG_CREATE_COUNTER_RESPONSE: MessageKind = 28;
pub const MSG_COUNTER_GET_REQUEST: MessageKind = 29;
pub const MSG_COUNTER_GET_RESPONSE: MessageKind = 30;
pub const MSG_COUNTER_INCREMENT_REQUEST: MessageKind = 31;
pub const MSG_COUNTER_INCREMENT_RESPONSE: MessageKind = 32;
pub const MSG_CREATE_LOCK_REQUEST: MessageKind = 33;
pub const MSG_CREATE_LOCK_RESPONSE: MessageKind = 34;
pub const MSG_LOCK_ACQUIRE_REQUEST: MessageKind = 35;
pub const MSG_LOCK_ACQUIRE_RESPONSE: MessageKind = 36;
pub const MSG_LOCK_RELEASE_REQUEST: MessageKind = 37;
pub const MSG_LOCK_RELEASE_RESPONSE: MessageKind = 38;
pub const MSG_LOCK_SYNC_REQUEST: MessageKind = 39;
pub const MSG_LOCK_SYNC_RESPONSE: MessageKind = 40;
pub const MSG_CREATE_MULTIMAP_REQUEST: MessageKind = 41;
pub const MSG_CREATE_MULTIMAP_RESPONSE: MessageKind = 42;
pub const MSG_MULTIMAP_PUT_REQUEST: MessageKind = 43;
pub const MSG_MULTIMAP_PUT_RESPONSE: MessageKind = 44;
pub const MSG_MULTIMAP_GET_REQUEST: MessageKind = 45;
pub const MSG_MULTIMAP_GET_RESPONSE: MessageKind = 46;
pub const MSG_MULTIMAP_REMOVE_REQUEST: MessageKind = 47;
pub const MSG_MULTIMAP_REMOVE_RESPONSE: MessageKind = 48;
pub const MSG_MULTIMAP_REMOVE_VALUE_REQUEST: MessageKind = 49;
pub const MSG_MULTIMAP_REMOVE_VALUE_RESPONSE: MessageKind = 50;
pub const MSG_MULTIMAP_SIZE_REQUEST: MessageKind = 51;
pub const MSG_MULTIMAP_SIZE_RESPONSE: MessageKind = 52;
pub const MSG_MULTIMAP_KEY_SIZE_REQUEST: MessageKind = 53;
pub const MSG_MULTIMAP_KEY_SIZE_RESPONSE: MessageKind = 54;
pub const MSG_MULTIMAP_CONTAINS_KEY_REQUEST: MessageKind = 55;
pub const MSG_MULTIMAP_CONTAINS_KEY_RESPONSE: MessageKind = 56;
pub const MSG_MULTIMAP_CONTAINS_VALUE_REQUEST: MessageKind = 57;
pub const MSG_MULTIMAP_CONTAINS_VALUE_RESPONSE: MessageKind = 58;
pub const MSG_MULTIMAP_CONTAINS_ENTRY_REQUEST: MessageKind = 59;
pub const MSG_MULTIMAP_CONTAINS_ENTRY_RESPONSE: MessageKind = 60;
pub const MSG_MULTIMAP_KEYS_REQUEST: MessageKind = 61;
pub const MSG_MULTIMAP_KEYS_RESPONSE: MessageKind = 62;
pub const MSG_MULTIMAP_VALUES_REQUEST: MessageKind = 63;
pub const MSG_MULTIMAP_VALUES_RESPONSE: MessageKind = 64;
pub const MSG_MULTIMAP_ENTRIES_REQUEST: MessageKind = 65;
pub const MSG_MULTIMAP_ENTRIES_RESPONSE: MessageKind = 66;
pub const MSG_MULTIMAP_CLEAR_REQUEST: MessageKind = 67;
pub const MSG_MULTIMAP_CLEAR_RESPONSE: MessageKind = 68;
pub const MSG_ERROR_RESPONSE: MessageKind = 69;
pub const MSG_LEADER_REDIRECT_RESPONSE: MessageKind = 70;
pub const MSG_PING: MessageKind = 71;
pub const MSG_PONG: MessageKind = 72;
pub const MSG_STATE_SYNC: MessageKind = 73;
pub const MSG_STATE_SYNC_ACK: MessageKind = 74;

/// Represents a single state synchronization operation
#[derive(Debug, Clone, PartialEq)]
pub enum StateSyncOperation {
    // Map operations
    MapCreated { map_name: String },
    MapUpdated { map_name: String, key: Vec<u8>, value: Option<Vec<u8>> }, // None = deletion
    MapDeleted { map_name: String },
    
    // Set operations
    SetCreated { set_name: String },
    SetUpdated { set_name: String, value: Vec<u8>, added: bool }, // added=true for add, false for remove
    SetDeleted { set_name: String },
    
    // Counter operations
    CounterCreated { counter_name: String },
    CounterUpdated { counter_name: String, value: i64 },
    CounterDeleted { counter_name: String },
    
    // Multimap operations
    MultimapCreated { multimap_name: String },
    MultimapUpdated { multimap_name: String, key: Vec<u8>, value: Vec<u8>, added: bool },
    MultimapKeyCleared { multimap_name: String, key: Vec<u8> },
    MultimapDeleted { multimap_name: String },
    
    // Lock operations
    LockCreated { lock_name: String },
    LockAcquired { lock_name: String, node_id: String },
    LockReleased { lock_name: String, node_id: String },
    LockDeleted { lock_name: String },
    
    // Node operations
    NodeJoined { node_id: String },
    NodeLeft { node_id: String },
    NodeConnectionAdded { node_id: String, connection_id: u64 },
    NodeConnectionRemoved { node_id: String, connection_id: u64 },
}

/// Response code for all response messages
#[derive(Debug, Clone, PartialEq)]
pub enum ResponseCode {
    OK,
    ERR,
}

/// A unified message type for the Vert.x 3 cluster manager wire protocol.
/// Each request has a corresponding specific response type, eliminating ambiguity.
/// Uses String-based identifiers instead of u64 for true Vert.x compatibility.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    // Cluster management
    JoinRequest { node_id: String },
    JoinResponse { code: ResponseCode, connection_id: u64 },
    LeaveRequest { node_id: String },
    LeaveResponse { code: ResponseCode },
    GetNodesRequest,
    GetNodesResponse { code: ResponseCode, nodes: Vec<String> },

    // AsyncMap operations  
    CreateMapRequest { name: String },
    CreateMapResponse { code: ResponseCode, map_name: String },
    MapPutRequest { map_name: String, key: Vec<u8>, value: Vec<u8> },
    MapPutResponse { code: ResponseCode },
    MapGetRequest { map_name: String, key: Vec<u8> },
    MapGetResponse { code: ResponseCode, value: Option<Vec<u8>> },
    MapRemoveRequest { map_name: String, key: Vec<u8> },
    MapRemoveResponse { code: ResponseCode },
    MapSizeRequest { map_name: String },
    MapSizeResponse { code: ResponseCode, size: usize },

    // AsyncSet operations
    CreateSetRequest { name: String },
    CreateSetResponse { code: ResponseCode, set_name: String },
    SetAddRequest { set_name: String, value: Vec<u8> },
    SetAddResponse { code: ResponseCode },
    SetRemoveRequest { set_name: String, value: Vec<u8> },
    SetRemoveResponse { code: ResponseCode },
    SetContainsRequest { set_name: String, value: Vec<u8> },
    SetContainsResponse { code: ResponseCode, contains: bool },
    SetSizeRequest { set_name: String },
    SetSizeResponse { code: ResponseCode, size: usize },

    // AsyncCounter operations
    CreateCounterRequest { name: String },
    CreateCounterResponse { code: ResponseCode, counter_name: String },
    CounterGetRequest { counter_name: String },
    CounterGetResponse { code: ResponseCode, value: i64 },
    CounterIncrementRequest { counter_name: String, delta: i64 },
    CounterIncrementResponse { code: ResponseCode, new_value: i64 },

    // AsyncLock operations
    CreateLockRequest { name: String },
    CreateLockResponse { code: ResponseCode, lock_name: String },
    LockAcquireRequest { lock_name: String },
    LockAcquireResponse { code: ResponseCode, acquired: bool, queue_position: Option<usize> },
    LockReleaseRequest { lock_name: String },
    LockReleaseResponse { code: ResponseCode },
    LockSyncRequest { held_locks: Vec<String> },
    LockSyncResponse { code: ResponseCode, synchronized_locks: Vec<String>, failed_locks: Vec<String> },

    // AsyncMultimap operations
    CreateMultimapRequest { name: String },
    CreateMultimapResponse { code: ResponseCode, multimap_name: String },
    MultimapPutRequest { multimap_name: String, key: Vec<u8>, value: Vec<u8> },
    MultimapPutResponse { code: ResponseCode },
    MultimapGetRequest { multimap_name: String, key: Vec<u8> },
    MultimapGetResponse { code: ResponseCode, values: std::collections::HashSet<Vec<u8>> },
    MultimapRemoveRequest { multimap_name: String, key: Vec<u8> },
    MultimapRemoveResponse { code: ResponseCode },
    MultimapRemoveValueRequest { multimap_name: String, key: Vec<u8>, value: Vec<u8> },
    MultimapRemoveValueResponse { code: ResponseCode },
    MultimapSizeRequest { multimap_name: String },
    MultimapSizeResponse { code: ResponseCode, size: usize },
    MultimapKeySizeRequest { multimap_name: String, key: Vec<u8> },
    MultimapKeySizeResponse { code: ResponseCode, size: usize },
    MultimapContainsKeyRequest { multimap_name: String, key: Vec<u8> },
    MultimapContainsKeyResponse { code: ResponseCode, contains: bool },
    MultimapContainsValueRequest { multimap_name: String, value: Vec<u8> },
    MultimapContainsValueResponse { code: ResponseCode, contains: bool },
    MultimapContainsEntryRequest { multimap_name: String, key: Vec<u8>, value: Vec<u8> },
    MultimapContainsEntryResponse { code: ResponseCode, contains: bool },
    MultimapKeysRequest { multimap_name: String },
    MultimapKeysResponse { code: ResponseCode, keys: std::collections::HashSet<Vec<u8>> },
    MultimapValuesRequest { multimap_name: String },
    MultimapValuesResponse { code: ResponseCode, values: Vec<Vec<u8>> },
    MultimapEntriesRequest { multimap_name: String },
    MultimapEntriesResponse { code: ResponseCode, entries: std::collections::HashMap<Vec<u8>, std::collections::HashSet<Vec<u8>>> },
    MultimapClearRequest { multimap_name: String },
    MultimapClearResponse { code: ResponseCode },

    // Error and control messages
    ErrorResponse { code: ResponseCode, error: String },
    LeaderRedirectResponse { code: ResponseCode, leader_address: String },
    
    // Ping/Pong for heartbeat
    Ping,
    Pong,

    // Synchronization messages for client-side caching
    StateSync { sync_id: u64, operations: Vec<StateSyncOperation> },
    StateSyncAck { code: ResponseCode, sync_id: u64 },
}

impl Message {
    /// Get the action ID for this message type
    pub fn get_message_kind(&self) -> MessageKind {
        match self {
            Message::JoinRequest { .. } => MSG_JOIN_REQUEST,
            Message::JoinResponse { .. } => MSG_JOIN_RESPONSE,
            Message::LeaveRequest { .. } => MSG_LEAVE_REQUEST,
            Message::LeaveResponse { .. } => MSG_LEAVE_RESPONSE,
            Message::GetNodesRequest => MSG_GET_NODES_REQUEST,
            Message::GetNodesResponse { .. } => MSG_GET_NODES_RESPONSE,
            Message::CreateMapRequest { .. } => MSG_CREATE_MAP_REQUEST,
            Message::CreateMapResponse { .. } => MSG_CREATE_MAP_RESPONSE,
            Message::MapPutRequest { .. } => MSG_MAP_PUT_REQUEST,
            Message::MapPutResponse { .. } => MSG_MAP_PUT_RESPONSE,
            Message::MapGetRequest { .. } => MSG_MAP_GET_REQUEST,
            Message::MapGetResponse { .. } => MSG_MAP_GET_RESPONSE,
            Message::MapRemoveRequest { .. } => MSG_MAP_REMOVE_REQUEST,
            Message::MapRemoveResponse { .. } => MSG_MAP_REMOVE_RESPONSE,
            Message::MapSizeRequest { .. } => MSG_MAP_SIZE_REQUEST,
            Message::MapSizeResponse { .. } => MSG_MAP_SIZE_RESPONSE,
            Message::CreateSetRequest { .. } => MSG_CREATE_SET_REQUEST,
            Message::CreateSetResponse { .. } => MSG_CREATE_SET_RESPONSE,
            Message::SetAddRequest { .. } => MSG_SET_ADD_REQUEST,
            Message::SetAddResponse { .. } => MSG_SET_ADD_RESPONSE,
            Message::SetRemoveRequest { .. } => MSG_SET_REMOVE_REQUEST,
            Message::SetRemoveResponse { .. } => MSG_SET_REMOVE_RESPONSE,
            Message::SetContainsRequest { .. } => MSG_SET_CONTAINS_REQUEST,
            Message::SetContainsResponse { .. } => MSG_SET_CONTAINS_RESPONSE,
            Message::SetSizeRequest { .. } => MSG_SET_SIZE_REQUEST,
            Message::SetSizeResponse { .. } => MSG_SET_SIZE_RESPONSE,
            Message::CreateCounterRequest { .. } => MSG_CREATE_COUNTER_REQUEST,
            Message::CreateCounterResponse { .. } => MSG_CREATE_COUNTER_RESPONSE,
            Message::CounterGetRequest { .. } => MSG_COUNTER_GET_REQUEST,
            Message::CounterGetResponse { .. } => MSG_COUNTER_GET_RESPONSE,
            Message::CounterIncrementRequest { .. } => MSG_COUNTER_INCREMENT_REQUEST,
            Message::CounterIncrementResponse { .. } => MSG_COUNTER_INCREMENT_RESPONSE,
            Message::CreateLockRequest { .. } => MSG_CREATE_LOCK_REQUEST,
            Message::CreateLockResponse { .. } => MSG_CREATE_LOCK_RESPONSE,
            Message::LockAcquireRequest { .. } => MSG_LOCK_ACQUIRE_REQUEST,
            Message::LockAcquireResponse { .. } => MSG_LOCK_ACQUIRE_RESPONSE,
            Message::LockReleaseRequest { .. } => MSG_LOCK_RELEASE_REQUEST,
            Message::LockReleaseResponse { .. } => MSG_LOCK_RELEASE_RESPONSE,
            Message::LockSyncRequest { .. } => MSG_LOCK_SYNC_REQUEST,
            Message::LockSyncResponse { .. } => MSG_LOCK_SYNC_RESPONSE,
            Message::CreateMultimapRequest { .. } => MSG_CREATE_MULTIMAP_REQUEST,
            Message::CreateMultimapResponse { .. } => MSG_CREATE_MULTIMAP_RESPONSE,
            Message::MultimapPutRequest { .. } => MSG_MULTIMAP_PUT_REQUEST,
            Message::MultimapPutResponse { .. } => MSG_MULTIMAP_PUT_RESPONSE,
            Message::MultimapGetRequest { .. } => MSG_MULTIMAP_GET_REQUEST,
            Message::MultimapGetResponse { .. } => MSG_MULTIMAP_GET_RESPONSE,
            Message::MultimapRemoveRequest { .. } => MSG_MULTIMAP_REMOVE_REQUEST,
            Message::MultimapRemoveResponse { .. } => MSG_MULTIMAP_REMOVE_RESPONSE,
            Message::MultimapRemoveValueRequest { .. } => MSG_MULTIMAP_REMOVE_VALUE_REQUEST,
            Message::MultimapRemoveValueResponse { .. } => MSG_MULTIMAP_REMOVE_VALUE_RESPONSE,
            Message::MultimapSizeRequest { .. } => MSG_MULTIMAP_SIZE_REQUEST,
            Message::MultimapSizeResponse { .. } => MSG_MULTIMAP_SIZE_RESPONSE,
            Message::MultimapKeySizeRequest { .. } => MSG_MULTIMAP_KEY_SIZE_REQUEST,
            Message::MultimapKeySizeResponse { .. } => MSG_MULTIMAP_KEY_SIZE_RESPONSE,
            Message::MultimapContainsKeyRequest { .. } => MSG_MULTIMAP_CONTAINS_KEY_REQUEST,
            Message::MultimapContainsKeyResponse { .. } => MSG_MULTIMAP_CONTAINS_KEY_RESPONSE,
            Message::MultimapContainsValueRequest { .. } => MSG_MULTIMAP_CONTAINS_VALUE_REQUEST,
            Message::MultimapContainsValueResponse { .. } => MSG_MULTIMAP_CONTAINS_VALUE_RESPONSE,
            Message::MultimapContainsEntryRequest { .. } => MSG_MULTIMAP_CONTAINS_ENTRY_REQUEST,
            Message::MultimapContainsEntryResponse { .. } => MSG_MULTIMAP_CONTAINS_ENTRY_RESPONSE,
            Message::MultimapKeysRequest { .. } => MSG_MULTIMAP_KEYS_REQUEST,
            Message::MultimapKeysResponse { .. } => MSG_MULTIMAP_KEYS_RESPONSE,
            Message::MultimapValuesRequest { .. } => MSG_MULTIMAP_VALUES_REQUEST,
            Message::MultimapValuesResponse { .. } => MSG_MULTIMAP_VALUES_RESPONSE,
            Message::MultimapEntriesRequest { .. } => MSG_MULTIMAP_ENTRIES_REQUEST,
            Message::MultimapEntriesResponse { .. } => MSG_MULTIMAP_ENTRIES_RESPONSE,
            Message::MultimapClearRequest { .. } => MSG_MULTIMAP_CLEAR_REQUEST,
            Message::MultimapClearResponse { .. } => MSG_MULTIMAP_CLEAR_RESPONSE,
            Message::ErrorResponse { .. } => MSG_ERROR_RESPONSE,
            Message::LeaderRedirectResponse { .. } => MSG_LEADER_REDIRECT_RESPONSE,
            Message::Ping => MSG_PING,
            Message::Pong => MSG_PONG,
            Message::StateSync { .. } => MSG_STATE_SYNC,
            Message::StateSyncAck { .. } => MSG_STATE_SYNC_ACK,
        }
    }
    
    /// Check if this is a request message (as opposed to a response)
    pub fn is_request(&self) -> bool {
        matches!(self,
            Message::JoinRequest { .. } |
            Message::LeaveRequest { .. } |
            Message::GetNodesRequest |
            Message::CreateMapRequest { .. } |
            Message::MapPutRequest { .. } |
            Message::MapGetRequest { .. } |
            Message::MapRemoveRequest { .. } |
            Message::MapSizeRequest { .. } |
            Message::CreateSetRequest { .. } |
            Message::SetAddRequest { .. } |
            Message::SetRemoveRequest { .. } |
            Message::SetContainsRequest { .. } |
            Message::SetSizeRequest { .. } |
            Message::CreateCounterRequest { .. } |
            Message::CounterGetRequest { .. } |
            Message::CounterIncrementRequest { .. } |
            Message::CreateLockRequest { .. } |
            Message::LockAcquireRequest { .. } |
            Message::LockReleaseRequest { .. } |
            Message::LockSyncRequest { .. } |
            Message::CreateMultimapRequest { .. } |
            Message::MultimapPutRequest { .. } |
            Message::MultimapGetRequest { .. } |
            Message::MultimapRemoveRequest { .. } |
            Message::MultimapRemoveValueRequest { .. } |
            Message::MultimapSizeRequest { .. } |
            Message::MultimapKeySizeRequest { .. } |
            Message::MultimapContainsKeyRequest { .. } |
            Message::MultimapContainsValueRequest { .. } |
            Message::MultimapContainsEntryRequest { .. } |
            Message::MultimapKeysRequest { .. } |
            Message::MultimapValuesRequest { .. } |
            Message::MultimapEntriesRequest { .. } |
            Message::MultimapClearRequest { .. } |
            Message::Ping
        )
    }
}

impl WireSerializable for Message {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        // Serialize based on message type
        match self {
            Message::JoinRequest { node_id } => {
                node_id.serialize(buf)?;
            }
            Message::JoinResponse { code, connection_id } => {
                code.serialize(buf)?;
                connection_id.serialize(buf)?;
            }
            Message::LeaveRequest { node_id } => {
                node_id.serialize(buf)?;
            }
            Message::LeaveResponse { code } => {
                code.serialize(buf)?;
            }
            Message::GetNodesRequest => {
                // No data to serialize
            }
            Message::GetNodesResponse { code, nodes } => {
                code.serialize(buf)?;
                nodes.serialize(buf)?;
            }
            // Map operations
            Message::CreateMapRequest { name } => {
                name.serialize(buf)?;
            }
            Message::CreateMapResponse { code, map_name } => {
                code.serialize(buf)?;
                map_name.serialize(buf)?;
            }
            Message::MapPutRequest { map_name, key, value } => {
                map_name.serialize(buf)?;
                key.serialize(buf)?;
                value.serialize(buf)?;
            }
            Message::MapPutResponse { code } => {
                code.serialize(buf)?;
            }
            Message::MapGetRequest { map_name, key } => {
                map_name.serialize(buf)?;
                key.serialize(buf)?;
            }
            Message::MapGetResponse { code, value } => {
                code.serialize(buf)?;
                value.serialize(buf)?;
            }
            Message::MapRemoveRequest { map_name, key } => {
                map_name.serialize(buf)?;
                key.serialize(buf)?;
            }
            Message::MapRemoveResponse { code } => {
                code.serialize(buf)?;
            }
            Message::MapSizeRequest { map_name } => {
                map_name.serialize(buf)?;
            }
            Message::MapSizeResponse { code, size } => {
                code.serialize(buf)?;
                size.serialize(buf)?;
            }
            // Counter operations
            Message::CreateCounterRequest { name } => {
                name.serialize(buf)?;
            }
            Message::CreateCounterResponse { code, counter_name } => {
                code.serialize(buf)?;
                counter_name.serialize(buf)?;
            }
            Message::CounterGetRequest { counter_name } => {
                counter_name.serialize(buf)?;
            }
            Message::CounterGetResponse { code, value } => {
                code.serialize(buf)?;
                value.serialize(buf)?;
            }
            Message::CounterIncrementRequest { counter_name, delta } => {
                counter_name.serialize(buf)?;
                delta.serialize(buf)?;
            }
            Message::CounterIncrementResponse { code, new_value } => {
                code.serialize(buf)?;
                new_value.serialize(buf)?;
            }
            // Lock operations
            Message::CreateLockRequest { name } => {
                name.serialize(buf)?;
            }
            Message::CreateLockResponse { code, lock_name } => {
                code.serialize(buf)?;
                lock_name.serialize(buf)?;
            }
            Message::LockAcquireRequest { lock_name } => {
                lock_name.serialize(buf)?;
            }
            Message::LockAcquireResponse { code, acquired, queue_position } => {
                code.serialize(buf)?;
                acquired.serialize(buf)?;
                queue_position.serialize(buf)?;
            }
            Message::LockReleaseRequest { lock_name } => {
                lock_name.serialize(buf)?;
            }
            Message::LockReleaseResponse { code } => {
                code.serialize(buf)?;
            }
            Message::LockSyncRequest { held_locks } => {
                held_locks.serialize(buf)?;
            }
            Message::LockSyncResponse { code, synchronized_locks, failed_locks } => {
                code.serialize(buf)?;
                synchronized_locks.serialize(buf)?;
                failed_locks.serialize(buf)?;
            }
            // Error and control messages
            Message::ErrorResponse { code, error } => {
                code.serialize(buf)?;
                error.serialize(buf)?;
            }
            Message::LeaderRedirectResponse { code, leader_address } => {
                code.serialize(buf)?;
                leader_address.serialize(buf)?;
            }
            Message::Ping | Message::Pong => {
                // No data to serialize
            }
            Message::StateSync { sync_id, operations } => {
                sync_id.serialize(buf)?;
                operations.serialize(buf)?;
            }
            Message::StateSyncAck { code, sync_id } => {
                code.serialize(buf)?;
                sync_id.serialize(buf)?;
            }
            // Set operations
            Message::CreateSetRequest { name } => {
                name.serialize(buf)?;
            }
            Message::CreateSetResponse { code, set_name } => {
                code.serialize(buf)?;
                set_name.serialize(buf)?;
            }
            Message::SetAddRequest { set_name, value } => {
                set_name.serialize(buf)?;
                value.serialize(buf)?;
            }
            Message::SetAddResponse { code } => {
                code.serialize(buf)?;
            }
            Message::SetRemoveRequest { set_name, value } => {
                set_name.serialize(buf)?;
                value.serialize(buf)?;
            }
            Message::SetRemoveResponse { code } => {
                code.serialize(buf)?;
            }
            Message::SetContainsRequest { set_name, value } => {
                set_name.serialize(buf)?;
                value.serialize(buf)?;
            }
            Message::SetContainsResponse { code, contains } => {
                code.serialize(buf)?;
                contains.serialize(buf)?;
            }
            Message::SetSizeRequest { set_name } => {
                set_name.serialize(buf)?;
            }
            Message::SetSizeResponse { code, size } => {
                code.serialize(buf)?;
                size.serialize(buf)?;
            }
            // Multimap operations - adding the missing ones
            Message::CreateMultimapRequest { name } => {
                name.serialize(buf)?;
            }
            Message::CreateMultimapResponse { code, multimap_name } => {
                code.serialize(buf)?;
                multimap_name.serialize(buf)?;
            }
            Message::MultimapPutRequest { multimap_name, key, value } => {
                multimap_name.serialize(buf)?;
                key.serialize(buf)?;
                value.serialize(buf)?;
            }
            Message::MultimapPutResponse { code } => {
                code.serialize(buf)?;
            }
            Message::MultimapGetRequest { multimap_name, key } => {
                multimap_name.serialize(buf)?;
                key.serialize(buf)?;
            }
            Message::MultimapGetResponse { code, values } => {
                code.serialize(buf)?;
                values.serialize(buf)?;
            }
            Message::MultimapRemoveRequest { multimap_name, key } => {
                multimap_name.serialize(buf)?;
                key.serialize(buf)?;
            }
            Message::MultimapRemoveResponse { code } => {
                code.serialize(buf)?;
            }
            Message::MultimapRemoveValueRequest { multimap_name, key, value } => {
                multimap_name.serialize(buf)?;
                key.serialize(buf)?;
                value.serialize(buf)?;
            }
            Message::MultimapRemoveValueResponse { code } => {
                code.serialize(buf)?;
            }
            Message::MultimapSizeRequest { multimap_name } => {
                multimap_name.serialize(buf)?;
            }
            Message::MultimapSizeResponse { code, size } => {
                code.serialize(buf)?;
                size.serialize(buf)?;
            }
            Message::MultimapKeySizeRequest { multimap_name, key } => {
                multimap_name.serialize(buf)?;
                key.serialize(buf)?;
            }
            Message::MultimapKeySizeResponse { code, size } => {
                code.serialize(buf)?;
                size.serialize(buf)?;
            }
            Message::MultimapContainsKeyRequest { multimap_name, key } => {
                multimap_name.serialize(buf)?;
                key.serialize(buf)?;
            }
            Message::MultimapContainsKeyResponse { code, contains } => {
                code.serialize(buf)?;
                contains.serialize(buf)?;
            }
            Message::MultimapContainsValueRequest { multimap_name, value } => {
                multimap_name.serialize(buf)?;
                value.serialize(buf)?;
            }
            Message::MultimapContainsValueResponse { code, contains } => {
                code.serialize(buf)?;
                contains.serialize(buf)?;
            }
            Message::MultimapContainsEntryRequest { multimap_name, key, value } => {
                multimap_name.serialize(buf)?;
                key.serialize(buf)?;
                value.serialize(buf)?;
            }
            Message::MultimapContainsEntryResponse { code, contains } => {
                code.serialize(buf)?;
                contains.serialize(buf)?;
            }
            Message::MultimapKeysRequest { multimap_name } => {
                multimap_name.serialize(buf)?;
            }
            Message::MultimapKeysResponse { code, keys } => {
                code.serialize(buf)?;
                keys.serialize(buf)?;
            }
            Message::MultimapValuesRequest { multimap_name } => {
                multimap_name.serialize(buf)?;
            }
            Message::MultimapValuesResponse { code, values } => {
                code.serialize(buf)?;
                values.serialize(buf)?;
            }
            Message::MultimapEntriesRequest { multimap_name } => {
                multimap_name.serialize(buf)?;
            }
            Message::MultimapEntriesResponse { code, entries } => {
                code.serialize(buf)?;
                entries.serialize(buf)?;
            }
            Message::MultimapClearRequest { multimap_name } => {
                multimap_name.serialize(buf)?;
            }
            Message::MultimapClearResponse { code } => {
                code.serialize(buf)?;
            }
        }
        Ok(())
    }
    
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        // This method would need the action_id to know which variant to deserialize
        // For now, we'll implement it in a separate function that takes action_id
        Err(WireError::InvalidData(
            "Message deserialization requires action_id parameter".to_string()
        ))
    }
}

impl Message {
    /// Deserialize a message with the given action ID
    pub fn deserialize_with_message_kind(cursor: &mut Cursor<&[u8]>, message_kind: MessageKind) -> Result<Self, WireError> {
        match message_kind {
            MSG_JOIN_REQUEST => Ok(Message::JoinRequest {
                node_id: String::deserialize(cursor)?,
            }),
            MSG_JOIN_RESPONSE => Ok(Message::JoinResponse {
                code: ResponseCode::deserialize(cursor)?,
                connection_id: u64::deserialize(cursor)?,
            }),
            MSG_LEAVE_REQUEST => Ok(Message::LeaveRequest {
                node_id: String::deserialize(cursor)?,
            }),
            MSG_LEAVE_RESPONSE => Ok(Message::LeaveResponse {
                code: ResponseCode::deserialize(cursor)?,
            }),
            MSG_GET_NODES_REQUEST => Ok(Message::GetNodesRequest),
            MSG_GET_NODES_RESPONSE => Ok(Message::GetNodesResponse {
                code: ResponseCode::deserialize(cursor)?,
                nodes: Vec::<String>::deserialize(cursor)?,
            }),
            MSG_CREATE_MAP_REQUEST => Ok(Message::CreateMapRequest {
                name: String::deserialize(cursor)?,
            }),
            MSG_CREATE_MAP_RESPONSE => Ok(Message::CreateMapResponse {
                code: ResponseCode::deserialize(cursor)?,
                map_name: String::deserialize(cursor)?,
            }),
            MSG_MAP_PUT_REQUEST => Ok(Message::MapPutRequest {
                map_name: String::deserialize(cursor)?,
                key: Vec::<u8>::deserialize(cursor)?,
                value: Vec::<u8>::deserialize(cursor)?,
            }),
            MSG_MAP_PUT_RESPONSE => Ok(Message::MapPutResponse {
                code: ResponseCode::deserialize(cursor)?,
            }),
            MSG_MAP_GET_REQUEST => Ok(Message::MapGetRequest {
                map_name: String::deserialize(cursor)?,
                key: Vec::<u8>::deserialize(cursor)?,
            }),
            MSG_MAP_GET_RESPONSE => Ok(Message::MapGetResponse {
                code: ResponseCode::deserialize(cursor)?,
                value: Option::<Vec<u8>>::deserialize(cursor)?,
            }),
            MSG_MAP_REMOVE_REQUEST => Ok(Message::MapRemoveRequest {
                map_name: String::deserialize(cursor)?,
                key: Vec::<u8>::deserialize(cursor)?,
            }),
            MSG_MAP_REMOVE_RESPONSE => Ok(Message::MapRemoveResponse {
                code: ResponseCode::deserialize(cursor)?,
            }),
            MSG_MAP_SIZE_REQUEST => Ok(Message::MapSizeRequest {
                map_name: String::deserialize(cursor)?,
            }),
            MSG_MAP_SIZE_RESPONSE => Ok(Message::MapSizeResponse {
                code: ResponseCode::deserialize(cursor)?,
                size: usize::deserialize(cursor)?,
            }),
            MSG_ERROR_RESPONSE => Ok(Message::ErrorResponse {
                code: ResponseCode::deserialize(cursor)?,
                error: String::deserialize(cursor)?,
            }),
            MSG_LEADER_REDIRECT_RESPONSE => Ok(Message::LeaderRedirectResponse {
                code: ResponseCode::deserialize(cursor)?,
                leader_address: String::deserialize(cursor)?,
            }),
            MSG_PING => Ok(Message::Ping),
            MSG_PONG => Ok(Message::Pong),
            MSG_STATE_SYNC => Ok(Message::StateSync {
                sync_id: u64::deserialize(cursor)?,
                operations: Vec::<StateSyncOperation>::deserialize(cursor)?,
            }),
            MSG_STATE_SYNC_ACK => Ok(Message::StateSyncAck {
                code: ResponseCode::deserialize(cursor)?,
                sync_id: u64::deserialize(cursor)?,
            }),
            
            // Set operations
            MSG_CREATE_SET_REQUEST => Ok(Message::CreateSetRequest {
                name: String::deserialize(cursor)?,
            }),
            MSG_CREATE_SET_RESPONSE => Ok(Message::CreateSetResponse {
                code: ResponseCode::deserialize(cursor)?,
                set_name: String::deserialize(cursor)?,
            }),
            MSG_SET_ADD_REQUEST => Ok(Message::SetAddRequest {
                set_name: String::deserialize(cursor)?,
                value: Vec::<u8>::deserialize(cursor)?,
            }),
            MSG_SET_ADD_RESPONSE => Ok(Message::SetAddResponse {
                code: ResponseCode::deserialize(cursor)?,
            }),
            MSG_SET_REMOVE_REQUEST => Ok(Message::SetRemoveRequest {
                set_name: String::deserialize(cursor)?,
                value: Vec::<u8>::deserialize(cursor)?,
            }),
            MSG_SET_REMOVE_RESPONSE => Ok(Message::SetRemoveResponse {
                code: ResponseCode::deserialize(cursor)?,
            }),
            MSG_SET_CONTAINS_REQUEST => Ok(Message::SetContainsRequest {
                set_name: String::deserialize(cursor)?,
                value: Vec::<u8>::deserialize(cursor)?,
            }),
            MSG_SET_CONTAINS_RESPONSE => Ok(Message::SetContainsResponse {
                code: ResponseCode::deserialize(cursor)?,
                contains: bool::deserialize(cursor)?,
            }),
            MSG_SET_SIZE_REQUEST => Ok(Message::SetSizeRequest {
                set_name: String::deserialize(cursor)?,
            }),
            MSG_SET_SIZE_RESPONSE => Ok(Message::SetSizeResponse {
                code: ResponseCode::deserialize(cursor)?,
                size: usize::deserialize(cursor)?,
            }),
            
            // Counter operations
            MSG_CREATE_COUNTER_REQUEST => Ok(Message::CreateCounterRequest {
                name: String::deserialize(cursor)?,
            }),
            MSG_CREATE_COUNTER_RESPONSE => Ok(Message::CreateCounterResponse {
                code: ResponseCode::deserialize(cursor)?,
                counter_name: String::deserialize(cursor)?,
            }),
            MSG_COUNTER_GET_REQUEST => Ok(Message::CounterGetRequest {
                counter_name: String::deserialize(cursor)?,
            }),
            MSG_COUNTER_GET_RESPONSE => Ok(Message::CounterGetResponse {
                code: ResponseCode::deserialize(cursor)?,
                value: i64::deserialize(cursor)?,
            }),
            MSG_COUNTER_INCREMENT_REQUEST => Ok(Message::CounterIncrementRequest {
                counter_name: String::deserialize(cursor)?,
                delta: i64::deserialize(cursor)?,
            }),
            MSG_COUNTER_INCREMENT_RESPONSE => Ok(Message::CounterIncrementResponse {
                code: ResponseCode::deserialize(cursor)?,
                new_value: i64::deserialize(cursor)?,
            }),
            
            // Lock operations
            MSG_CREATE_LOCK_REQUEST => Ok(Message::CreateLockRequest {
                name: String::deserialize(cursor)?,
            }),
            MSG_CREATE_LOCK_RESPONSE => Ok(Message::CreateLockResponse {
                code: ResponseCode::deserialize(cursor)?,
                lock_name: String::deserialize(cursor)?,
            }),
            MSG_LOCK_ACQUIRE_REQUEST => Ok(Message::LockAcquireRequest {
                lock_name: String::deserialize(cursor)?,
            }),
            MSG_LOCK_ACQUIRE_RESPONSE => Ok(Message::LockAcquireResponse {
                code: ResponseCode::deserialize(cursor)?,
                acquired: bool::deserialize(cursor)?,
                queue_position: Option::<usize>::deserialize(cursor)?,
            }),
            MSG_LOCK_RELEASE_REQUEST => Ok(Message::LockReleaseRequest {
                lock_name: String::deserialize(cursor)?,
            }),
            MSG_LOCK_RELEASE_RESPONSE => Ok(Message::LockReleaseResponse {
                code: ResponseCode::deserialize(cursor)?,
            }),
            MSG_LOCK_SYNC_REQUEST => Ok(Message::LockSyncRequest {
                held_locks: Vec::<String>::deserialize(cursor)?,
            }),
            MSG_LOCK_SYNC_RESPONSE => Ok(Message::LockSyncResponse {
                code: ResponseCode::deserialize(cursor)?,
                synchronized_locks: Vec::<String>::deserialize(cursor)?,
                failed_locks: Vec::<String>::deserialize(cursor)?,
            }),
            
            // Multimap operations
            MSG_CREATE_MULTIMAP_REQUEST => Ok(Message::CreateMultimapRequest {
                name: String::deserialize(cursor)?,
            }),
            MSG_CREATE_MULTIMAP_RESPONSE => Ok(Message::CreateMultimapResponse {
                code: ResponseCode::deserialize(cursor)?,
                multimap_name: String::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_PUT_REQUEST => Ok(Message::MultimapPutRequest {
                multimap_name: String::deserialize(cursor)?,
                key: Vec::<u8>::deserialize(cursor)?,
                value: Vec::<u8>::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_PUT_RESPONSE => Ok(Message::MultimapPutResponse {
                code: ResponseCode::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_GET_REQUEST => Ok(Message::MultimapGetRequest {
                multimap_name: String::deserialize(cursor)?,
                key: Vec::<u8>::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_GET_RESPONSE => Ok(Message::MultimapGetResponse {
                code: ResponseCode::deserialize(cursor)?,
                values: std::collections::HashSet::<Vec<u8>>::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_REMOVE_REQUEST => Ok(Message::MultimapRemoveRequest {
                multimap_name: String::deserialize(cursor)?,
                key: Vec::<u8>::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_REMOVE_RESPONSE => Ok(Message::MultimapRemoveResponse {
                code: ResponseCode::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_REMOVE_VALUE_REQUEST => Ok(Message::MultimapRemoveValueRequest {
                multimap_name: String::deserialize(cursor)?,
                key: Vec::<u8>::deserialize(cursor)?,
                value: Vec::<u8>::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_REMOVE_VALUE_RESPONSE => Ok(Message::MultimapRemoveValueResponse {
                code: ResponseCode::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_SIZE_REQUEST => Ok(Message::MultimapSizeRequest {
                multimap_name: String::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_SIZE_RESPONSE => Ok(Message::MultimapSizeResponse {
                code: ResponseCode::deserialize(cursor)?,
                size: usize::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_KEY_SIZE_REQUEST => Ok(Message::MultimapKeySizeRequest {
                multimap_name: String::deserialize(cursor)?,
                key: Vec::<u8>::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_KEY_SIZE_RESPONSE => Ok(Message::MultimapKeySizeResponse {
                code: ResponseCode::deserialize(cursor)?,
                size: usize::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_CONTAINS_KEY_REQUEST => Ok(Message::MultimapContainsKeyRequest {
                multimap_name: String::deserialize(cursor)?,
                key: Vec::<u8>::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_CONTAINS_KEY_RESPONSE => Ok(Message::MultimapContainsKeyResponse {
                code: ResponseCode::deserialize(cursor)?,
                contains: bool::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_CONTAINS_VALUE_REQUEST => Ok(Message::MultimapContainsValueRequest {
                multimap_name: String::deserialize(cursor)?,
                value: Vec::<u8>::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_CONTAINS_VALUE_RESPONSE => Ok(Message::MultimapContainsValueResponse {
                code: ResponseCode::deserialize(cursor)?,
                contains: bool::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_CONTAINS_ENTRY_REQUEST => Ok(Message::MultimapContainsEntryRequest {
                multimap_name: String::deserialize(cursor)?,
                key: Vec::<u8>::deserialize(cursor)?,
                value: Vec::<u8>::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_CONTAINS_ENTRY_RESPONSE => Ok(Message::MultimapContainsEntryResponse {
                code: ResponseCode::deserialize(cursor)?,
                contains: bool::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_KEYS_REQUEST => Ok(Message::MultimapKeysRequest {
                multimap_name: String::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_KEYS_RESPONSE => Ok(Message::MultimapKeysResponse {
                code: ResponseCode::deserialize(cursor)?,
                keys: std::collections::HashSet::<Vec<u8>>::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_VALUES_REQUEST => Ok(Message::MultimapValuesRequest {
                multimap_name: String::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_VALUES_RESPONSE => Ok(Message::MultimapValuesResponse {
                code: ResponseCode::deserialize(cursor)?,
                values: Vec::<Vec<u8>>::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_ENTRIES_REQUEST => Ok(Message::MultimapEntriesRequest {
                multimap_name: String::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_ENTRIES_RESPONSE => Ok(Message::MultimapEntriesResponse {
                code: ResponseCode::deserialize(cursor)?,
                entries: std::collections::HashMap::<Vec<u8>, std::collections::HashSet<Vec<u8>>>::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_CLEAR_REQUEST => Ok(Message::MultimapClearRequest {
                multimap_name: String::deserialize(cursor)?,
            }),
            MSG_MULTIMAP_CLEAR_RESPONSE => Ok(Message::MultimapClearResponse {
                code: ResponseCode::deserialize(cursor)?,
            }),
            
            _ => Err(WireError::InvalidData(format!("Unknown message kind: {}", message_kind))),
        }
    }
}

impl<T: WireSerializable> WireSerializable for FramedMessage<T> {
    fn serialize(&self, buf: &mut Vec<u8>) -> Result<(), WireError> {
        // Reserve space for checksum and size
        let _checksum_placeholder = 0u32;
        let _size_placeholder = 0u32;
        
        // Start building the message
        let mut temp_buf = Vec::new();
        
        // Write header (without checksum and size)
        self.protocol_version.serialize(&mut temp_buf)?;
        self.flags.as_u8().serialize(&mut temp_buf)?;
        self.message_kind.serialize(&mut temp_buf)?;
        self.request_id.serialize(&mut temp_buf)?;
        
        // Write payload
        self.payload.serialize(&mut temp_buf)?;
        
        // Calculate total size (including checksum and size fields)
        let total_size = (4 + 4 + temp_buf.len()) as u32;
        
        // Calculate checksum (CRC32 of everything except the checksum field)
        let mut hasher = Hasher::new();
        hasher.update(&total_size.to_le_bytes());
        hasher.update(&temp_buf);
        let checksum = hasher.finalize();
        
        // Write the complete message
        checksum.serialize(buf)?;
        total_size.serialize(buf)?;
        buf.extend_from_slice(&temp_buf);
        
        Ok(())
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, WireError> {
        // Read checksum and size
        let stored_checksum = u32::deserialize(cursor)?;
        let size = u32::deserialize(cursor)?;
        
        // Read the rest of the message
        let remaining_size = (size - 8) as usize; // Subtract checksum and size fields
        let mut remaining_data = vec![0u8; remaining_size];
        cursor.read_exact(&mut remaining_data)?;
        
        // Verify checksum
        let mut hasher = Hasher::new();
        hasher.update(&size.to_le_bytes());
        hasher.update(&remaining_data);
        let calculated_checksum = hasher.finalize();
        
        if stored_checksum != calculated_checksum {
            return Err(WireError::InvalidData(format!(
                "Checksum mismatch: expected {}, got {}",
                stored_checksum, calculated_checksum
            )));
        }
        
        // Parse the remaining data
        let mut data_cursor = Cursor::new(&remaining_data[..]);
        
        Ok(FramedMessage {
            protocol_version: u8::deserialize(&mut data_cursor)?,
            flags: MessageFlags::new(u8::deserialize(&mut data_cursor)?),
            message_kind: u16::deserialize(&mut data_cursor)?,
            request_id: u32::deserialize(&mut data_cursor)?,
            payload: T::deserialize(&mut data_cursor)?,
        })
    }
}

/// A framed message containing header and payload.
/// The wire format is:
/// - checksum: u32 (little-endian) - CRC32 of entire message excluding checksum
/// - size: u32 (little-endian) - Total message size including header
/// - protocol_version: u8
/// - flags: u8 - Message type flags
/// - message_kind: u16 (little-endian) - Action identifier
/// - request_id: u32 (little-endian) - Unique request ID
/// - payload: serialized data (using custom binary format)
#[derive(Debug, PartialEq)]
pub struct FramedMessage<T> {
    /// Protocol version for compatibility checks.
    pub protocol_version: u8,
    /// Message type flags.
    pub flags: MessageFlags,
    /// Message kind identifier.
    pub message_kind: MessageKind,
    /// Unique request ID for matching requests and responses.
    pub request_id: RequestId,
    /// The payload (Request or Response).
    pub payload: T,
}


/// Helper to serialize a framed message to bytes.
/// Uses custom binary serialization with little-endian byte order.
pub fn serialize_message<T: WireSerializable>(msg: &FramedMessage<T>) -> Result<Vec<u8>, WireError> {
    let mut buf = Vec::new();
    msg.serialize(&mut buf)?;
    Ok(buf)
}

/// Helper to deserialize a framed message from bytes.
/// Uses custom binary serialization with little-endian byte order.
pub fn deserialize_message<T: WireSerializable>(data: &[u8]) -> Result<FramedMessage<T>, WireError> {
    let mut cursor = Cursor::new(data);
    FramedMessage::deserialize(&mut cursor)
}

/// Specialized function to deserialize FramedMessage<Message> that uses action_id.
/// This overrides the generic deserialize_message for Message specifically.
pub fn deserialize_message_specialized(data: &[u8]) -> Result<FramedMessage<Message>, WireError> {
    let mut cursor = Cursor::new(data);
    
    // Read checksum and size
    let stored_checksum = u32::deserialize(&mut cursor)?;
    let size = u32::deserialize(&mut cursor)?;
    
    // Read the rest of the message
    let remaining_size = (size - 8) as usize; // Subtract checksum and size fields
    let mut remaining_data = vec![0u8; remaining_size];
    cursor.read_exact(&mut remaining_data)?;
    
    // Verify checksum
    let mut hasher = Hasher::new();
    hasher.update(&size.to_le_bytes());
    hasher.update(&remaining_data);
    let calculated_checksum = hasher.finalize();
    
    if stored_checksum != calculated_checksum {
        return Err(WireError::InvalidData(format!(
            "Checksum mismatch: expected {}, got {}",
            stored_checksum, calculated_checksum
        )));
    }
    
    // Parse the remaining data
    let mut data_cursor = Cursor::new(&remaining_data[..]);
    
    let protocol_version = u8::deserialize(&mut data_cursor)?;
    let flags = MessageFlags::new(u8::deserialize(&mut data_cursor)?);
    let message_kind = u16::deserialize(&mut data_cursor)?;
    let request_id = u32::deserialize(&mut data_cursor)?;
    
    // Use the message_kind to deserialize the Message payload
    let payload = Message::deserialize_with_message_kind(&mut data_cursor, message_kind)?;
    
    Ok(FramedMessage {
        protocol_version,
        flags,
        message_kind,
        request_id,
        payload,
    })
}

// FFI-friendly functions for external integration (e.g., Java via JNI)
use std::ffi::CString;
use std::os::raw::c_char;
use crate::App;

/// Create a new App instance and return a pointer to it.
/// Caller must free using free_app().
#[unsafe(no_mangle)]
pub extern "C" fn create_app() -> *mut App {
    let app = Box::new(App::new());
    Box::into_raw(app)
}

/// Create a new App instance with custom Raft configuration.
/// Caller must free using free_app().
#[unsafe(no_mangle)]
pub extern "C" fn create_app_with_raft_node(node_id: i32, address: *const c_char) -> *mut App {
    let address = unsafe {
        if address.is_null() {
            "127.0.0.1:8080".to_string()
        } else {
            let c_str = std::ffi::CStr::from_ptr(address);
            c_str.to_string_lossy().into_owned()
        }
    };
    
    let app = Box::new(App::new_with_raft_node(node_id, address));
    Box::into_raw(app)
}

/// Process a request given as a byte array using the provided App instance.
/// Returns a pointer to the serialized response bytes.
/// Caller must free the returned pointer using free_response().
#[unsafe(no_mangle)]
pub extern "C" fn process_request_bytes(
    app: *mut App,
    request_bytes: *const u8,
    len: usize
) -> *mut c_char {
    unsafe {
        if app.is_null() {
            let error_msg = b"App instance is null";
            let c_string = CString::new(error_msg).unwrap();
            return c_string.into_raw();
        }
        
        let app_ref = &*app;
        let data = std::slice::from_raw_parts(request_bytes, len);
        
        if let Ok(message) = deserialize_message::<Message>(data) {
            let response = app_ref.process_message(message, None, None);
            if let Ok(serialized) = serialize_message(&response) {
                // Convert to C string (null-terminated)
                let c_string = CString::new(serialized).unwrap();
                c_string.into_raw()
            } else {
                let error_msg = b"Serialization failed";
                let c_string = CString::new(error_msg).unwrap();
                c_string.into_raw()
            }
        } else {
            let error_msg = b"Deserialization failed";
            let c_string = CString::new(error_msg).unwrap();
            c_string.into_raw()
        }
    }
}

/// Free the memory allocated by process_request_bytes.
#[unsafe(no_mangle)]
pub extern "C" fn free_response(response: *mut c_char) {
    unsafe {
        if !response.is_null() {
            let _ = CString::from_raw(response);
        }
    }
}

/// Free an App instance created by create_app().
#[unsafe(no_mangle)]
pub extern "C" fn free_app(app: *mut App) {
    unsafe {
        if !app.is_null() {
            let _ = Box::from_raw(app);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to test round-trip serialization
    fn test_round_trip<T: WireSerializable + PartialEq + std::fmt::Debug>(value: T) {
        let mut buf = Vec::new();
        value.serialize(&mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        let deserialized = T::deserialize(&mut cursor).unwrap();
        assert_eq!(value, deserialized);
    }

    // Helper function to test round-trip serialization for Message enum using FramedMessage
    fn test_message_round_trip(message: Message) {
        let framed = FramedMessage {
            protocol_version: PROTOCOL_VERSION,
            flags: MessageFlags::REQUEST,
            message_kind: message.get_message_kind(),
            request_id: 12345,
            payload: message.clone(),
        };

        let serialized = serialize_message(&framed).unwrap();
        let deserialized = deserialize_message_specialized(&serialized).unwrap();

        assert_eq!(framed.protocol_version, deserialized.protocol_version);
        assert_eq!(framed.flags, deserialized.flags);
        assert_eq!(framed.message_kind, deserialized.message_kind);
        assert_eq!(framed.request_id, deserialized.request_id);
        assert_eq!(framed.payload, deserialized.payload);
    }

    #[test]
    fn test_u8_serialization() {
        test_round_trip(42u8);
        test_round_trip(0u8);
        test_round_trip(255u8);
    }

    #[test]
    fn test_u64_serialization() {
        test_round_trip(42u64);
        test_round_trip(0u64);
        test_round_trip(u64::MAX);
    }

    #[test]
    fn test_i64_serialization() {
        test_round_trip(42i64);
        test_round_trip(-42i64);
        test_round_trip(0i64);
        test_round_trip(i64::MIN);
        test_round_trip(i64::MAX);
    }

    #[test]
    fn test_usize_serialization() {
        test_round_trip(42usize);
        test_round_trip(0usize);
    }

    #[test]
    fn test_bool_serialization() {
        test_round_trip(true);
        test_round_trip(false);
    }

    #[test]
    fn test_string_serialization() {
        test_round_trip("".to_string());
        test_round_trip("hello".to_string());
        test_round_trip("hello world with spaces".to_string());
        test_round_trip("unicode: 你好世界 🌍".to_string());
    }

    #[test]
    fn test_vec_u8_serialization() {
        test_round_trip(Vec::<u8>::new());
        test_round_trip(vec![1u8, 2u8, 3u8, 4u8, 5u8]);
        test_round_trip(vec![0u8; 1000]); // Large vector
    }

    #[test]
    fn test_vec_string_serialization() {
        test_round_trip(Vec::<String>::new());
        test_round_trip(vec!["hello".to_string(), "world".to_string()]);
        test_round_trip(vec!["".to_string(), "test".to_string(), "multiple".to_string()]);
    }

    #[test]
    fn test_option_vec_u8_serialization() {
        test_round_trip(Some(vec![1, 2, 3]));
        test_round_trip(Some(Vec::<u8>::new()));
        test_round_trip(None::<Vec<u8>>);
    }

    #[test]
    fn test_message_join_request() {
        test_message_round_trip(Message::JoinRequest { node_id: "node1".to_string() });
        test_message_round_trip(Message::JoinRequest { node_id: "".to_string() });
    }

    #[test]
    fn test_message_leave_request() {
        test_message_round_trip(Message::LeaveRequest { node_id: "node1".to_string() });
    }

    #[test]
    fn test_message_get_nodes_request() {
        test_message_round_trip(Message::GetNodesRequest);
    }

    #[test]
    fn test_message_create_map_request() {
        test_message_round_trip(Message::CreateMapRequest { name: "my_map".to_string() });
        test_message_round_trip(Message::CreateMapRequest { name: "".to_string() });
    }

    #[test]
    fn test_message_map_put_request() {
        test_message_round_trip(Message::MapPutRequest {
            map_name: "map1".to_string(),
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        });
        test_message_round_trip(Message::MapPutRequest {
            map_name: "map0".to_string(),
            key: Vec::new(),
            value: Vec::new(),
        });
    }

    #[test]
    fn test_message_map_get_request() {
        test_message_round_trip(Message::MapGetRequest {
            map_name: "map1".to_string(),
            key: b"key".to_vec(),
        });
    }

    #[test]
    fn test_message_map_remove_request() {
        test_message_round_trip(Message::MapRemoveRequest {
            map_name: "map1".to_string(),
            key: b"key".to_vec(),
        });
    }

    #[test]
    fn test_message_map_size_request() {
        test_message_round_trip(Message::MapSizeRequest { map_name: "map1".to_string() });
    }

    #[test]
    fn test_message_create_set_request() {
        test_message_round_trip(Message::CreateSetRequest { name: "my_set".to_string() });
    }

    #[test]
    fn test_message_set_add_request() {
        test_message_round_trip(Message::SetAddRequest {
            set_name: "set1".to_string(),
            value: b"value".to_vec(),
        });
    }

    #[test]
    fn test_message_set_remove_request() {
        test_message_round_trip(Message::SetRemoveRequest {
            set_name: "set1".to_string(),
            value: b"value".to_vec(),
        });
    }

    #[test]
    fn test_message_set_contains_request() {
        test_message_round_trip(Message::SetContainsRequest {
            set_name: "set1".to_string(),
            value: b"value".to_vec(),
        });
    }

    #[test]
    fn test_message_create_counter_request() {
        test_message_round_trip(Message::CreateCounterRequest { name: "my_counter".to_string() });
    }

    #[test]
    fn test_message_counter_get_request() {
        test_message_round_trip(Message::CounterGetRequest { counter_name: "counter1".to_string() });
    }

    #[test]
    fn test_message_counter_increment_request() {
        test_message_round_trip(Message::CounterIncrementRequest {
            counter_name: "counter1".to_string(),
            delta: 5,
        });
        test_message_round_trip(Message::CounterIncrementRequest {
            counter_name: "counter1".to_string(),
            delta: -5,
        });
    }

    #[test]
    fn test_message_create_lock_request() {
        test_message_round_trip(Message::CreateLockRequest { name: "my_lock".to_string() });
    }

    #[test]
    fn test_message_lock_acquire_request() {
        test_message_round_trip(Message::LockAcquireRequest { lock_name: "lock1".to_string() });
    }

    #[test]
    fn test_message_lock_release_request() {
        test_message_round_trip(Message::LockReleaseRequest { lock_name: "lock1".to_string() });
    }
    
    #[test]
    fn test_message_create_multimap_request() {
        test_message_round_trip(Message::CreateMultimapRequest { name: "my_multimap".to_string() });
        test_message_round_trip(Message::CreateMultimapRequest { name: "".to_string() });
    }
    
    #[test]
    fn test_message_multimap_put_request() {
        test_message_round_trip(Message::MultimapPutRequest {
            multimap_name: "multimap1".to_string(),
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        });
    }
    
    #[test]
    fn test_message_multimap_get_request() {
        test_message_round_trip(Message::MultimapGetRequest {
            multimap_name: "multimap1".to_string(),
            key: b"key".to_vec(),
        });
    }
    
    #[test]
    fn test_message_multimap_remove_request() {
        test_message_round_trip(Message::MultimapRemoveRequest {
            multimap_name: "multimap1".to_string(),
            key: b"key".to_vec(),
        });
    }
    
    #[test]
    fn test_message_multimap_remove_value_request() {
        test_message_round_trip(Message::MultimapRemoveValueRequest {
            multimap_name: "multimap1".to_string(),
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        });
    }
    
    #[test]
    fn test_message_multimap_size_request() {
        test_message_round_trip(Message::MultimapSizeRequest { multimap_name: "multimap1".to_string() });
    }
    
    #[test]
    fn test_message_multimap_key_size_request() {
        test_message_round_trip(Message::MultimapKeySizeRequest {
            multimap_name: "multimap1".to_string(),
            key: b"key".to_vec(),
        });
    }
    
    #[test]
    fn test_message_multimap_contains_key_request() {
        test_message_round_trip(Message::MultimapContainsKeyRequest {
            multimap_name: "multimap1".to_string(),
            key: b"key".to_vec(),
        });
    }
    
    #[test]
    fn test_message_multimap_contains_value_request() {
        test_message_round_trip(Message::MultimapContainsValueRequest {
            multimap_name: "multimap1".to_string(),
            value: b"value".to_vec(),
        });
    }
    
    #[test]
    fn test_message_multimap_contains_entry_request() {
        test_message_round_trip(Message::MultimapContainsEntryRequest {
            multimap_name: "multimap1".to_string(),
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        });
    }
    
    #[test]
    fn test_message_multimap_keys_request() {
        test_message_round_trip(Message::MultimapKeysRequest { multimap_name: "multimap1".to_string() });
    }
    
    #[test]
    fn test_message_multimap_values_request() {
        test_message_round_trip(Message::MultimapValuesRequest { multimap_name: "multimap1".to_string() });
    }
    
    #[test]
    fn test_message_multimap_entries_request() {
        test_message_round_trip(Message::MultimapEntriesRequest { multimap_name: "multimap1".to_string() });
    }
    
    #[test]
    fn test_message_multimap_clear_request() {
        test_message_round_trip(Message::MultimapClearRequest { multimap_name: "multimap1".to_string() });
    }

    #[test]
    fn test_message_join_response() {
        test_message_round_trip(Message::JoinResponse { code: ResponseCode::OK, connection_id: 12345 });
    }

    #[test]
    fn test_message_leave_response() {
        test_message_round_trip(Message::LeaveResponse { code: ResponseCode::OK });
    }

    #[test]
    fn test_message_map_put_response() {
        test_message_round_trip(Message::MapPutResponse { code: ResponseCode::OK });
    }

    #[test]
    fn test_message_map_remove_response() {
        test_message_round_trip(Message::MapRemoveResponse { code: ResponseCode::OK });
    }

    #[test]
    fn test_message_set_add_response() {
        test_message_round_trip(Message::SetAddResponse { code: ResponseCode::OK });
    }

    #[test]
    fn test_message_set_remove_response() {
        test_message_round_trip(Message::SetRemoveResponse { code: ResponseCode::OK });
    }

    #[test]
    fn test_message_lock_release_response() {
        test_message_round_trip(Message::LockReleaseResponse { code: ResponseCode::OK });
    }

    #[test]
    fn test_message_error_response() {
        test_message_round_trip(Message::ErrorResponse { code: ResponseCode::ERR, error: "error message".to_string() });
        test_message_round_trip(Message::ErrorResponse { code: ResponseCode::ERR, error: "".to_string() });
    }

    #[test]
    fn test_message_get_nodes_response() {
        test_message_round_trip(Message::GetNodesResponse { code: ResponseCode::OK, nodes: vec!["node1".to_string(), "node2".to_string()] });
        test_message_round_trip(Message::GetNodesResponse { code: ResponseCode::OK, nodes: Vec::new() });
    }

    #[test]
    fn test_message_create_map_response() {
        test_message_round_trip(Message::CreateMapResponse { code: ResponseCode::OK, map_name: "map1".to_string() });
        test_message_round_trip(Message::CreateMapResponse { code: ResponseCode::OK, map_name: "map0".to_string() });
    }

    #[test]
    fn test_message_create_set_response() {
        test_message_round_trip(Message::CreateSetResponse { code: ResponseCode::OK, set_name: "set1".to_string() });
    }

    #[test]
    fn test_message_create_counter_response() {
        test_message_round_trip(Message::CreateCounterResponse { code: ResponseCode::OK, counter_name: "counter1".to_string() });
    }

    #[test]
    fn test_message_create_lock_response() {
        test_message_round_trip(Message::CreateLockResponse { code: ResponseCode::OK, lock_name: "lock1".to_string() });
    }

    #[test]
    fn test_message_map_get_response() {
        test_message_round_trip(Message::MapGetResponse { code: ResponseCode::OK, value: Some(b"value".to_vec()) });
        test_message_round_trip(Message::MapGetResponse { code: ResponseCode::OK, value: None });
        test_message_round_trip(Message::MapGetResponse { code: ResponseCode::OK, value: Some(Vec::new()) });
    }

    #[test]
    fn test_message_map_size_response() {
        test_message_round_trip(Message::MapSizeResponse { code: ResponseCode::OK, size: 0 });
        test_message_round_trip(Message::MapSizeResponse { code: ResponseCode::OK, size: 1000 });
    }

    #[test]
    fn test_message_set_size_response() {
        test_message_round_trip(Message::SetSizeResponse { code: ResponseCode::OK, size: 0 });
        test_message_round_trip(Message::SetSizeResponse { code: ResponseCode::OK, size: 1000 });
    }

    #[test]
    fn test_message_set_contains_response() {
        test_message_round_trip(Message::SetContainsResponse { code: ResponseCode::OK, contains: true });
        test_message_round_trip(Message::SetContainsResponse { code: ResponseCode::OK, contains: false });
    }

    #[test]
    fn test_message_counter_get_response() {
        test_message_round_trip(Message::CounterGetResponse { code: ResponseCode::OK, value: 42 });
        test_message_round_trip(Message::CounterGetResponse { code: ResponseCode::OK, value: -42 });
    }

    #[test]
    fn test_message_counter_increment_response() {
        test_message_round_trip(Message::CounterIncrementResponse { code: ResponseCode::OK, new_value: 47 });
        test_message_round_trip(Message::CounterIncrementResponse { code: ResponseCode::OK, new_value: -37 });
    }

    #[test]
    fn test_message_lock_acquire_response() {
        test_message_round_trip(Message::LockAcquireResponse { code: ResponseCode::OK, acquired: true, queue_position: None });
        test_message_round_trip(Message::LockAcquireResponse { code: ResponseCode::OK, acquired: false, queue_position: Some(5) });
    }
    
    #[test]
    fn test_message_create_multimap_response() {
        test_message_round_trip(Message::CreateMultimapResponse { code: ResponseCode::OK, multimap_name: "multimap1".to_string() });
        test_message_round_trip(Message::CreateMultimapResponse { code: ResponseCode::OK, multimap_name: "multimap0".to_string() });
    }
    
    #[test]
    fn test_message_multimap_values_response() {
        test_message_round_trip(Message::MultimapValuesResponse { code: ResponseCode::OK, values: vec![b"value1".to_vec(), b"value2".to_vec()] });
        test_message_round_trip(Message::MultimapValuesResponse { code: ResponseCode::OK, values: Vec::new() });
    }
    
    #[test]
    fn test_message_multimap_keys_response() {
        let mut keys = std::collections::HashSet::new();
        keys.insert(b"key1".to_vec());
        keys.insert(b"key2".to_vec());
        test_message_round_trip(Message::MultimapKeysResponse { code: ResponseCode::OK, keys: keys.clone() });
        test_message_round_trip(Message::MultimapKeysResponse { code: ResponseCode::OK, keys: std::collections::HashSet::new() });
    }
    
    #[test]
    fn test_message_multimap_entries_response() {
        let mut entries = std::collections::HashMap::new();
        let mut values1 = std::collections::HashSet::new();
        values1.insert(b"value1".to_vec());
        let mut values2 = std::collections::HashSet::new();
        values2.insert(b"value2".to_vec());
        entries.insert(b"key1".to_vec(), values1);
        entries.insert(b"key2".to_vec(), values2);
        test_message_round_trip(Message::MultimapEntriesResponse { code: ResponseCode::OK, entries });
        test_message_round_trip(Message::MultimapEntriesResponse { code: ResponseCode::OK, entries: std::collections::HashMap::new() });
    }

    #[test]
    fn test_message_multimap_get_response() {
        let mut values = std::collections::HashSet::new();
        values.insert(b"value1".to_vec());
        values.insert(b"value2".to_vec());
        test_message_round_trip(Message::MultimapGetResponse { code: ResponseCode::OK, values: values.clone() });
        test_message_round_trip(Message::MultimapGetResponse { code: ResponseCode::OK, values: std::collections::HashSet::new() });
    }

    #[test]
    fn test_message_multimap_size_response() {
        test_message_round_trip(Message::MultimapSizeResponse { code: ResponseCode::OK, size: 0 });
        test_message_round_trip(Message::MultimapSizeResponse { code: ResponseCode::OK, size: 1000 });
    }

    #[test]
    fn test_message_multimap_key_size_response() {
        test_message_round_trip(Message::MultimapKeySizeResponse { code: ResponseCode::OK, size: 0 });
        test_message_round_trip(Message::MultimapKeySizeResponse { code: ResponseCode::OK, size: 10 });
    }

    #[test]
    fn test_message_multimap_contains_key_response() {
        test_message_round_trip(Message::MultimapContainsKeyResponse { code: ResponseCode::OK, contains: true });
        test_message_round_trip(Message::MultimapContainsKeyResponse { code: ResponseCode::OK, contains: false });
    }

    #[test]
    fn test_message_multimap_contains_value_response() {
        test_message_round_trip(Message::MultimapContainsValueResponse { code: ResponseCode::OK, contains: true });
        test_message_round_trip(Message::MultimapContainsValueResponse { code: ResponseCode::OK, contains: false });
    }

    #[test]
    fn test_message_multimap_contains_entry_response() {
        test_message_round_trip(Message::MultimapContainsEntryResponse { code: ResponseCode::OK, contains: true });
        test_message_round_trip(Message::MultimapContainsEntryResponse { code: ResponseCode::OK, contains: false });
    }

    #[test]
    fn test_message_multimap_put_response() {
        test_message_round_trip(Message::MultimapPutResponse { code: ResponseCode::OK });
    }

    #[test]
    fn test_message_multimap_remove_response() {
        test_message_round_trip(Message::MultimapRemoveResponse { code: ResponseCode::OK });
    }

    #[test]
    fn test_message_multimap_remove_value_response() {
        test_message_round_trip(Message::MultimapRemoveValueResponse { code: ResponseCode::OK });
    }

    #[test]
    fn test_message_multimap_clear_response() {
        test_message_round_trip(Message::MultimapClearResponse { code: ResponseCode::OK });
    }

    #[test]
    fn test_framed_message_request() {
        let msg = FramedMessage {
            protocol_version: 1,
            flags: MessageFlags::REQUEST,
            message_kind: MSG_JOIN_REQUEST,
            request_id: 123,
            payload: Message::JoinRequest { node_id: "test".to_string() },
        };
        let serialized = serialize_message(&msg).unwrap();
        let deserialized = deserialize_message_specialized(&serialized).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_framed_message_response() {
        let msg = FramedMessage {
            protocol_version: 1,
            flags: MessageFlags::RESPONSE,
            message_kind: MSG_MAP_GET_RESPONSE,
            request_id: 456,
            payload: Message::MapGetResponse { code: ResponseCode::OK, value: Some(b"data".to_vec()) },
        };
        let serialized = serialize_message(&msg).unwrap();
        let deserialized = deserialize_message_specialized(&serialized).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_serialize_deserialize_message_functions() {
        let msg = FramedMessage {
            protocol_version: 1,
            flags: MessageFlags::REQUEST,
            message_kind: MSG_MAP_PUT_REQUEST,
            request_id: 789,
            payload: Message::MapPutRequest {
                map_name: "test_map".to_string(),
                key: b"key".to_vec(),
                value: b"value".to_vec(),
            },
        };

        let serialized = serialize_message(&msg).unwrap();
        let deserialized = deserialize_message_specialized(&serialized).unwrap();

        assert_eq!(msg.protocol_version, deserialized.protocol_version);
        assert_eq!(msg.flags, deserialized.flags);
        assert_eq!(msg.message_kind, deserialized.message_kind);
        assert_eq!(msg.request_id, deserialized.request_id);
        assert_eq!(msg.payload, deserialized.payload);
    }

    #[test]
    fn test_protocol_version() {
        assert_eq!(PROTOCOL_VERSION, 1);
    }
    
    #[test]
    fn test_message_flags() {
        let request_flag = MessageFlags::REQUEST;
        assert!(request_flag.is_request());
        assert!(!request_flag.is_response());
        assert!(!request_flag.is_ping());
        assert!(!request_flag.is_pong());
        
        let response_flag = MessageFlags::RESPONSE;
        assert!(!response_flag.is_request());
        assert!(response_flag.is_response());
        assert!(!response_flag.is_ping());
        assert!(!response_flag.is_pong());
        
        let ping_flag = MessageFlags::PING;
        assert!(!ping_flag.is_request());
        assert!(!ping_flag.is_response());
        assert!(ping_flag.is_ping());
        assert!(!ping_flag.is_pong());
        
        let pong_flag = MessageFlags::PONG;
        assert!(!pong_flag.is_request());
        assert!(!pong_flag.is_response());
        assert!(!pong_flag.is_ping());
        assert!(pong_flag.is_pong());
        
        // Test combined flags
        let combined = MessageFlags::new(MessageFlags::REQUEST.0 | MessageFlags::PING.0);
        assert!(combined.is_request());
        assert!(combined.is_ping());
        assert!(!combined.is_response());
        assert!(!combined.is_pong());
    }
    
    #[test]
    fn test_checksum_validation() {
        let msg = FramedMessage {
            protocol_version: 1,
            flags: MessageFlags::REQUEST,
            message_kind: MSG_JOIN_REQUEST,
            request_id: 123,
            payload: Message::JoinRequest { node_id: "test_node".to_string() },
        };
        
        // Serialize the message
        let serialized = serialize_message(&msg).unwrap();
        
        // Corrupt the data (change a byte after the checksum)
        let mut corrupted = serialized.clone();
        corrupted[10] ^= 0xFF; // Flip bits in a data byte
        
        // Try to deserialize the corrupted message
        let result = deserialize_message_specialized(&corrupted);
        assert!(result.is_err());
        
        // Verify the error is about checksum mismatch
        if let Err(WireError::InvalidData(msg)) = result {
            assert!(msg.contains("Checksum mismatch"));
        } else {
            panic!("Expected checksum mismatch error");
        }
        
        // Original message should still deserialize correctly
        let deserialized = deserialize_message_specialized(&serialized).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_error_handling() {
        // Test invalid discriminant for Message
        let invalid_data = vec![255u8]; // Invalid discriminant
        let mut cursor = Cursor::new(&invalid_data[..]);
        let result = Message::deserialize(&mut cursor);
        assert!(result.is_err());

        // Test invalid Option tag
        let invalid_data = vec![2u8]; // Invalid tag for Option
        let mut cursor = Cursor::new(&invalid_data[..]);
        let result = Option::<Vec<u8>>::deserialize(&mut cursor);
        assert!(result.is_err());

        // Test invalid bool value
        let invalid_data = vec![2u8]; // Invalid bool value
        let mut cursor = Cursor::new(&invalid_data[..]);
        let result = bool::deserialize(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_edge_cases() {
        // Empty strings
        test_round_trip("".to_string());

        // Empty vectors
        test_round_trip(Vec::<u8>::new());
        test_round_trip(Vec::<String>::new());

        // Large data
        let large_vec = vec![42u8; 10000];
        test_round_trip(large_vec);

        // Unicode strings
        test_round_trip("🚀 Unicode test 🌟".to_string());

        // Zero values
        test_round_trip(0u64);
        test_round_trip(0i64);
        test_round_trip(0usize);
    }
}
