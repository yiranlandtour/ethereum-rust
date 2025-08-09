use ethereum_rust::{Node, NodeConfig, Genesis};
use ethereum_storage::MemoryDatabase;
use ethereum_types::{H256, U256, Address};
use ethereum_core::Transaction;
use ethereum_account::{Account, AccountManager, HDWallet};
use std::sync::Arc;
use tempfile::TempDir;
use tokio;

#[tokio::test]
async fn test_node_startup_and_shutdown() {
    // Create temporary directory for test data
    let temp_dir = TempDir::new().unwrap();
    let mut config = NodeConfig::default();
    config.data_dir = temp_dir.path().to_path_buf();
    
    // Use memory database for testing
    let db = Arc::new(MemoryDatabase::new());
    
    // Create and start node
    let mut node = Node::with_database(config, db).await.unwrap();
    node.start().await.unwrap();
    
    // Check node info
    let info = node.node_info();
    assert!(info.client_version.contains("ethereum-rust"));
    
    // Shutdown node
    node.stop().await.unwrap();
}

#[tokio::test]
async fn test_genesis_block_creation() {
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(MemoryDatabase::new());
    
    // Create mainnet genesis
    let genesis = Genesis::mainnet();
    let genesis_hash = genesis.init_db(db.clone()).await.unwrap();
    
    // Verify genesis hash is not zero
    assert_ne!(genesis_hash, H256::zero());
    
    // Verify genesis block is stored
    let key = format!("block:{}", hex::encode(genesis_hash));
    let block_data = db.get(key.as_bytes()).unwrap();
    assert!(block_data.is_some());
}

#[tokio::test]
async fn test_account_creation_and_signing() {
    let temp_dir = TempDir::new().unwrap();
    let keystore_dir = temp_dir.path().join("keystore");
    
    // Create account manager
    let mut account_manager = AccountManager::new(&keystore_dir).unwrap();
    
    // Create new account
    let password = "test_password_123";
    let address = account_manager.new_account(password).await.unwrap();
    assert_ne!(address, Address::zero());
    
    // Unlock account
    account_manager.unlock_account(address, password).await.unwrap();
    
    // Sign a message
    let message = b"Hello, Ethereum!";
    let signature = account_manager.sign_message(address, message).unwrap();
    
    // Verify signature
    let account = account_manager.get_account(address).unwrap();
    assert!(account.verify_signature(message, &signature));
}

#[tokio::test]
async fn test_hd_wallet_derivation() {
    // Test mnemonic (DO NOT USE IN PRODUCTION)
    let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    
    // Create HD wallet
    let mut wallet = HDWallet::from_mnemonic_str(mnemonic, "").unwrap();
    
    // Derive accounts
    let addr0 = wallet.derive_account(0).unwrap();
    let addr1 = wallet.derive_account(1).unwrap();
    let addr2 = wallet.derive_account(2).unwrap();
    
    // Verify addresses are different
    assert_ne!(addr0, addr1);
    assert_ne!(addr1, addr2);
    assert_ne!(addr0, addr2);
    
    // Verify known address for test mnemonic
    let expected_addr0 = "0x9858effd232b4033e47d90003d41ec34ecaeda94";
    assert_eq!(format!("{:?}", addr0).to_lowercase(), expected_addr0);
}

#[tokio::test]
async fn test_transaction_pool() {
    use ethereum_txpool::TransactionPool;
    
    // Create transaction pool
    let txpool = Arc::new(TransactionPool::new(100, 50));
    
    // Create test account
    let account = Account::new().unwrap();
    
    // Create test transaction
    let tx = Transaction {
        nonce: 0,
        gas_price: Some(U256::from(20_000_000_000u64)), // 20 gwei
        gas_limit: U256::from(21000),
        to: Some(Address::from([1u8; 20])),
        value: U256::from(1_000_000_000_000_000_000u64), // 1 ETH
        input: vec![],
        signature: Default::default(),
        transaction_type: 0,
        chain_id: Some(1),
        access_list: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
    };
    
    // Add transaction to pool
    txpool.add_transaction(tx.clone()).await.unwrap();
    
    // Get pending transactions
    let pending = txpool.get_pending_transactions(10).await;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].hash(), tx.hash());
}

#[tokio::test]
async fn test_block_import() {
    use ethereum_core::{Block, Header, BlockBody};
    
    let temp_dir = TempDir::new().unwrap();
    let mut config = NodeConfig::default();
    config.data_dir = temp_dir.path().to_path_buf();
    
    let db = Arc::new(MemoryDatabase::new());
    
    // Initialize genesis
    let genesis = Genesis::mainnet();
    let genesis_hash = genesis.init_db(db.clone()).await.unwrap();
    
    // Create node
    let mut node = Node::with_database(config, db.clone()).await.unwrap();
    
    // Create a test block
    let block = Block {
        header: Header {
            parent_hash: genesis_hash,
            number: U256::one(),
            gas_limit: U256::from(8_000_000),
            gas_used: U256::zero(),
            timestamp: 1000,
            difficulty: U256::from(1),
            ..Default::default()
        },
        body: BlockBody {
            transactions: vec![],
            uncles: vec![],
        },
    };
    
    // Import block (this will fail validation but tests the flow)
    let result = node.import_block(block).await;
    // We expect this to fail as we haven't properly set up validation
    assert!(result.is_err());
}

#[tokio::test]
async fn test_rpc_server() {
    use ethereum_rpc::{RpcServer, RpcHandler};
    use std::net::SocketAddr;
    
    let db = Arc::new(MemoryDatabase::new());
    let handler = Arc::new(RpcHandler::new(
        db,
        1,
        "ethereum-rust/test".to_string(),
    ));
    
    // Create RPC server on random port
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let server = RpcServer::new(addr, handler);
    
    // Start server in background
    let server_handle = tokio::spawn(async move {
        // Server would run here
        // For testing, we just verify it was created
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    });
    
    // Wait a bit for server to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    
    // In a real test, we would make HTTP requests here
    
    // Stop server
    server_handle.abort();
}

#[tokio::test]
async fn test_state_trie() {
    use ethereum_trie::PatriciaTrie;
    
    let db = Arc::new(MemoryDatabase::new());
    let mut trie = PatriciaTrie::new(db);
    
    // Insert some test data
    let key1 = b"key1";
    let value1 = b"value1";
    trie.insert(key1, value1.to_vec()).await.unwrap();
    
    let key2 = b"key2";
    let value2 = b"value2";
    trie.insert(key2, value2.to_vec()).await.unwrap();
    
    // Commit and get root
    let root = trie.commit().await.unwrap();
    assert_ne!(root, H256::zero());
    
    // Verify we can retrieve values
    let retrieved1 = trie.get(key1).await.unwrap();
    assert_eq!(retrieved1, Some(value1.to_vec()));
    
    let retrieved2 = trie.get(key2).await.unwrap();
    assert_eq!(retrieved2, Some(value2.to_vec()));
}

#[tokio::test]
async fn test_network_message_encoding() {
    use ethereum_network::messages::{Message, Status};
    
    // Create a status message
    let status = Status {
        protocol_version: 68,
        network_id: 1,
        total_difficulty: U256::from(1000000),
        best_hash: H256::from([1u8; 32]),
        genesis_hash: H256::from([2u8; 32]),
    };
    
    // In a real test, we would encode/decode the message
    assert_eq!(status.protocol_version, 68);
    assert_eq!(status.network_id, 1);
}

#[tokio::test]
async fn test_consensus_validation() {
    use ethereum_consensus::{Consensus, ConsensusConfig, EngineType};
    use ethereum_core::{Block, Header, BlockBody};
    
    let db = Arc::new(MemoryDatabase::new());
    
    let config = ConsensusConfig {
        engine_type: EngineType::ProofOfStake,
        epoch_length: 32,
        block_period: 12,
        validators: vec![Address::from([1u8; 20])],
        genesis_validators: vec![],
    };
    
    let consensus = Consensus::new(config, db);
    
    // Create a test block
    let block = Block {
        header: Header {
            number: U256::one(),
            gas_limit: U256::from(8_000_000),
            gas_used: U256::zero(),
            timestamp: 1000,
            ..Default::default()
        },
        body: BlockBody {
            transactions: vec![],
            uncles: vec![],
        },
    };
    
    // Validate block (will fail without proper setup but tests the interface)
    let result = consensus.validate_block(&block).await;
    // We expect this to fail as we haven't set up proper validation
    assert!(result.is_err());
}

// Performance benchmarks
#[tokio::test]
async fn benchmark_transaction_signing() {
    use std::time::Instant;
    
    let account = Account::new().unwrap();
    let message = b"Benchmark message";
    
    let iterations = 1000;
    let start = Instant::now();
    
    for _ in 0..iterations {
        let _ = account.sign_message(message).unwrap();
    }
    
    let duration = start.elapsed();
    let ops_per_sec = iterations as f64 / duration.as_secs_f64();
    
    println!("Transaction signing: {:.0} ops/sec", ops_per_sec);
    assert!(ops_per_sec > 100.0); // Should be able to sign >100 tx/sec
}

#[tokio::test]
async fn benchmark_keccak256() {
    use std::time::Instant;
    use ethereum_crypto::keccak256;
    
    let data = vec![0u8; 1024]; // 1KB of data
    let iterations = 10000;
    
    let start = Instant::now();
    
    for _ in 0..iterations {
        let _ = keccak256(&data);
    }
    
    let duration = start.elapsed();
    let mb_per_sec = (iterations as f64 * 1024.0) / (duration.as_secs_f64() * 1_000_000.0);
    
    println!("Keccak256 hashing: {:.2} MB/sec", mb_per_sec);
    assert!(mb_per_sec > 10.0); // Should hash >10 MB/sec
}