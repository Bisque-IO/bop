/// Library constants matching usockets behavior

/// 512KB shared receive buffer
pub const RECV_BUFFER_LENGTH: usize = 524_288;

/// 32 byte padding of receive buffer ends
pub const RECV_BUFFER_PADDING: usize = 32;

/// Timeout granularity of 4 seconds
pub const TIMEOUT_GRANULARITY: u32 = 4;

/// Guaranteed alignment of extension memory
pub const EXT_ALIGNMENT: usize = 16;

/// Maximum number of low-priority sockets to handle per loop iteration
/// This prevents SSL handshakes from starving regular traffic
pub const MAX_LOW_PRIO_PER_ITERATION: usize = 5;

/// Timeout value indicating "no timeout"
pub const NO_TIMEOUT: u8 = 255;

/// Number of buckets in timeout wheel (4 minutes with 4-second granularity)
pub const TIMEOUT_BUCKETS: u8 = 240;

/// Maximum UDP packet size
pub const UDP_MAX_SIZE: usize = 2048;

/// Maximum number of UDP packets to batch
pub const UDP_MAX_NUM: usize = 16;

/// Listen options
pub mod listen_options {
    /// Default listen option
    pub const DEFAULT: i32 = 0;

    /// Exclusively own this port (SO_REUSEPORT not set)
    pub const EXCLUSIVE_PORT: i32 = 1;
}

/// Poll types - stored in lower 2 bits
pub mod poll_type {
    pub const SOCKET: u8 = 0;
    pub const SOCKET_SHUT_DOWN: u8 = 1;
    pub const SEMI_SOCKET: u8 = 2;
    pub const CALLBACK: u8 = 3;

    /// Mask to extract poll type from flags
    pub const MASK: u8 = 0x03;
}

/// Poll flags - stored in upper bits
pub mod poll_flags {
    pub const POLLING_OUT: u8 = 4;
    pub const POLLING_IN: u8 = 8;
}
