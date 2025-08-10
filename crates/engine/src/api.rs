use async_trait::async_trait;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use ethereum_types::{H256, U256};
use ethereum_core::Block;
use ethereum_storage::Storage;
use ethereum_consensus::ConsensusEngine;
use ethereum_txpool::TxPool;
use std::sync::Arc;
use tracing::{info, warn, error};

use crate::{EngineError, Result};
use crate::auth::{JwtAuth, JwtSecret};
use crate::forkchoice::ForkChoiceStore;
use crate::payload::{PayloadBuilder, PayloadAttributes};
use crate::types::*;

#[rpc(server, namespace = "engine")]
pub trait EngineApi {
    #[method(name = "newPayloadV1")]
    async fn new_payload_v1(&self, payload: ExecutionPayloadV1) -> RpcResult<PayloadStatusV1>;

    #[method(name = "newPayloadV2")]
    async fn new_payload_v2(&self, payload: ExecutionPayloadV2) -> RpcResult<PayloadStatusV1>;

    #[method(name = "newPayloadV3")]
    async fn new_payload_v3(
        &self,
        payload: ExecutionPayloadV3,
        versioned_hashes: Vec<BlobVersionedHash>,
        parent_beacon_block_root: H256,
    ) -> RpcResult<PayloadStatusV1>;

    #[method(name = "forkchoiceUpdatedV1")]
    async fn forkchoice_updated_v1(
        &self,
        forkchoice_state: ForkchoiceStateV1,
        payload_attributes: Option<PayloadAttributesV1>,
    ) -> RpcResult<ForkchoiceUpdatedResponseV1>;

    #[method(name = "forkchoiceUpdatedV2")]
    async fn forkchoice_updated_v2(
        &self,
        forkchoice_state: ForkchoiceStateV1,
        payload_attributes: Option<PayloadAttributesV2>,
    ) -> RpcResult<ForkchoiceUpdatedResponseV1>;

    #[method(name = "forkchoiceUpdatedV3")]
    async fn forkchoice_updated_v3(
        &self,
        forkchoice_state: ForkchoiceStateV1,
        payload_attributes: Option<PayloadAttributesV3>,
    ) -> RpcResult<ForkchoiceUpdatedResponseV1>;

    #[method(name = "getPayloadV1")]
    async fn get_payload_v1(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV1>;

    #[method(name = "getPayloadV2")]
    async fn get_payload_v2(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV2>;

    #[method(name = "getPayloadV3")]
    async fn get_payload_v3(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV3>;

    #[method(name = "getPayloadBodiesByHashV1")]
    async fn get_payload_bodies_by_hash_v1(
        &self,
        block_hashes: Vec<H256>,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBody>>>;

    #[method(name = "getPayloadBodiesByRangeV1")]
    async fn get_payload_bodies_by_range_v1(
        &self,
        start: u64,
        count: u64,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBody>>>;

    #[method(name = "exchangeTransitionConfigurationV1")]
    async fn exchange_transition_configuration_v1(
        &self,
        config: TransitionConfiguration,
    ) -> RpcResult<TransitionConfiguration>;

    #[method(name = "getClientVersionV1")]
    async fn get_client_version_v1(&self) -> RpcResult<Vec<ClientVersionV1>>;

    #[method(name = "exchangeCapabilities")]
    async fn exchange_capabilities(&self, methods: Vec<String>) -> RpcResult<Vec<String>>;
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPayloadBody {
    pub transactions: Vec<ethereum_types::Bytes>,
    pub withdrawals: Option<Vec<Withdrawal>>,
}

pub struct EngineApiServer {
    storage: Arc<dyn Storage>,
    consensus: Arc<dyn ConsensusEngine>,
    tx_pool: Arc<TxPool>,
    jwt_auth: Arc<JwtAuth>,
    forkchoice: Arc<ForkChoiceStore>,
    payload_builder: Arc<PayloadBuilder>,
    chain_id: u64,
}

impl EngineApiServer {
    pub fn new(
        storage: Arc<dyn Storage>,
        consensus: Arc<dyn ConsensusEngine>,
        tx_pool: Arc<TxPool>,
        jwt_secret: JwtSecret,
        chain_id: u64,
    ) -> Self {
        Self {
            storage,
            consensus,
            tx_pool: tx_pool.clone(),
            jwt_auth: Arc::new(JwtAuth::new(jwt_secret)),
            forkchoice: Arc::new(ForkChoiceStore::new()),
            payload_builder: Arc::new(PayloadBuilder::new(tx_pool, chain_id)),
            chain_id,
        }
    }

    async fn validate_and_import_payload(&self, block: Block) -> Result<PayloadStatusV1> {
        match self.consensus.validate_block(&block) {
            Ok(_) => {
                self.storage.insert_block(block.clone())
                    .map_err(|e| EngineError::Internal(format!("Failed to store block: {:?}", e)))?;
                
                self.forkchoice.add_block(
                    block.hash(),
                    block.header.parent_hash,
                    block.header.number,
                    block.header.difficulty,
                );
                
                self.forkchoice.validate_block(&block.hash())?;
                
                info!("Imported new payload: {:?}", block.hash());
                
                Ok(PayloadStatusV1 {
                    status: PayloadStatus::Valid,
                    latest_valid_hash: Some(block.hash()),
                    validation_error: None,
                })
            }
            Err(e) => {
                warn!("Invalid payload: {:?}", e);
                
                Ok(PayloadStatusV1 {
                    status: PayloadStatus::Invalid,
                    latest_valid_hash: None,
                    validation_error: Some(format!("{:?}", e)),
                })
            }
        }
    }

    fn payload_to_block(&self, payload: ExecutionPayloadV1) -> Block {
        use ethereum_core::Header;
        
        let header = Header {
            parent_hash: payload.parent_hash,
            uncles_hash: H256::zero(),
            beneficiary: payload.fee_recipient,
            state_root: payload.state_root,
            transactions_root: H256::zero(),
            receipts_root: payload.receipts_root,
            logs_bloom: payload.logs_bloom,
            difficulty: U256::zero(),
            number: payload.block_number.as_u64(),
            gas_limit: payload.gas_limit.as_u256(),
            gas_used: payload.gas_used.as_u256(),
            timestamp: payload.timestamp.as_u64(),
            extra_data: payload.extra_data,
            mix_hash: payload.prev_randao,
            nonce: [0u8; 8],
            base_fee_per_gas: Some(payload.base_fee_per_gas),
            withdrawals_root: None,
            blob_gas_used: None,
            excess_blob_gas: None,
            parent_beacon_block_root: None,
        };

        let transactions = payload.transactions
            .iter()
            .filter_map(|bytes| {
                ethereum_rlp::decode::<ethereum_core::Transaction>(bytes.as_ref()).ok()
            })
            .collect();

        Block {
            header,
            transactions,
            uncles: Vec::new(),
            withdrawals: None,
        }
    }

    fn payload_v2_to_block(&self, payload: ExecutionPayloadV2) -> Block {
        let mut block = self.payload_to_block(ExecutionPayloadV1 {
            parent_hash: payload.parent_hash,
            fee_recipient: payload.fee_recipient,
            state_root: payload.state_root,
            receipts_root: payload.receipts_root,
            logs_bloom: payload.logs_bloom,
            prev_randao: payload.prev_randao,
            block_number: payload.block_number,
            gas_limit: payload.gas_limit,
            gas_used: payload.gas_used,
            timestamp: payload.timestamp,
            extra_data: payload.extra_data,
            base_fee_per_gas: payload.base_fee_per_gas,
            block_hash: payload.block_hash,
            transactions: payload.transactions,
        });

        block.withdrawals = Some(payload.withdrawals);
        block.header.withdrawals_root = Some(H256::zero());
        
        block
    }

    fn payload_v3_to_block(&self, payload: ExecutionPayloadV3) -> Block {
        let mut block = self.payload_v2_to_block(ExecutionPayloadV2 {
            parent_hash: payload.parent_hash,
            fee_recipient: payload.fee_recipient,
            state_root: payload.state_root,
            receipts_root: payload.receipts_root,
            logs_bloom: payload.logs_bloom,
            prev_randao: payload.prev_randao,
            block_number: payload.block_number,
            gas_limit: payload.gas_limit,
            gas_used: payload.gas_used,
            timestamp: payload.timestamp,
            extra_data: payload.extra_data,
            base_fee_per_gas: payload.base_fee_per_gas,
            block_hash: payload.block_hash,
            transactions: payload.transactions,
            withdrawals: payload.withdrawals,
        });

        block.header.blob_gas_used = Some(payload.blob_gas_used);
        block.header.excess_blob_gas = Some(payload.excess_blob_gas);
        
        block
    }
}

#[async_trait]
impl EngineApi for EngineApiServer {
    async fn new_payload_v1(&self, payload: ExecutionPayloadV1) -> RpcResult<PayloadStatusV1> {
        let block = self.payload_to_block(payload);
        Ok(self.validate_and_import_payload(block).await?)
    }

    async fn new_payload_v2(&self, payload: ExecutionPayloadV2) -> RpcResult<PayloadStatusV1> {
        let block = self.payload_v2_to_block(payload);
        Ok(self.validate_and_import_payload(block).await?)
    }

    async fn new_payload_v3(
        &self,
        payload: ExecutionPayloadV3,
        versioned_hashes: Vec<BlobVersionedHash>,
        parent_beacon_block_root: H256,
    ) -> RpcResult<PayloadStatusV1> {
        let mut block = self.payload_v3_to_block(payload);
        block.header.parent_beacon_block_root = Some(parent_beacon_block_root);
        
        Ok(self.validate_and_import_payload(block).await?)
    }

    async fn forkchoice_updated_v1(
        &self,
        forkchoice_state: ForkchoiceStateV1,
        payload_attributes: Option<PayloadAttributesV1>,
    ) -> RpcResult<ForkchoiceUpdatedResponseV1> {
        let status = self.forkchoice.update_forkchoice(forkchoice_state.clone())?;
        
        let payload_id = if let Some(attributes) = payload_attributes {
            let parent = self.storage
                .get_block_by_hash(forkchoice_state.head_block_hash)
                .map_err(|_| EngineError::InvalidForkChoiceState("Parent block not found".to_string()))?
                .ok_or(EngineError::InvalidForkChoiceState("Parent block not found".to_string()))?;
            
            Some(self.payload_builder.build_payload(
                forkchoice_state.head_block_hash,
                &parent,
                attributes.into(),
            )?)
        } else {
            None
        };
        
        Ok(ForkchoiceUpdatedResponseV1 {
            payload_status: status,
            payload_id,
        })
    }

    async fn forkchoice_updated_v2(
        &self,
        forkchoice_state: ForkchoiceStateV1,
        payload_attributes: Option<PayloadAttributesV2>,
    ) -> RpcResult<ForkchoiceUpdatedResponseV1> {
        let status = self.forkchoice.update_forkchoice(forkchoice_state.clone())?;
        
        let payload_id = if let Some(attributes) = payload_attributes {
            let parent = self.storage
                .get_block_by_hash(forkchoice_state.head_block_hash)
                .map_err(|_| EngineError::InvalidForkChoiceState("Parent block not found".to_string()))?
                .ok_or(EngineError::InvalidForkChoiceState("Parent block not found".to_string()))?;
            
            Some(self.payload_builder.build_payload(
                forkchoice_state.head_block_hash,
                &parent,
                attributes.into(),
            )?)
        } else {
            None
        };
        
        Ok(ForkchoiceUpdatedResponseV1 {
            payload_status: status,
            payload_id,
        })
    }

    async fn forkchoice_updated_v3(
        &self,
        forkchoice_state: ForkchoiceStateV1,
        payload_attributes: Option<PayloadAttributesV3>,
    ) -> RpcResult<ForkchoiceUpdatedResponseV1> {
        let status = self.forkchoice.update_forkchoice(forkchoice_state.clone())?;
        
        let payload_id = if let Some(attributes) = payload_attributes {
            let parent = self.storage
                .get_block_by_hash(forkchoice_state.head_block_hash)
                .map_err(|_| EngineError::InvalidForkChoiceState("Parent block not found".to_string()))?
                .ok_or(EngineError::InvalidForkChoiceState("Parent block not found".to_string()))?;
            
            Some(self.payload_builder.build_payload(
                forkchoice_state.head_block_hash,
                &parent,
                attributes.into(),
            )?)
        } else {
            None
        };
        
        Ok(ForkchoiceUpdatedResponseV1 {
            payload_status: status,
            payload_id,
        })
    }

    async fn get_payload_v1(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV1> {
        Ok(self.payload_builder.get_payload_v1(&payload_id)?)
    }

    async fn get_payload_v2(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV2> {
        Ok(self.payload_builder.get_payload_v2(&payload_id)?)
    }

    async fn get_payload_v3(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV3> {
        Ok(self.payload_builder.get_payload_v3(&payload_id)?)
    }

    async fn get_payload_bodies_by_hash_v1(
        &self,
        block_hashes: Vec<H256>,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBody>>> {
        let mut bodies = Vec::new();
        
        for hash in block_hashes {
            let body = match self.storage.get_block_by_hash(hash) {
                Ok(Some(block)) => Some(ExecutionPayloadBody {
                    transactions: block.transactions
                        .iter()
                        .map(|tx| ethereum_types::Bytes::from(ethereum_rlp::encode(tx)))
                        .collect(),
                    withdrawals: block.withdrawals,
                }),
                _ => None,
            };
            
            bodies.push(body);
        }
        
        Ok(bodies)
    }

    async fn get_payload_bodies_by_range_v1(
        &self,
        start: u64,
        count: u64,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBody>>> {
        let mut bodies = Vec::new();
        
        for number in start..start + count {
            let body = match self.storage.get_block_by_number(number) {
                Ok(Some(block)) => Some(ExecutionPayloadBody {
                    transactions: block.transactions
                        .iter()
                        .map(|tx| ethereum_types::Bytes::from(ethereum_rlp::encode(tx)))
                        .collect(),
                    withdrawals: block.withdrawals,
                }),
                _ => None,
            };
            
            bodies.push(body);
        }
        
        Ok(bodies)
    }

    async fn exchange_transition_configuration_v1(
        &self,
        config: TransitionConfiguration,
    ) -> RpcResult<TransitionConfiguration> {
        Ok(config)
    }

    async fn get_client_version_v1(&self) -> RpcResult<Vec<ClientVersionV1>> {
        Ok(vec![ClientVersionV1 {
            code: "ER".to_string(),
            name: "ethereum-rust".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            commit: "unknown".to_string(),
        }])
    }

    async fn exchange_capabilities(&self, methods: Vec<String>) -> RpcResult<Vec<String>> {
        let supported = vec![
            "engine_newPayloadV1",
            "engine_newPayloadV2",
            "engine_newPayloadV3",
            "engine_forkchoiceUpdatedV1",
            "engine_forkchoiceUpdatedV2",
            "engine_forkchoiceUpdatedV3",
            "engine_getPayloadV1",
            "engine_getPayloadV2",
            "engine_getPayloadV3",
            "engine_getPayloadBodiesByHashV1",
            "engine_getPayloadBodiesByRangeV1",
            "engine_exchangeTransitionConfigurationV1",
            "engine_getClientVersionV1",
            "engine_exchangeCapabilities",
        ];
        
        let result: Vec<String> = methods
            .iter()
            .filter(|m| supported.contains(&m.as_str()))
            .cloned()
            .collect();
        
        Ok(result)
    }
}