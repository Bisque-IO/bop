pub mod codec;
pub mod config;
pub mod manager;
pub mod network;
pub(crate) mod manifest_mdbx;
pub mod rpc_server;
pub mod storage;
pub mod storage_impl;
pub mod tcp_transport;
pub mod type_config;

#[cfg(test)]
mod test_support;

pub use config::MultiRaftConfig;
pub use manager::MultiRaftManager;
pub use network::{MultiRaftNetwork, MultiRaftNetworkFactory, MultiplexedTransport};
pub use storage::MultiRaftLogStorage;

// Maniac-specific implementations
pub use rpc_server::{
    ManiacRpcServer, ManiacRpcServerConfig,
    protocol::{ResponseMessage, RpcMessage},
};
pub use storage_impl::{GroupLogStorage, MultiplexedLogStorage, MultiplexedStorageConfig};
pub use tcp_transport::{
    DefaultNodeRegistry, ManiacTcpTransport, ManiacTcpTransportConfig, ManiacTransportError,
    NodeAddressResolver,
};
