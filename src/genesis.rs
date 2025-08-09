use std::collections::HashMap;
use std::path::Path;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use ethereum_types::{H256, U256, Address, Bloom};
use ethereum_core::{Block, Header, BlockBody, Account};
use ethereum_storage::Database;
use ethereum_crypto::keccak256;
use ethereum_trie::PatriciaTrie;
use std::sync::Arc;

/// Genesis configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenesisConfig {
    /// Chain configuration
    pub config: ChainConfig,
    /// Nonce for mining
    pub nonce: String,
    /// Timestamp
    pub timestamp: String,
    /// Extra data
    pub extra_data: String,
    /// Gas limit
    pub gas_limit: String,
    /// Difficulty
    pub difficulty: String,
    /// Mix hash
    pub mix_hash: String,
    /// Coinbase
    pub coinbase: String,
    /// Pre-allocated accounts
    pub alloc: HashMap<String, GenesisAccount>,
    /// Parent hash (usually zero)
    pub parent_hash: Option<String>,
}

/// Chain configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainConfig {
    pub chain_id: u64,
    pub homestead_block: Option<u64>,
    pub eip150_block: Option<u64>,
    pub eip155_block: Option<u64>,
    pub eip158_block: Option<u64>,
    pub byzantium_block: Option<u64>,
    pub constantinople_block: Option<u64>,
    pub petersburg_block: Option<u64>,
    pub istanbul_block: Option<u64>,
    pub berlin_block: Option<u64>,
    pub london_block: Option<u64>,
    pub arrow_glacier_block: Option<u64>,
    pub gray_glacier_block: Option<u64>,
    pub merge_netsplit_block: Option<u64>,
    pub shanghai_time: Option<u64>,
    pub cancun_time: Option<u64>,
    pub terminal_total_difficulty: Option<String>,
    pub terminal_total_difficulty_passed: Option<bool>,
}

/// Genesis account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisAccount {
    /// Balance
    pub balance: String,
    /// Nonce
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<u64>,
    /// Code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Storage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<HashMap<String, String>>,
}

/// Genesis block builder
pub struct Genesis {
    config: GenesisConfig,
}

impl Genesis {
    /// Load genesis configuration from file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .context("Failed to read genesis file")?;
        
        let config: GenesisConfig = serde_json::from_str(&content)
            .context("Failed to parse genesis configuration")?;
        
        Ok(Self { config })
    }
    
    /// Load genesis configuration from JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        let config: GenesisConfig = serde_json::from_str(json)
            .context("Failed to parse genesis configuration")?;
        
        Ok(Self { config })
    }
    
    /// Get default mainnet genesis
    pub fn mainnet() -> Self {
        Self {
            config: GenesisConfig {
                config: ChainConfig {
                    chain_id: 1,
                    homestead_block: Some(1150000),
                    eip150_block: Some(2463000),
                    eip155_block: Some(2675000),
                    eip158_block: Some(2675000),
                    byzantium_block: Some(4370000),
                    constantinople_block: Some(7280000),
                    petersburg_block: Some(7280000),
                    istanbul_block: Some(9069000),
                    berlin_block: Some(12244000),
                    london_block: Some(12965000),
                    arrow_glacier_block: Some(13773000),
                    gray_glacier_block: Some(15050000),
                    merge_netsplit_block: Some(15537394),
                    shanghai_time: Some(1681338455),
                    cancun_time: None,
                    terminal_total_difficulty: Some("58750000000000000000000".to_string()),
                    terminal_total_difficulty_passed: Some(true),
                },
                nonce: "0x42".to_string(),
                timestamp: "0x0".to_string(),
                extra_data: "0x11bbe8db4e347b4e8c937c1c8370e4b5ed33adb3db69cbdb7a38e1e50b1b82fa".to_string(),
                gas_limit: "0x1388".to_string(),
                difficulty: "0x400000000".to_string(),
                mix_hash: "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
                coinbase: "0x0000000000000000000000000000000000000000".to_string(),
                alloc: HashMap::new(),
                parent_hash: None,
            },
        }
    }
    
    /// Get default Goerli testnet genesis
    pub fn goerli() -> Self {
        Self {
            config: GenesisConfig {
                config: ChainConfig {
                    chain_id: 5,
                    homestead_block: Some(0),
                    eip150_block: Some(0),
                    eip155_block: Some(0),
                    eip158_block: Some(0),
                    byzantium_block: Some(0),
                    constantinople_block: Some(0),
                    petersburg_block: Some(0),
                    istanbul_block: Some(1561651),
                    berlin_block: Some(4460644),
                    london_block: Some(5062605),
                    arrow_glacier_block: None,
                    gray_glacier_block: None,
                    merge_netsplit_block: None,
                    shanghai_time: Some(1678832736),
                    cancun_time: None,
                    terminal_total_difficulty: Some("10790000".to_string()),
                    terminal_total_difficulty_passed: Some(true),
                },
                nonce: "0x0".to_string(),
                timestamp: "0x5c51a607".to_string(),
                extra_data: "0x22466c6578692069732061207468696e6722202d204166726900000000000000".to_string(),
                gas_limit: "0xa00000".to_string(),
                difficulty: "0x1".to_string(),
                mix_hash: "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
                coinbase: "0x0000000000000000000000000000000000000000".to_string(),
                alloc: HashMap::new(),
                parent_hash: None,
            },
        }
    }
    
    /// Build genesis block
    pub async fn build_block<D: Database>(&self, db: Arc<D>) -> Result<Block> {
        // Parse values
        let nonce = self.parse_u64(&self.config.nonce)?;
        let timestamp = self.parse_u64(&self.config.timestamp)?;
        let gas_limit = self.parse_u256(&self.config.gas_limit)?;
        let difficulty = self.parse_u256(&self.config.difficulty)?;
        let extra_data = self.parse_bytes(&self.config.extra_data)?;
        let mix_hash = self.parse_h256(&self.config.mix_hash)?;
        let coinbase = self.parse_address(&self.config.coinbase)?;
        
        // Create state trie with pre-allocated accounts
        let state_root = self.build_state(db.clone()).await?;
        
        // Create genesis header
        let header = Header {
            parent_hash: H256::zero(),
            uncles_hash: H256::from([0x1d, 0xcc, 0x4d, 0xe8, 0xde, 0xc7, 0x5d, 0x7a,
                                     0xab, 0x85, 0xb5, 0x67, 0xb6, 0xcc, 0xd4, 0x1a,
                                     0xd3, 0x12, 0x45, 0x1b, 0x94, 0x8a, 0x74, 0x13,
                                     0xf0, 0xa1, 0x42, 0xfd, 0x40, 0xd4, 0x93, 0x47]),
            author: coinbase,
            state_root,
            transactions_root: H256::from([0x56, 0xe8, 0x1f, 0x17, 0x1b, 0xcc, 0x55, 0xa6,
                                           0xff, 0x83, 0x45, 0xe6, 0x92, 0xc0, 0xf8, 0x6e,
                                           0x5b, 0x48, 0xe0, 0x1b, 0x99, 0x6c, 0xad, 0xc0,
                                           0x01, 0x62, 0x2f, 0xb5, 0xe3, 0x63, 0xb4, 0x21]),
            receipts_root: H256::from([0x56, 0xe8, 0x1f, 0x17, 0x1b, 0xcc, 0x55, 0xa6,
                                       0xff, 0x83, 0x45, 0xe6, 0x92, 0xc0, 0xf8, 0x6e,
                                       0x5b, 0x48, 0xe0, 0x1b, 0x99, 0x6c, 0xad, 0xc0,
                                       0x01, 0x62, 0x2f, 0xb5, 0xe3, 0x63, 0xb4, 0x21]),
            bloom: Bloom::default(),
            difficulty,
            number: U256::zero(),
            gas_limit,
            gas_used: U256::zero(),
            timestamp,
            extra_data,
            mix_hash,
            nonce,
        };
        
        // Create genesis block
        let block = Block {
            header,
            body: BlockBody {
                transactions: vec![],
                uncles: vec![],
            },
        };
        
        Ok(block)
    }
    
    /// Build state trie with pre-allocated accounts
    async fn build_state<D: Database>(&self, db: Arc<D>) -> Result<H256> {
        let mut state = PatriciaTrie::new(db.clone());
        
        for (address_str, genesis_account) in &self.config.alloc {
            let address = self.parse_address(address_str)?;
            let balance = self.parse_u256(&genesis_account.balance)?;
            
            // Create account
            let mut account = Account {
                nonce: genesis_account.nonce.unwrap_or(0),
                balance,
                storage_root: H256::zero(),
                code_hash: H256::zero(),
                code: vec![],
            };
            
            // Set code if provided
            if let Some(ref code_str) = genesis_account.code {
                let code = self.parse_bytes(code_str)?;
                account.code_hash = H256(keccak256(&code));
                account.code = code;
            }
            
            // Set storage if provided
            if let Some(ref storage) = genesis_account.storage {
                let storage_root = self.build_storage(db.clone(), storage).await?;
                account.storage_root = storage_root;
            }
            
            // Insert account into state trie
            let account_bytes = bincode::serialize(&account)?;
            state.insert(address.as_bytes(), account_bytes).await
                .context("Failed to insert account into state trie")?;
        }
        
        // Commit state trie and get root
        let state_root = state.commit().await
            .context("Failed to commit state trie")?;
        
        Ok(state_root)
    }
    
    /// Build storage trie for an account
    async fn build_storage<D: Database>(
        &self,
        db: Arc<D>,
        storage: &HashMap<String, String>,
    ) -> Result<H256> {
        let mut storage_trie = PatriciaTrie::new(db);
        
        for (key_str, value_str) in storage {
            let key = self.parse_h256(key_str)?;
            let value = self.parse_h256(value_str)?;
            
            storage_trie.insert(key.as_bytes(), value.as_bytes().to_vec()).await
                .context("Failed to insert storage value")?;
        }
        
        let storage_root = storage_trie.commit().await
            .context("Failed to commit storage trie")?;
        
        Ok(storage_root)
    }
    
    /// Initialize database with genesis block
    pub async fn init_db<D: Database>(&self, db: Arc<D>) -> Result<H256> {
        let block = self.build_block(db.clone()).await?;
        let genesis_hash = block.header.hash();
        
        // Store genesis block
        let block_key = format!("block:{}", hex::encode(genesis_hash));
        db.put(
            block_key.as_bytes(),
            &bincode::serialize(&block)?,
        )?;
        
        // Store block number -> hash mapping
        let number_key = b"block:number:0";
        db.put(number_key, genesis_hash.as_bytes())?;
        
        // Store genesis hash
        db.put(b"genesis", genesis_hash.as_bytes())?;
        
        // Store chain config
        let config_bytes = serde_json::to_vec(&self.config.config)?;
        db.put(b"chain_config", &config_bytes)?;
        
        // Store latest block
        db.put(b"latest_block", &U256::zero().to_le_bytes())?;
        db.put(b"latest_hash", genesis_hash.as_bytes())?;
        
        Ok(genesis_hash)
    }
    
    // Parsing helpers
    
    fn parse_u64(&self, s: &str) -> Result<u64> {
        if s.starts_with("0x") {
            u64::from_str_radix(&s[2..], 16)
                .context("Failed to parse hex u64")
        } else {
            s.parse().context("Failed to parse u64")
        }
    }
    
    fn parse_u256(&self, s: &str) -> Result<U256> {
        if s.starts_with("0x") {
            U256::from_str_radix(&s[2..], 16)
                .context("Failed to parse hex U256")
        } else {
            U256::from_dec_str(s)
                .context("Failed to parse U256")
        }
    }
    
    fn parse_h256(&self, s: &str) -> Result<H256> {
        let bytes = self.parse_bytes(s)?;
        if bytes.len() != 32 {
            anyhow::bail!("Invalid H256 length: {}", bytes.len());
        }
        Ok(H256::from_slice(&bytes))
    }
    
    fn parse_address(&self, s: &str) -> Result<Address> {
        let bytes = self.parse_bytes(s)?;
        if bytes.len() != 20 {
            anyhow::bail!("Invalid address length: {}", bytes.len());
        }
        Ok(Address::from_slice(&bytes))
    }
    
    fn parse_bytes(&self, s: &str) -> Result<Vec<u8>> {
        if s.starts_with("0x") {
            hex::decode(&s[2..])
                .context("Failed to parse hex bytes")
        } else {
            hex::decode(s)
                .context("Failed to parse hex bytes")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethereum_storage::MemoryDatabase;
    
    #[tokio::test]
    async fn test_mainnet_genesis() {
        let genesis = Genesis::mainnet();
        let db = Arc::new(MemoryDatabase::new());
        
        let block = genesis.build_block(db.clone()).await.unwrap();
        assert_eq!(block.header.number, U256::zero());
        assert_eq!(block.header.parent_hash, H256::zero());
    }
    
    #[tokio::test]
    async fn test_goerli_genesis() {
        let genesis = Genesis::goerli();
        let db = Arc::new(MemoryDatabase::new());
        
        let block = genesis.build_block(db.clone()).await.unwrap();
        assert_eq!(block.header.number, U256::zero());
    }
    
    #[test]
    fn test_parse_u256() {
        let genesis = Genesis::mainnet();
        
        let val1 = genesis.parse_u256("0x1388").unwrap();
        assert_eq!(val1, U256::from(5000));
        
        let val2 = genesis.parse_u256("1000000").unwrap();
        assert_eq!(val2, U256::from(1000000));
    }
}