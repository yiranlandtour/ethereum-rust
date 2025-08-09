// Core modules
pub mod config;
pub mod genesis;
pub mod node;

// Re-export commonly used types
pub use config::{Config, NodeConfig, NetworkConfig, RpcConfig};
pub use genesis::{Genesis, GenesisConfig, ChainConfig};
pub use node::{Node, NodeInfo};

// Re-export crate modules
pub use ethereum_core as core;
pub use ethereum_crypto as crypto;
pub use ethereum_network as network;
pub use ethereum_rpc as rpc;
pub use ethereum_storage as storage;
pub use ethereum_types as types;
pub use ethereum_consensus as consensus;
pub use ethereum_evm as evm;
pub use ethereum_filter as filter;
pub use ethereum_sync as sync;
pub use ethereum_trie as trie;
pub use ethereum_txpool as txpool;
pub use ethereum_verification as verification;
pub use ethereum_debug as debug;

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Get client version string
pub fn client_version() -> String {
    format!("ethereum-rust/v{}/rust", VERSION)
}