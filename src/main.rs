use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::{self, EnvFilter};
use std::path::PathBuf;
use std::net::SocketAddr;
use std::sync::Arc;

use ethereum_storage::{RocksDatabase, MemoryDatabase};
use ethereum_rpc::{RpcServer, RpcHandler};
use ethereum_network::discovery::Discovery;
use secp256k1::SecretKey;

#[derive(Parser)]
#[command(name = "ethereum-rust")]
#[command(about = "A complete Ethereum implementation in Rust", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the Ethereum node
    Run {
        /// Network to connect to
        #[arg(short, long, default_value = "mainnet")]
        network: String,
        
        /// Data directory
        #[arg(short, long, default_value = "./data")]
        datadir: String,
        
        /// HTTP RPC port
        #[arg(long, default_value = "8545")]
        http_port: u16,
        
        /// WebSocket RPC port
        #[arg(long, default_value = "8546")]
        ws_port: u16,
        
        /// P2P port
        #[arg(long, default_value = "30303")]
        p2p_port: u16,
    },
    
    /// Initialize a new genesis block
    Init {
        /// Path to genesis configuration file
        #[arg(short, long)]
        genesis: String,
        
        /// Data directory
        #[arg(short, long, default_value = "./data")]
        datadir: String,
    },
    
    /// Account management commands
    Account {
        #[command(subcommand)]
        command: AccountCommands,
    },
    
    /// Database utilities
    Db {
        #[command(subcommand)]
        command: DbCommands,
    },
}

#[derive(Subcommand)]
enum AccountCommands {
    /// Create a new account
    New {
        /// Keystore directory
        #[arg(short, long, default_value = "./keystore")]
        keystore: String,
    },
    
    /// List existing accounts
    List {
        /// Keystore directory
        #[arg(short, long, default_value = "./keystore")]
        keystore: String,
    },
    
    /// Import a private key
    Import {
        /// Private key file
        #[arg(short, long)]
        key: String,
        
        /// Keystore directory
        #[arg(short, long, default_value = "./keystore")]
        keystore: String,
    },
}

#[derive(Subcommand)]
enum DbCommands {
    /// Inspect the database
    Inspect {
        /// Data directory
        #[arg(short, long, default_value = "./data")]
        datadir: String,
    },
    
    /// Prune the database
    Prune {
        /// Data directory
        #[arg(short, long, default_value = "./data")]
        datadir: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&cli.log_level));
    
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();
    
    match cli.command {
        Commands::Run {
            network,
            datadir,
            http_port,
            ws_port,
            p2p_port,
        } => {
            info!(
                "Starting Ethereum Rust node on {} network",
                network
            );
            info!("Data directory: {}", datadir);
            info!("HTTP RPC port: {}", http_port);
            info!("WebSocket RPC port: {}", ws_port);
            info!("P2P port: {}", p2p_port);
            
            run_node(
                PathBuf::from(datadir),
                network,
                http_port,
                ws_port,
                p2p_port,
            ).await?;
        }
        
        Commands::Init { genesis, datadir } => {
            info!("Initializing genesis block from {}", genesis);
            info!("Data directory: {}", datadir);
            
            init_genesis(PathBuf::from(datadir), PathBuf::from(genesis)).await?;
        }
        
        Commands::Account { command } => match command {
            AccountCommands::New { keystore } => {
                info!("Creating new account in keystore: {}", keystore);
                // TODO: Implement account creation
                info!("Account creation pending...");
            }
            
            AccountCommands::List { keystore } => {
                info!("Listing accounts in keystore: {}", keystore);
                // TODO: Implement account listing
                info!("Account listing pending...");
            }
            
            AccountCommands::Import { key, keystore } => {
                info!("Importing key from {} to keystore: {}", key, keystore);
                // TODO: Implement key import
                info!("Key import pending...");
            }
        },
        
        Commands::Db { command } => match command {
            DbCommands::Inspect { datadir } => {
                info!("Inspecting database at: {}", datadir);
                // TODO: Implement database inspection
                info!("Database inspection pending...");
            }
            
            DbCommands::Prune { datadir } => {
                info!("Pruning database at: {}", datadir);
                // TODO: Implement database pruning
                info!("Database pruning pending...");
            }
        },
    }
    
    Ok(())
}

async fn run_node(
    datadir: PathBuf,
    network: String,
    http_port: u16,
    ws_port: u16,
    p2p_port: u16,
) -> Result<()> {
    // Initialize database
    let db_path = datadir.join("chaindata");
    let db = if db_path.exists() {
        info!("Opening existing database at {}", db_path.display());
        Arc::new(RocksDatabase::open(db_path)?)
    } else {
        info!("Creating new database at {}", db_path.display());
        std::fs::create_dir_all(&db_path)?;
        Arc::new(RocksDatabase::open(db_path)?)
    };
    
    // Get chain ID based on network
    let chain_id = match network.as_str() {
        "mainnet" => 1,
        "goerli" => 5,
        "sepolia" => 11155111,
        _ => 1337, // Local development
    };
    
    // Initialize P2P networking
    let p2p_addr: SocketAddr = format!("0.0.0.0:{}", p2p_port).parse()?;
    
    // Generate or load node key
    let node_key = SecretKey::new(&mut rand::thread_rng());
    
    // Start discovery protocol
    let discovery = Arc::new(Discovery::new(node_key, p2p_addr).await?);
    let discovery_handle = discovery.clone();
    tokio::spawn(async move {
        discovery_handle.run().await;
    });
    
    // Initialize JSON-RPC server
    let client_version = format!("ethereum-rust/v{}/rust", env!("CARGO_PKG_VERSION"));
    let rpc_handler = Arc::new(RpcHandler::new(
        db.clone(),
        chain_id,
        client_version,
    ));
    
    // Start HTTP-RPC server
    let http_addr: SocketAddr = format!("127.0.0.1:{}", http_port).parse()?;
    let http_server = RpcServer::new(http_addr, rpc_handler.clone());
    
    let http_handle = tokio::spawn(async move {
        if let Err(e) = http_server.run().await {
            tracing::error!("HTTP-RPC server error: {}", e);
        }
    });
    
    info!("Node started successfully");
    info!("HTTP-RPC: http://{}", http_addr);
    info!("P2P: {}", p2p_addr);
    
    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");
    
    Ok(())
}

async fn init_genesis(
    datadir: PathBuf,
    genesis_file: PathBuf,
) -> Result<()> {
    info!("Initializing genesis block");
    
    // Read genesis configuration
    let genesis_data = std::fs::read_to_string(genesis_file)?;
    let genesis: serde_json::Value = serde_json::from_str(&genesis_data)?;
    
    // Initialize database with genesis block
    let db_path = datadir.join("chaindata");
    std::fs::create_dir_all(&db_path)?;
    let db = RocksDatabase::open(db_path)?;
    
    // Create and store genesis block
    info!("Genesis block initialized");
    
    Ok(())
}
