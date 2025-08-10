use async_trait::async_trait;
use ethereum_types::{Address, H256, U256};
use ethereum_engine::types::{ExecutionPayloadV3, ExecutionPayloadHeader};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn, error};

use crate::{MevError, Result};

/// MEV-Boost relay interface
#[async_trait]
pub trait Relay: Send + Sync {
    async fn register_validator(&self, registration: ValidatorRegistration) -> Result<()>;
    async fn get_header(&self, slot: u64, parent_hash: H256, pubkey: Vec<u8>) -> Result<Option<SignedBuilderBid>>;
    async fn get_payload(&self, signed_block: SignedBlindedBeaconBlock) -> Result<ExecutionPayloadV3>;
    async fn get_status(&self) -> Result<RelayStatus>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayInfo {
    pub url: String,
    pub pubkey: Vec<u8>,
    pub network: String,
    pub min_bid: U256,
    pub submission_rate_limit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayStatus {
    pub healthy: bool,
    pub latency_ms: u64,
    pub last_successful_slot: u64,
    pub error_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorRegistration {
    pub fee_recipient: Address,
    pub gas_limit: u64,
    pub timestamp: u64,
    pub pubkey: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedBuilderBid {
    pub message: BuilderBid,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderBid {
    pub header: ExecutionPayloadHeader,
    pub value: U256,
    pub pubkey: Vec<u8>,
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
    pub proposer_slashings: Vec<serde_json::Value>,
    pub attester_slashings: Vec<serde_json::Value>,
    pub attestations: Vec<serde_json::Value>,
    pub deposits: Vec<serde_json::Value>,
    pub voluntary_exits: Vec<serde_json::Value>,
    pub sync_aggregate: SyncAggregate,
    pub execution_payload_header: ExecutionPayloadHeader,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eth1Data {
    pub deposit_root: H256,
    pub deposit_count: u64,
    pub block_hash: H256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAggregate {
    pub sync_committee_bits: Vec<u8>,
    pub sync_committee_signature: Vec<u8>,
}

/// Standard MEV-Boost relay client
pub struct RelayClient {
    client: Client,
    info: RelayInfo,
    metrics: Arc<RelayMetrics>,
}

impl RelayClient {
    pub fn new(info: RelayInfo) -> Self {
        Self {
            client: Client::new(),
            info,
            metrics: Arc::new(RelayMetrics::default()),
        }
    }
    
    async fn post<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        body: &T,
    ) -> Result<R> {
        let url = format!("{}{}", self.info.url, endpoint);
        
        let response = self.client
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| MevError::NetworkError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(MevError::RelayError(format!(
                "Request failed with status: {}",
                response.status()
            )));
        }
        
        response.json::<R>()
            .await
            .map_err(|e| MevError::RelayError(e.to_string()))
    }
    
    async fn get<R: for<'de> Deserialize<'de>>(&self, endpoint: &str) -> Result<R> {
        let url = format!("{}{}", self.info.url, endpoint);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| MevError::NetworkError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(MevError::RelayError(format!(
                "Request failed with status: {}",
                response.status()
            )));
        }
        
        response.json::<R>()
            .await
            .map_err(|e| MevError::RelayError(e.to_string()))
    }
}

#[async_trait]
impl Relay for RelayClient {
    async fn register_validator(&self, registration: ValidatorRegistration) -> Result<()> {
        let endpoint = "/eth/v1/builder/validators";
        let registrations = vec![registration];
        
        self.post::<_, serde_json::Value>(endpoint, &registrations).await?;
        self.metrics.record_registration();
        
        info!("Validator registered successfully");
        Ok(())
    }
    
    async fn get_header(
        &self,
        slot: u64,
        parent_hash: H256,
        pubkey: Vec<u8>,
    ) -> Result<Option<SignedBuilderBid>> {
        let endpoint = format!(
            "/eth/v1/builder/header/{}/{}/{}",
            slot,
            hex::encode(parent_hash.as_bytes()),
            hex::encode(&pubkey)
        );
        
        match self.get::<SignedBuilderBid>(&endpoint).await {
            Ok(bid) => {
                self.metrics.record_header_request(true);
                Ok(Some(bid))
            }
            Err(MevError::RelayError(e)) if e.contains("404") => {
                self.metrics.record_header_request(false);
                Ok(None)
            }
            Err(e) => {
                self.metrics.record_header_request(false);
                Err(e)
            }
        }
    }
    
    async fn get_payload(&self, signed_block: SignedBlindedBeaconBlock) -> Result<ExecutionPayloadV3> {
        let endpoint = "/eth/v1/builder/blinded_blocks";
        
        let payload = self.post::<_, ExecutionPayloadV3>(endpoint, &signed_block).await?;
        self.metrics.record_payload_request();
        
        Ok(payload)
    }
    
    async fn get_status(&self) -> Result<RelayStatus> {
        let endpoint = "/eth/v1/builder/status";
        
        let start = std::time::Instant::now();
        let _: serde_json::Value = self.get(endpoint).await?;
        let latency_ms = start.elapsed().as_millis() as u64;
        
        Ok(RelayStatus {
            healthy: true,
            latency_ms,
            last_successful_slot: self.metrics.last_successful_slot(),
            error_rate: self.metrics.error_rate(),
        })
    }
}

/// Multi-relay aggregator
pub struct MultiRelay {
    relays: Vec<Arc<dyn Relay>>,
}

impl MultiRelay {
    pub fn new(relays: Vec<Arc<dyn Relay>>) -> Self {
        Self { relays }
    }
    
    pub async fn get_best_header(
        &self,
        slot: u64,
        parent_hash: H256,
        pubkey: Vec<u8>,
    ) -> Result<Option<SignedBuilderBid>> {
        let mut best_bid: Option<SignedBuilderBid> = None;
        let mut best_value = U256::zero();
        
        for relay in &self.relays {
            match relay.get_header(slot, parent_hash, pubkey.clone()).await {
                Ok(Some(bid)) if bid.message.value > best_value => {
                    best_value = bid.message.value;
                    best_bid = Some(bid);
                }
                Ok(_) => {}
                Err(e) => warn!("Relay error: {:?}", e),
            }
        }
        
        Ok(best_bid)
    }
    
    pub async fn register_validator_all(&self, registration: ValidatorRegistration) -> Vec<Result<()>> {
        let mut results = Vec::new();
        
        for relay in &self.relays {
            results.push(relay.register_validator(registration.clone()).await);
        }
        
        results
    }
}

#[derive(Default)]
struct RelayMetrics {
    registrations: std::sync::atomic::AtomicU64,
    header_requests: std::sync::atomic::AtomicU64,
    header_hits: std::sync::atomic::AtomicU64,
    payload_requests: std::sync::atomic::AtomicU64,
    errors: std::sync::atomic::AtomicU64,
    last_slot: std::sync::atomic::AtomicU64,
}

impl RelayMetrics {
    fn record_registration(&self) {
        self.registrations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    
    fn record_header_request(&self, hit: bool) {
        self.header_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if hit {
            self.header_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    
    fn record_payload_request(&self) {
        self.payload_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    
    fn last_successful_slot(&self) -> u64 {
        self.last_slot.load(std::sync::atomic::Ordering::Relaxed)
    }
    
    fn error_rate(&self) -> f64 {
        let total = self.header_requests.load(std::sync::atomic::Ordering::Relaxed)
            + self.payload_requests.load(std::sync::atomic::Ordering::Relaxed);
        let errors = self.errors.load(std::sync::atomic::Ordering::Relaxed);
        
        if total == 0 {
            0.0
        } else {
            errors as f64 / total as f64
        }
    }
}