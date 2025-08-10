use ethereum_types::{Address, Bloom, Bytes, H256, U256, U64};
use ethereum_core::{Block, Header, Transaction};
use ethereum_evm::EvmContext;
use ethereum_txpool::TxPool;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{EngineError, Result};
use crate::types::{
    ExecutionPayloadV1, ExecutionPayloadV2, ExecutionPayloadV3,
    PayloadAttributesV1, PayloadAttributesV2, PayloadAttributesV3,
    PayloadId, Withdrawal,
};

pub struct PayloadAttributes {
    pub timestamp: u64,
    pub prev_randao: H256,
    pub suggested_fee_recipient: Address,
    pub withdrawals: Option<Vec<Withdrawal>>,
    pub parent_beacon_block_root: Option<H256>,
}

impl From<PayloadAttributesV1> for PayloadAttributes {
    fn from(v1: PayloadAttributesV1) -> Self {
        Self {
            timestamp: v1.timestamp.as_u64(),
            prev_randao: v1.prev_randao,
            suggested_fee_recipient: v1.suggested_fee_recipient,
            withdrawals: None,
            parent_beacon_block_root: None,
        }
    }
}

impl From<PayloadAttributesV2> for PayloadAttributes {
    fn from(v2: PayloadAttributesV2) -> Self {
        Self {
            timestamp: v2.timestamp.as_u64(),
            prev_randao: v2.prev_randao,
            suggested_fee_recipient: v2.suggested_fee_recipient,
            withdrawals: Some(v2.withdrawals),
            parent_beacon_block_root: None,
        }
    }
}

impl From<PayloadAttributesV3> for PayloadAttributes {
    fn from(v3: PayloadAttributesV3) -> Self {
        Self {
            timestamp: v3.timestamp.as_u64(),
            prev_randao: v3.prev_randao,
            suggested_fee_recipient: v3.suggested_fee_recipient,
            withdrawals: Some(v3.withdrawals),
            parent_beacon_block_root: Some(v3.parent_beacon_block_root),
        }
    }
}

pub struct PayloadBuilder {
    tx_pool: Arc<TxPool>,
    payloads: Arc<RwLock<HashMap<PayloadId, BuildingPayload>>>,
    chain_id: u64,
}

struct BuildingPayload {
    parent_hash: H256,
    attributes: PayloadAttributes,
    block: Block,
    value: U256,
    started_at: SystemTime,
}

impl PayloadBuilder {
    pub fn new(tx_pool: Arc<TxPool>, chain_id: u64) -> Self {
        Self {
            tx_pool,
            payloads: Arc::new(RwLock::new(HashMap::new())),
            chain_id,
        }
    }

    pub fn build_payload(
        &self,
        parent_hash: H256,
        parent: &Block,
        attributes: PayloadAttributes,
    ) -> Result<PayloadId> {
        let payload_id = PayloadId::new();
        
        let block = self.create_block(parent_hash, parent, &attributes)?;
        
        let building = BuildingPayload {
            parent_hash,
            attributes,
            block,
            value: U256::zero(),
            started_at: SystemTime::now(),
        };
        
        let mut payloads = self.payloads.write().unwrap();
        payloads.insert(payload_id.clone(), building);
        
        let builder = self.clone();
        let id = payload_id.clone();
        tokio::spawn(async move {
            builder.build_async(id).await;
        });
        
        Ok(payload_id)
    }

    async fn build_async(&self, payload_id: PayloadId) {
        loop {
            let should_continue = self.update_payload(&payload_id);
            if !should_continue {
                break;
            }
            
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    fn update_payload(&self, payload_id: &PayloadId) -> bool {
        let mut payloads = self.payloads.write().unwrap();
        
        let building = match payloads.get_mut(payload_id) {
            Some(p) => p,
            None => return false,
        };
        
        let elapsed = SystemTime::now()
            .duration_since(building.started_at)
            .unwrap_or_default();
        
        if elapsed.as_secs() >= 12 {
            return false;
        }
        
        let gas_limit = building.block.header.gas_limit;
        let mut gas_used = building.block.header.gas_used;
        
        let pending_txs = self.tx_pool.get_pending();
        
        for tx in pending_txs {
            if gas_used + tx.gas_limit() > gas_limit {
                continue;
            }
            
            gas_used += tx.gas_limit();
            building.value += tx.max_fee();
            
            let tx_bytes = ethereum_rlp::encode(&tx);
            if let Ok(payload) = self.get_payload_v3(payload_id) {
                
            }
        }
        
        true
    }

    fn create_block(
        &self,
        parent_hash: H256,
        parent: &Block,
        attributes: &PayloadAttributes,
    ) -> Result<Block> {
        let mut header = Header {
            parent_hash,
            uncles_hash: H256::zero(),
            beneficiary: attributes.suggested_fee_recipient,
            state_root: H256::zero(),
            transactions_root: H256::zero(),
            receipts_root: H256::zero(),
            logs_bloom: Bloom::zero(),
            difficulty: U256::zero(),
            number: parent.header.number + 1,
            gas_limit: parent.header.gas_limit,
            gas_used: U256::zero(),
            timestamp: attributes.timestamp,
            extra_data: Bytes::from(b"ethereum-rust".to_vec()),
            mix_hash: attributes.prev_randao,
            nonce: [0u8; 8],
            base_fee_per_gas: Some(self.calculate_base_fee(parent)),
            withdrawals_root: attributes.withdrawals.as_ref().map(|_| H256::zero()),
            blob_gas_used: None,
            excess_blob_gas: None,
            parent_beacon_block_root: attributes.parent_beacon_block_root,
        };
        
        if attributes.withdrawals.is_some() {
            header.blob_gas_used = Some(U64::zero());
            header.excess_blob_gas = Some(U64::zero());
        }
        
        Ok(Block {
            header,
            transactions: Vec::new(),
            uncles: Vec::new(),
            withdrawals: attributes.withdrawals.clone(),
        })
    }

    fn calculate_base_fee(&self, parent: &Block) -> U256 {
        let parent_base_fee = parent.header.base_fee_per_gas.unwrap_or(U256::from(1_000_000_000));
        let parent_gas_used = parent.header.gas_used;
        let parent_gas_target = parent.header.gas_limit / 2;
        
        if parent_gas_used == parent_gas_target {
            return parent_base_fee;
        }
        
        let base_fee_delta = if parent_gas_used > parent_gas_target {
            let gas_used_delta = parent_gas_used - parent_gas_target;
            parent_base_fee * gas_used_delta / parent_gas_target / 8
        } else {
            let gas_used_delta = parent_gas_target - parent_gas_used;
            parent_base_fee * gas_used_delta / parent_gas_target / 8
        };
        
        if parent_gas_used > parent_gas_target {
            parent_base_fee + base_fee_delta
        } else {
            if parent_base_fee > base_fee_delta {
                parent_base_fee - base_fee_delta
            } else {
                U256::from(1)
            }
        }
    }

    pub fn get_payload_v1(&self, payload_id: &PayloadId) -> Result<ExecutionPayloadV1> {
        let payloads = self.payloads.read().unwrap();
        let building = payloads.get(payload_id)
            .ok_or(EngineError::UnknownPayload)?;
        
        Ok(ExecutionPayloadV1 {
            parent_hash: building.block.header.parent_hash,
            fee_recipient: building.block.header.beneficiary,
            state_root: building.block.header.state_root,
            receipts_root: building.block.header.receipts_root,
            logs_bloom: building.block.header.logs_bloom,
            prev_randao: building.block.header.mix_hash,
            block_number: U64::from(building.block.header.number),
            gas_limit: U64::from(building.block.header.gas_limit.as_u64()),
            gas_used: U64::from(building.block.header.gas_used.as_u64()),
            timestamp: U64::from(building.block.header.timestamp),
            extra_data: building.block.header.extra_data.clone(),
            base_fee_per_gas: building.block.header.base_fee_per_gas.unwrap_or(U256::zero()),
            block_hash: building.block.hash(),
            transactions: building.block.transactions.iter()
                .map(|tx| Bytes::from(ethereum_rlp::encode(tx)))
                .collect(),
        })
    }

    pub fn get_payload_v2(&self, payload_id: &PayloadId) -> Result<ExecutionPayloadV2> {
        let payloads = self.payloads.read().unwrap();
        let building = payloads.get(payload_id)
            .ok_or(EngineError::UnknownPayload)?;
        
        let v1 = self.get_payload_v1(payload_id)?;
        
        Ok(ExecutionPayloadV2 {
            parent_hash: v1.parent_hash,
            fee_recipient: v1.fee_recipient,
            state_root: v1.state_root,
            receipts_root: v1.receipts_root,
            logs_bloom: v1.logs_bloom,
            prev_randao: v1.prev_randao,
            block_number: v1.block_number,
            gas_limit: v1.gas_limit,
            gas_used: v1.gas_used,
            timestamp: v1.timestamp,
            extra_data: v1.extra_data,
            base_fee_per_gas: v1.base_fee_per_gas,
            block_hash: v1.block_hash,
            transactions: v1.transactions,
            withdrawals: building.block.withdrawals.clone().unwrap_or_default(),
        })
    }

    pub fn get_payload_v3(&self, payload_id: &PayloadId) -> Result<ExecutionPayloadV3> {
        let payloads = self.payloads.read().unwrap();
        let building = payloads.get(payload_id)
            .ok_or(EngineError::UnknownPayload)?;
        
        let v2 = self.get_payload_v2(payload_id)?;
        
        Ok(ExecutionPayloadV3 {
            parent_hash: v2.parent_hash,
            fee_recipient: v2.fee_recipient,
            state_root: v2.state_root,
            receipts_root: v2.receipts_root,
            logs_bloom: v2.logs_bloom,
            prev_randao: v2.prev_randao,
            block_number: v2.block_number,
            gas_limit: v2.gas_limit,
            gas_used: v2.gas_used,
            timestamp: v2.timestamp,
            extra_data: v2.extra_data,
            base_fee_per_gas: v2.base_fee_per_gas,
            block_hash: v2.block_hash,
            transactions: v2.transactions,
            withdrawals: v2.withdrawals,
            blob_gas_used: building.block.header.blob_gas_used.unwrap_or(U64::zero()),
            excess_blob_gas: building.block.header.excess_blob_gas.unwrap_or(U64::zero()),
        })
    }

    pub fn remove_payload(&self, payload_id: &PayloadId) {
        let mut payloads = self.payloads.write().unwrap();
        payloads.remove(payload_id);
    }
}