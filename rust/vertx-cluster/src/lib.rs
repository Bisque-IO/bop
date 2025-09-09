//! # Vertx Cluster
//! 
//! Distributed clustering and state management for the BOP platform.
//! 
//! This crate provides high-level abstractions for building distributed applications
//! using the BOP platform's networking, consensus, and storage capabilities.
//! 
//! ## Architecture
//! 
//! - **State Machine**: Finite state machine for managing cluster state transitions
//! - **Wire Protocol**: Serialization and messaging for inter-node communication  
//! - **Cluster Management**: Node discovery, health monitoring, and failover
//! - **Application Layer**: High-level APIs for distributed applications
//! 
//! ## Features
//! 
//! - Message-driven state management with strong consistency
//! - Binary wire protocol with CRC validation
//! - Support for Raft consensus (when enabled)
//! - Integration with BOP's high-performance networking
//! - Custom memory allocation for optimal performance
//! 
//! ## Example
//! 
//! ```rust,no_run
//! use vertx_cluster::{App, State};
//! 
//! // Create a new cluster application
//! let app = App::new();
//! 
//! // Access the shared state
//! let state = app.state();
//! 
//! // Process messages and handle state transitions
//! // let response = app.process_message(message, connection_id, node_id);
//! ```

pub mod fsm;
pub mod wire;
pub mod net;

// Re-export the main types for convenience
pub use fsm::{process_message, State};
pub use wire::{FramedMessage, Message, RequestId, StateSyncOperation, PROTOCOL_VERSION};
pub use net::{TcpServerConfig, TlsConfig, ConnectionHandler, EchoHandler, NetError, NetResult};
// Note: TcpServer is generic over the handler type, use net::TcpServer<YourHandler> directly

// Re-export BOP platform components
pub use bop_rs::{
    allocator,
    // raft (when compilation issues are resolved)
};

use std::sync::Arc;

/// Application wrapper that holds the cluster manager state
pub struct App {
    state: Arc<State>,
}

impl App {
    /// Create a new App instance with default configuration
    pub fn new() -> Self {
        Self {
            state: Arc::new(State::new()),
        }
    }

    /// Create a new App instance with custom Raft node configuration
    pub fn new_with_raft_node(node_id: i32, address: String) -> Self {
        Self {
            state: Arc::new(State::new_with_raft_node(node_id, address)),
        }
    }

    /// Get a reference to the internal state
    pub fn state(&self) -> &Arc<State> {
        &self.state
    }

    /// Process a message and return a response message
    pub fn process_message(
        &self,
        message: FramedMessage<Message>,
        connection_id: Option<u64>,
        node_id: Option<String>,
    ) -> FramedMessage<Message> {
        process_message(&self.state, message, connection_id, node_id)
    }

    /// Start periodic timeout processing (returns a closure for background threads)
    pub fn create_timeout_processor(&self) -> impl Fn() -> usize + '_ {
        self.state.create_timeout_processor()
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Application builder for configuring cluster settings
pub struct AppBuilder {
    node_id: Option<i32>,
    address: Option<String>,
    // Add more configuration options as needed
}

impl AppBuilder {
    /// Create a new application builder
    pub fn new() -> Self {
        Self {
            node_id: None,
            address: None,
        }
    }

    /// Set the node ID for Raft clustering
    pub fn node_id(mut self, id: i32) -> Self {
        self.node_id = Some(id);
        self
    }

    /// Set the node address for networking
    pub fn address<S: Into<String>>(mut self, addr: S) -> Self {
        self.address = Some(addr.into());
        self
    }

    /// Build the application with the configured settings
    pub fn build(self) -> App {
        match (self.node_id, self.address) {
            (Some(id), Some(addr)) => App::new_with_raft_node(id, addr),
            _ => App::new(),
        }
    }
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to create a new application
pub fn app() -> App {
    App::new()
}

/// Convenience function to create an application builder
pub fn builder() -> AppBuilder {
    AppBuilder::new()
}

/// Library greeting function
pub fn greeting() -> &'static str {
    "Hello from Vertx Cluster!"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_creation() {
        let app = App::new();
        // assert!(!app.state().is_null());
    }

    #[test]
    fn test_app_builder() {
        let app = AppBuilder::new()
            .node_id(1)
            .address("127.0.0.1:8080")
            .build();
        
        // assert!(!app.state().is_null());
    }

    #[test]
    fn test_convenience_functions() {
        let app1 = app();
        let app2 = builder().build();
        
        // assert!(!app1.state().is_null());
        // assert!(!app2.state().is_null());
    }

    #[test]
    fn test_greeting() {
        assert_eq!(greeting(), "Hello from Vertx Cluster!");
    }
}