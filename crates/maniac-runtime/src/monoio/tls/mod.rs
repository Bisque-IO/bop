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
pub use rustls::{TlsConnector as RustlsConnector, TlsAcceptor as RustlsAcceptor};

#[cfg(feature = "tls-native")]
pub use native_tls::{TlsConnector as NativeTlsConnector, TlsAcceptor as NativeTlsAcceptor};

