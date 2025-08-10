use ethereum_types::{Address, Bloom, Bytes, H256, U256, U64};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPayloadV1 {
    pub parent_hash: H256,
    pub fee_recipient: Address,
    pub state_root: H256,
    pub receipts_root: H256,
    pub logs_bloom: Bloom,
    pub prev_randao: H256,
    pub block_number: U64,
    pub gas_limit: U64,
    pub gas_used: U64,
    pub timestamp: U64,
    pub extra_data: Bytes,
    pub base_fee_per_gas: U256,
    pub block_hash: H256,
    pub transactions: Vec<Bytes>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPayloadV2 {
    pub parent_hash: H256,
    pub fee_recipient: Address,
    pub state_root: H256,
    pub receipts_root: H256,
    pub logs_bloom: Bloom,
    pub prev_randao: H256,
    pub block_number: U64,
    pub gas_limit: U64,
    pub gas_used: U64,
    pub timestamp: U64,
    pub extra_data: Bytes,
    pub base_fee_per_gas: U256,
    pub block_hash: H256,
    pub transactions: Vec<Bytes>,
    pub withdrawals: Vec<Withdrawal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPayloadV3 {
    pub parent_hash: H256,
    pub fee_recipient: Address,
    pub state_root: H256,
    pub receipts_root: H256,
    pub logs_bloom: Bloom,
    pub prev_randao: H256,
    pub block_number: U64,
    pub gas_limit: U64,
    pub gas_used: U64,
    pub timestamp: U64,
    pub extra_data: Bytes,
    pub base_fee_per_gas: U256,
    pub block_hash: H256,
    pub transactions: Vec<Bytes>,
    pub withdrawals: Vec<Withdrawal>,
    pub blob_gas_used: U64,
    pub excess_blob_gas: U64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Withdrawal {
    pub index: U64,
    pub validator_index: U64,
    pub address: Address,
    pub amount: U64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadAttributesV1 {
    pub timestamp: U64,
    pub prev_randao: H256,
    pub suggested_fee_recipient: Address,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadAttributesV2 {
    pub timestamp: U64,
    pub prev_randao: H256,
    pub suggested_fee_recipient: Address,
    pub withdrawals: Vec<Withdrawal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadAttributesV3 {
    pub timestamp: U64,
    pub prev_randao: H256,
    pub suggested_fee_recipient: Address,
    pub withdrawals: Vec<Withdrawal>,
    pub parent_beacon_block_root: H256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PayloadStatus {
    Valid,
    Invalid,
    Syncing,
    Accepted,
    InvalidBlockHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadStatusV1 {
    pub status: PayloadStatus,
    pub latest_valid_hash: Option<H256>,
    pub validation_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkchoiceStateV1 {
    pub head_block_hash: H256,
    pub safe_block_hash: H256,
    pub finalized_block_hash: H256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkchoiceUpdatedResponseV1 {
    pub payload_status: PayloadStatusV1,
    pub payload_id: Option<PayloadId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayloadId(pub [u8; 8]);

impl PayloadId {
    pub fn new() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut id = [0u8; 8];
        rng.fill(&mut id);
        Self(id)
    }

    pub fn from_bytes(bytes: [u8; 8]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }
}

impl std::fmt::Display for PayloadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionConfiguration {
    pub terminal_total_difficulty: U256,
    pub terminal_block_hash: H256,
    pub terminal_block_number: U64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobVersionedHash(pub H256);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientVersionV1 {
    pub code: String,
    pub name: String,
    pub version: String,
    pub commit: String,
}