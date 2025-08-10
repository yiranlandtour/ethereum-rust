use ethereum_types::{Address, H256, U256};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use reqwest::Client;

use crate::{EngineError, Result};
use crate::types::{ExecutionPayloadV3, BlobVersionedHash};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderBid {
    pub header: ExecutionPayloadHeader,
    pub value: U256,
    pub pubkey: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPayloadHeader {
    pub parent_hash: H256,
    pub fee_recipient: Address,
    pub state_root: H256,
    pub receipts_root: H256,
    pub logs_bloom: Vec<u8>,
    pub prev_randao: H256,
    pub block_number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub extra_data: Vec<u8>,
    pub base_fee_per_gas: U256,
    pub block_hash: H256,
    pub transactions_root: H256,
    pub withdrawals_root: Option<H256>,
    pub blob_gas_used: Option<u64>,
    pub excess_blob_gas: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedBlindedBeaconBlock {
    pub message: BlindedBeaconBlock,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlindedBeaconBlock {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: H256,
    pub state_root: H256,
    pub body: BlindedBeaconBlockBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlindedBeaconBlockBody {
    pub randao_reveal: Vec<u8>,
    pub eth1_data: Eth1Data,
    pub graffiti: H256,
    pub proposer_slashings: Vec<ProposerSlashing>,
    pub attester_slashings: Vec<AttesterSlashing>,
    pub attestations: Vec<Attestation>,
    pub deposits: Vec<Deposit>,
    pub voluntary_exits: Vec<VoluntaryExit>,
    pub sync_aggregate: SyncAggregate,
    pub execution_payload_header: ExecutionPayloadHeader,
    pub bls_to_execution_changes: Vec<BlsToExecutionChange>,
    pub blob_kzg_commitments: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eth1Data {
    pub deposit_root: H256,
    pub deposit_count: u64,
    pub block_hash: H256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposerSlashing;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttesterSlashing;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deposit;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoluntaryExit;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAggregate {
    pub sync_committee_bits: Vec<u8>,
    pub sync_committee_signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlsToExecutionChange;

pub struct MevBoostClient {
    client: Client,
    relay_urls: Vec<String>,
}

impl MevBoostClient {
    pub fn new(relay_urls: Vec<String>) -> Self {
        Self {
            client: Client::new(),
            relay_urls,
        }
    }

    pub async fn register_validator(&self, registrations: Vec<ValidatorRegistration>) -> Result<()> {
        for relay_url in &self.relay_urls {
            let url = format!("{}/eth/v1/builder/validators", relay_url);
            
            let response = self.client
                .post(&url)
                .json(&registrations)
                .send()
                .await
                .map_err(|e| EngineError::Internal(format!("Failed to register validator: {}", e)))?;
            
            if !response.status().is_success() {
                return Err(EngineError::Internal(format!(
                    "Validator registration failed: {}",
                    response.status()
                )));
            }
        }
        
        Ok(())
    }

    pub async fn get_header(
        &self,
        slot: u64,
        parent_hash: H256,
        pubkey: Vec<u8>,
    ) -> Result<Option<BuilderBid>> {
        for relay_url in &self.relay_urls {
            let url = format!(
                "{}/eth/v1/builder/header/{}/{}/{}",
                relay_url,
                slot,
                hex::encode(parent_hash.as_bytes()),
                hex::encode(&pubkey)
            );
            
            let response = self.client
                .get(&url)
                .send()
                .await
                .map_err(|e| EngineError::Internal(format!("Failed to get header: {}", e)))?;
            
            if response.status().is_success() {
                let bid: BuilderBid = response
                    .json()
                    .await
                    .map_err(|e| EngineError::Internal(format!("Failed to parse bid: {}", e)))?;
                
                return Ok(Some(bid));
            }
        }
        
        Ok(None)
    }

    pub async fn get_payload(
        &self,
        signed_block: SignedBlindedBeaconBlock,
    ) -> Result<ExecutionPayloadV3> {
        for relay_url in &self.relay_urls {
            let url = format!("{}/eth/v1/builder/blinded_blocks", relay_url);
            
            let response = self.client
                .post(&url)
                .json(&signed_block)
                .send()
                .await
                .map_err(|e| EngineError::Internal(format!("Failed to get payload: {}", e)))?;
            
            if response.status().is_success() {
                let payload: ExecutionPayloadV3 = response
                    .json()
                    .await
                    .map_err(|e| EngineError::Internal(format!("Failed to parse payload: {}", e)))?;
                
                return Ok(payload);
            }
        }
        
        Err(EngineError::Internal("Failed to get payload from any relay".to_string()))
    }

    pub async fn get_status(&self) -> Result<Vec<RelayStatus>> {
        let mut statuses = Vec::new();
        
        for relay_url in &self.relay_urls {
            let url = format!("{}/eth/v1/builder/status", relay_url);
            
            let status = match self.client.get(&url).send().await {
                Ok(response) if response.status().is_success() => RelayStatus::Online,
                _ => RelayStatus::Offline,
            };
            
            statuses.push(status);
        }
        
        Ok(statuses)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorRegistration {
    pub fee_recipient: Address,
    pub gas_limit: u64,
    pub timestamp: u64,
    pub pubkey: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayStatus {
    Online,
    Offline,
}

pub struct LocalBuilder {
    fee_recipient: Address,
    extra_data: Vec<u8>,
}

impl LocalBuilder {
    pub fn new(fee_recipient: Address) -> Self {
        Self {
            fee_recipient,
            extra_data: b"ethereum-rust-builder".to_vec(),
        }
    }

    pub fn build_bid(
        &self,
        payload: &ExecutionPayloadV3,
        value: U256,
    ) -> BuilderBid {
        let header = ExecutionPayloadHeader {
            parent_hash: payload.parent_hash,
            fee_recipient: self.fee_recipient,
            state_root: payload.state_root,
            receipts_root: payload.receipts_root,
            logs_bloom: payload.logs_bloom.as_bytes().to_vec(),
            prev_randao: payload.prev_randao,
            block_number: payload.block_number.as_u64(),
            gas_limit: payload.gas_limit.as_u64(),
            gas_used: payload.gas_used.as_u64(),
            timestamp: payload.timestamp.as_u64(),
            extra_data: self.extra_data.clone(),
            base_fee_per_gas: payload.base_fee_per_gas,
            block_hash: payload.block_hash,
            transactions_root: self.calculate_transactions_root(&payload.transactions),
            withdrawals_root: Some(self.calculate_withdrawals_root(&payload.withdrawals)),
            blob_gas_used: Some(payload.blob_gas_used.as_u64()),
            excess_blob_gas: Some(payload.excess_blob_gas.as_u64()),
        };

        BuilderBid {
            header,
            value,
            pubkey: vec![0u8; 48],
            signature: vec![0u8; 96],
        }
    }

    fn calculate_transactions_root(&self, transactions: &[ethereum_types::Bytes]) -> H256 {
        H256::zero()
    }

    fn calculate_withdrawals_root(&self, withdrawals: &[crate::types::Withdrawal]) -> H256 {
        H256::zero()
    }
}