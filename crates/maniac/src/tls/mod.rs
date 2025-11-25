//! TLS support for monoio
//!
//! This module provides TLS/SSL support through both rustls and native-tls backends.

#[cfg(feature = "tls-rustls")]
pub mod rustls;

#[cfg(feature = "tls-native")]
pub mod native_tls;

pub mod io_wrapper;

// Re-export commonly used types
#[cfg(feature = "tls-rustls")]
pub use rustls::{TlsAcceptor as RustlsAcceptor, TlsConnector as RustlsConnector};

#[cfg(feature = "tls-native")]
pub use native_tls::{TlsAcceptor as NativeTlsAcceptor, TlsConnector as NativeTlsConnector};
