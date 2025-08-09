use serde::{Deserialize, Serialize};
use serde_json::Value;
use ethereum_types::{H160, H256, U256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
    pub id: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcErrorResponse>,
    pub id: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcErrorResponse {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub number: Option<U256>,
    pub hash: Option<H256>,
    pub parent_hash: H256,
    pub nonce: Option<U256>,
    pub sha3_uncles: H256,
    pub logs_bloom: Option<String>,
    pub transactions_root: H256,
    pub state_root: H256,
    pub receipts_root: H256,
    pub miner: H160,
    pub difficulty: U256,
    pub total_difficulty: Option<U256>,
    pub extra_data: String,
    pub size: U256,
    pub gas_limit: U256,
    pub gas_used: U256,
    pub timestamp: U256,
    pub transactions: Vec<TransactionOrHash>,
    pub uncles: Vec<H256>,
    pub base_fee_per_gas: Option<U256>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TransactionOrHash {
    Hash(H256),
    Transaction(Transaction),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub hash: H256,
    pub nonce: U256,
    pub block_hash: Option<H256>,
    pub block_number: Option<U256>,
    pub transaction_index: Option<U256>,
    pub from: H160,
    pub to: Option<H160>,
    pub value: U256,
    pub gas_price: Option<U256>,
    pub gas: U256,
    pub input: String,
    pub v: U256,
    pub r: U256,
    pub s: U256,
    #[serde(rename = "type")]
    pub tx_type: Option<U256>,
    pub max_fee_per_gas: Option<U256>,
    pub max_priority_fee_per_gas: Option<U256>,
    pub access_list: Option<Vec<AccessListItem>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessListItem {
    pub address: H160,
    pub storage_keys: Vec<H256>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub transaction_hash: H256,
    pub transaction_index: U256,
    pub block_hash: H256,
    pub block_number: U256,
    pub from: H160,
    pub to: Option<H160>,
    pub cumulative_gas_used: U256,
    pub gas_used: U256,
    pub contract_address: Option<H160>,
    pub logs: Vec<Log>,
    pub logs_bloom: String,
    pub status: U256,
    pub effective_gas_price: U256,
    #[serde(rename = "type")]
    pub tx_type: U256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Log {
    pub removed: bool,
    pub log_index: U256,
    pub transaction_index: U256,
    pub transaction_hash: H256,
    pub block_hash: H256,
    pub block_number: U256,
    pub address: H160,
    pub data: String,
    pub topics: Vec<H256>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallRequest {
    pub from: Option<H160>,
    pub to: Option<H160>,
    pub gas: Option<U256>,
    pub gas_price: Option<U256>,
    pub max_fee_per_gas: Option<U256>,
    pub max_priority_fee_per_gas: Option<U256>,
    pub value: Option<U256>,
    pub data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterOptions {
    pub from_block: Option<BlockNumber>,
    pub to_block: Option<BlockNumber>,
    pub address: Option<FilterAddress>,
    pub topics: Option<Vec<Option<FilterTopic>>>,
    pub block_hash: Option<H256>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FilterAddress {
    Single(H160),
    Multiple(Vec<H160>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FilterTopic {
    Single(H256),
    Multiple(Vec<H256>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockNumber {
    Latest,
    Earliest,
    Pending,
    Number(U256),
}

impl Default for BlockNumber {
    fn default() -> Self {
        BlockNumber::Latest
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub starting_block: U256,
    pub current_block: U256,
    pub highest_block: U256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeHistory {
    pub oldest_block: U256,
    pub base_fee_per_gas: Vec<U256>,
    pub gas_used_ratio: Vec<f64>,
    pub reward: Option<Vec<Vec<U256>>>,
}