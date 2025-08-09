use ethereum_types::{H256, U256};
use ethereum_core::{Block, Header};
// use ethereum_rlp::{Encode, Decode}; // Unused imports
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusMessage {
    pub protocol_version: u8,
    pub network_id: u64,
    pub total_difficulty: U256,
    pub best_hash: H256,
    pub genesis_hash: H256,
    pub fork_id: Option<ForkId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkId {
    pub hash: [u8; 4],
    pub next: u64,
}

#[derive(Debug, Clone)]
pub struct NewBlockHashesMessage {
    pub hashes: Vec<(H256, U256)>, // (hash, number)
}

#[derive(Debug, Clone)]
pub struct GetBlockHeadersMessage {
    pub start: HashOrNumber,
    pub limit: u64,
    pub skip: u64,
    pub reverse: bool,
}

#[derive(Debug, Clone)]
pub enum HashOrNumber {
    Hash(H256),
    Number(U256),
}

#[derive(Debug, Clone)]
pub struct BlockHeadersMessage {
    pub headers: Vec<Header>,
}

#[derive(Debug, Clone)]
pub struct GetBlockBodiesMessage {
    pub hashes: Vec<H256>,
}

#[derive(Debug, Clone)]
pub struct BlockBodiesMessage {
    pub bodies: Vec<BlockBody>,
}

#[derive(Debug, Clone)]
pub struct BlockBody {
    pub transactions: Vec<Vec<u8>>, // RLP encoded transactions
    pub uncles: Vec<Header>,
}

#[derive(Debug, Clone)]
pub struct NewBlockMessage {
    pub block: Block,
    pub total_difficulty: U256,
}

#[derive(Debug, Clone)]
pub struct NewPooledTransactionHashesMessage {
    pub types: Vec<u8>,
    pub sizes: Vec<u32>,
    pub hashes: Vec<H256>,
}

#[derive(Debug, Clone)]
pub struct GetPooledTransactionsMessage {
    pub hashes: Vec<H256>,
}

#[derive(Debug, Clone)]
pub struct PooledTransactionsMessage {
    pub transactions: Vec<Vec<u8>>, // RLP encoded transactions
}

#[derive(Debug, Clone)]
pub struct GetReceiptsMessage {
    pub hashes: Vec<H256>,
}

#[derive(Debug, Clone)]
pub struct ReceiptsMessage {
    pub receipts: Vec<Vec<Receipt>>,
}

#[derive(Debug, Clone)]
pub struct Receipt {
    pub status: bool,
    pub cumulative_gas_used: U256,
    pub logs_bloom: [u8; 256],
    pub logs: Vec<Log>,
}

#[derive(Debug, Clone)]
pub struct Log {
    pub address: H256,
    pub topics: Vec<H256>,
    pub data: Vec<u8>,
}

impl StatusMessage {
    pub fn encode(&self) -> Vec<u8> {
        // Simplified encoding - real implementation would use RLP
        bincode::serialize(self).unwrap_or_default()
    }
    
    pub fn decode(data: &[u8]) -> Result<Self, crate::NetworkError> {
        bincode::deserialize(data)
            .map_err(|e| crate::NetworkError::InvalidMessage(e.to_string()))
    }
}