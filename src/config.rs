use std::path::{Path, PathBuf};
use std::fs;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use ethereum_types::U256;

/// Complete node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Node configuration
    pub node: NodeConfig,
    /// Network configuration
    pub network: NetworkConfig,
    /// RPC configuration
    pub rpc: RpcConfig,
    /// Mining/Validation configuration
    pub mining: MiningConfig,
    /// Transaction pool configuration
    pub txpool: TxPoolConfig,
    /// Database configuration
    pub database: DatabaseConfig,
    /// Logging configuration
    pub log: LogConfig,
    /// Metrics configuration
    pub metrics: MetricsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeConfig {
    /// Node name
    pub name: String,
    /// Data directory
    pub datadir: PathBuf,
    /// Keystore directory
    pub keystore: PathBuf,
    /// IPC path
    pub ipc_path: Option<PathBuf>,
    /// Enable archive mode
    pub archive: bool,
    /// Cache size in MB
    pub cache: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    /// Network ID
    pub network_id: u64,
    /// Chain ID
    pub chain_id: u64,
    /// Listen address
    pub listen_addr: String,
    /// P2P port
    pub port: u16,
    /// Maximum number of peers
    pub max_peers: usize,
    /// Maximum number of pending peers
    pub max_pending_peers: usize,
    /// Bootnodes
    pub bootnodes: Vec<String>,
    /// Trusted nodes
    pub trusted_nodes: Vec<String>,
    /// Enable discovery
    pub discovery: bool,
    /// Discovery port
    pub discovery_port: u16,
    /// NAT traversal mode
    pub nat: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RpcConfig {
    /// HTTP configuration
    pub http: HttpConfig,
    /// WebSocket configuration
    pub ws: WsConfig,
    /// IPC configuration
    pub ipc: IpcConfig,
    /// Enabled APIs
    pub apis: Vec<String>,
    /// Gas price oracle configuration
    pub gpo: GasPriceOracleConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpConfig {
    /// Enable HTTP RPC
    pub enabled: bool,
    /// HTTP host
    pub host: String,
    /// HTTP port
    pub port: u16,
    /// CORS domains
    pub cors: Vec<String>,
    /// Virtual hosts
    pub vhosts: Vec<String>,
    /// Request timeout in seconds
    pub timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WsConfig {
    /// Enable WebSocket RPC
    pub enabled: bool,
    /// WebSocket host
    pub host: String,
    /// WebSocket port
    pub port: u16,
    /// Allowed origins
    pub origins: Vec<String>,
    /// Maximum connections
    pub max_connections: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IpcConfig {
    /// Enable IPC
    pub enabled: bool,
    /// IPC path
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GasPriceOracleConfig {
    /// Number of blocks to check
    pub blocks: u64,
    /// Percentile to use
    pub percentile: u64,
    /// Default gas price
    pub default: U256,
    /// Maximum gas price
    pub max_price: U256,
    /// Ignore transactions with gas price below this
    pub ignore_price: U256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MiningConfig {
    /// Enable mining/validation
    pub enabled: bool,
    /// Coinbase address
    pub coinbase: Option<String>,
    /// Extra data
    pub extra_data: String,
    /// Gas floor target
    pub gas_floor: U256,
    /// Gas ceiling
    pub gas_ceil: U256,
    /// Gas price
    pub gas_price: U256,
    /// Number of threads
    pub threads: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TxPoolConfig {
    /// Maximum number of pending transactions
    pub max_pending: usize,
    /// Maximum number of queued transactions
    pub max_queued: usize,
    /// Maximum time for pending transactions
    pub lifetime: u64,
    /// Minimum gas price
    pub price_floor: U256,
    /// Price bump percentage for replacement
    pub price_bump: u64,
    /// Maximum account slots
    pub account_slots: usize,
    /// Global slots
    pub global_slots: usize,
    /// Maximum account queue
    pub account_queue: usize,
    /// Global queue
    pub global_queue: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    /// Database cache size in MB
    pub cache_size: usize,
    /// Number of open files
    pub max_open_files: i32,
    /// Compaction configuration
    pub compaction: CompactionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CompactionConfig {
    /// Enable auto compaction
    pub auto: bool,
    /// Compaction interval in seconds
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    /// Log level
    pub level: String,
    /// Log file path
    pub file: Option<PathBuf>,
    /// Maximum log file size in MB
    pub max_size: usize,
    /// Maximum number of log files
    pub max_files: usize,
    /// Enable JSON logging
    pub json: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricsConfig {
    /// Enable metrics
    pub enabled: bool,
    /// Metrics host
    pub host: String,
    /// Metrics port
    pub port: u16,
    /// Metrics prefix
    pub prefix: String,
    /// Enable expensive metrics
    pub expensive: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            node: NodeConfig::default(),
            network: NetworkConfig::default(),
            rpc: RpcConfig::default(),
            mining: MiningConfig::default(),
            txpool: TxPoolConfig::default(),
            database: DatabaseConfig::default(),
            log: LogConfig::default(),
            metrics: MetricsConfig::default(),
        }
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            name: "ethereum-rust".to_string(),
            datadir: PathBuf::from("./data"),
            keystore: PathBuf::from("./keystore"),
            ipc_path: None,
            archive: false,
            cache: 1024,
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            network_id: 1,
            chain_id: 1,
            listen_addr: "0.0.0.0".to_string(),
            port: 30303,
            max_peers: 25,
            max_pending_peers: 50,
            bootnodes: vec![],
            trusted_nodes: vec![],
            discovery: true,
            discovery_port: 30303,
            nat: "any".to_string(),
        }
    }
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            http: HttpConfig::default(),
            ws: WsConfig::default(),
            ipc: IpcConfig::default(),
            apis: vec![
                "eth".to_string(),
                "net".to_string(),
                "web3".to_string(),
            ],
            gpo: GasPriceOracleConfig::default(),
        }
    }
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            host: "127.0.0.1".to_string(),
            port: 8545,
            cors: vec!["*".to_string()],
            vhosts: vec!["localhost".to_string()],
            timeout: 120,
        }
    }
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            host: "127.0.0.1".to_string(),
            port: 8546,
            origins: vec!["*".to_string()],
            max_connections: 100,
        }
    }
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            path: None,
        }
    }
}

impl Default for GasPriceOracleConfig {
    fn default() -> Self {
        Self {
            blocks: 20,
            percentile: 60,
            default: U256::from(1_000_000_000), // 1 gwei
            max_price: U256::from(500_000_000_000), // 500 gwei
            ignore_price: U256::from(2), // 2 wei
        }
    }
}

impl Default for MiningConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            coinbase: None,
            extra_data: String::new(),
            gas_floor: U256::from(8_000_000),
            gas_ceil: U256::from(8_000_000),
            gas_price: U256::from(1_000_000_000), // 1 gwei
            threads: 0, // Auto-detect
        }
    }
}

impl Default for TxPoolConfig {
    fn default() -> Self {
        Self {
            max_pending: 4096,
            max_queued: 1024,
            lifetime: 10800, // 3 hours
            price_floor: U256::from(1_000_000_000), // 1 gwei
            price_bump: 10, // 10%
            account_slots: 16,
            global_slots: 4096,
            account_queue: 64,
            global_queue: 1024,
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            cache_size: 1024,
            max_open_files: 1024,
            compaction: CompactionConfig::default(),
        }
    }
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            auto: true,
            interval: 3600, // 1 hour
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            file: None,
            max_size: 100,
            max_files: 10,
            json: false,
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "127.0.0.1".to_string(),
            port: 6060,
            prefix: "ethereum".to_string(),
            expensive: false,
        }
    }
}

impl Config {
    /// Load configuration from file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)
            .context("Failed to read configuration file")?;
        
        let config: Config = toml::from_str(&content)
            .context("Failed to parse configuration")?;
        
        config.validate()?;
        
        Ok(config)
    }
    
    /// Save configuration to file
    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize configuration")?;
        
        fs::write(path, content)
            .context("Failed to write configuration file")?;
        
        Ok(())
    }
    
    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate network configuration
        if self.network.max_peers == 0 {
            anyhow::bail!("max_peers must be greater than 0");
        }
        
        // Validate RPC configuration
        if self.rpc.apis.is_empty() && (self.rpc.http.enabled || self.rpc.ws.enabled) {
            anyhow::bail!("At least one API must be enabled when RPC is enabled");
        }
        
        // Validate transaction pool configuration
        if self.txpool.max_pending == 0 {
            anyhow::bail!("max_pending must be greater than 0");
        }
        
        if self.txpool.price_bump > 100 {
            anyhow::bail!("price_bump must be between 0 and 100");
        }
        
        Ok(())
    }
    
    /// Get configuration for specific network
    pub fn for_network(network: &str) -> Result<Self> {
        let mut config = Config::default();
        
        match network.to_lowercase().as_str() {
            "mainnet" | "main" => {
                config.network.network_id = 1;
                config.network.chain_id = 1;
                // Add mainnet bootnodes
                config.network.bootnodes = vec![
                    "enode://d860a01f9722d78051619d1e2351aba3f43f943f6f00718d1b9baa4101932a1f5011f16bb2b1bb35db20d6fe28fa0bf09636d26a87d31de9ec6203eeedb1f666@18.138.108.67:30303".to_string(),
                    "enode://22a8232c3abc76a16ae9d6c3b164f98775fe226f0917b0ca871128a74a8e9630b458460865bab457221f1d448dd9791d24c4e5d88786180ac185df813a68d4de@3.209.45.79:30303".to_string(),
                ];
            }
            "goerli" => {
                config.network.network_id = 5;
                config.network.chain_id = 5;
                // Add Goerli bootnodes
                config.network.bootnodes = vec![
                    "enode://011f758e6552d105183b1761c5e2dea0111bc20fd5f6422bc7f91e0fabbec9a6595caf6239b37feb773dddd3f87240d99d859431891e4a642cf2a0a9e6cbb98a@51.141.78.53:30303".to_string(),
                    "enode://176b9417f511d05b6b2cf3e34b756cf0a7096b3094572a8f6ef4cdcb9d1f9d00683bf0f83347eebdf3b81c3521c2332086d9592802230bf528eaf606a1d9677b@13.93.54.137:30303".to_string(),
                ];
            }
            "sepolia" => {
                config.network.network_id = 11155111;
                config.network.chain_id = 11155111;
                // Add Sepolia bootnodes
                config.network.bootnodes = vec![
                    "enode://9246d00bc8fd1742e5ad2428b80fc4dc45d786283e05ef6edbd9002cbc335d40998444732fbe921cb88e1d2c73d1b1de53bae6a2237996e9bfe14f871baf7066@18.168.182.86:30303".to_string(),
                ];
            }
            _ => anyhow::bail!("Unknown network: {}", network),
        }
        
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.network.chain_id, 1);
        assert_eq!(config.rpc.http.port, 8545);
    }
    
    #[test]
    fn test_validation() {
        let mut config = Config::default();
        
        // Valid configuration
        assert!(config.validate().is_ok());
        
        // Invalid max_peers
        config.network.max_peers = 0;
        assert!(config.validate().is_err());
        config.network.max_peers = 25;
        
        // Invalid price_bump
        config.txpool.price_bump = 101;
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_network_config() {
        let mainnet = Config::for_network("mainnet").unwrap();
        assert_eq!(mainnet.network.chain_id, 1);
        
        let goerli = Config::for_network("goerli").unwrap();
        assert_eq!(goerli.network.chain_id, 5);
        
        let sepolia = Config::for_network("sepolia").unwrap();
        assert_eq!(sepolia.network.chain_id, 11155111);
    }
}