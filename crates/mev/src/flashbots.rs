use ethereum_types::{Address, H256, U256};
use ethereum_core::Transaction;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::{Bundle, BundleTransaction, MevError, Result};

/// Flashbots client for interacting with Flashbots relay
pub struct FlashbotsClient {
    client: Client,
    relay_url: String,
    auth_key: Vec<u8>,
}

impl FlashbotsClient {
    pub fn new(relay_url: String, auth_key: Vec<u8>) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
            relay_url,
            auth_key,
        }
    }
    
    pub fn mainnet() -> Self {
        Self::new(
            "https://relay.flashbots.net".to_string(),
            vec![],
        )
    }
    
    pub fn goerli() -> Self {
        Self::new(
            "https://relay-goerli.flashbots.net".to_string(),
            vec![],
        )
    }
    
    /// Send a bundle to Flashbots
    pub async fn send_bundle(&self, bundle: FlashbotsBundle) -> Result<SendBundleResponse> {
        let request = SendBundleRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "eth_sendBundle".to_string(),
            params: vec![bundle],
        };
        
        let response = self.client
            .post(&self.relay_url)
            .header("X-Flashbots-Signature", self.sign_request(&request)?)
            .json(&request)
            .send()
            .await
            .map_err(|e| MevError::NetworkError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(MevError::RelayError(format!(
                "Request failed with status: {}",
                response.status()
            )));
        }
        
        response.json::<SendBundleResponse>()
            .await
            .map_err(|e| MevError::RelayError(e.to_string()))
    }
    
    /// Simulate a bundle
    pub async fn simulate_bundle(
        &self,
        bundle: FlashbotsBundle,
        block_number: u64,
    ) -> Result<SimulateResponse> {
        let request = SimulateBundleRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "eth_callBundle".to_string(),
            params: vec![bundle, block_number],
        };
        
        let response = self.client
            .post(&self.relay_url)
            .header("X-Flashbots-Signature", self.sign_request(&request)?)
            .json(&request)
            .send()
            .await
            .map_err(|e| MevError::NetworkError(e.to_string()))?;
        
        response.json::<SimulateResponse>()
            .await
            .map_err(|e| MevError::RelayError(e.to_string()))
    }
    
    /// Get bundle stats
    pub async fn get_bundle_stats(
        &self,
        bundle_hash: H256,
        block_number: u64,
    ) -> Result<BundleStats> {
        let request = GetBundleStatsRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "flashbots_getBundleStats".to_string(),
            params: vec![
                serde_json::json!(format!("0x{}", hex::encode(bundle_hash))),
                serde_json::json!(format!("0x{:x}", block_number)),
            ],
        };
        
        let response = self.client
            .post(&self.relay_url)
            .header("X-Flashbots-Signature", self.sign_request(&request)?)
            .json(&request)
            .send()
            .await
            .map_err(|e| MevError::NetworkError(e.to_string()))?;
        
        response.json::<BundleStats>()
            .await
            .map_err(|e| MevError::RelayError(e.to_string()))
    }
    
    fn sign_request<T: Serialize>(&self, request: &T) -> Result<String> {
        // Sign the request with auth key
        // Implementation would use secp256k1 signature
        Ok(String::new())
    }
}

/// Flashbots bundle format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashbotsBundle {
    #[serde(rename = "txs")]
    pub transactions: Vec<String>, // Raw transaction hex
    #[serde(rename = "blockNumber")]
    pub block_number: String,
    #[serde(rename = "minTimestamp", skip_serializing_if = "Option::is_none")]
    pub min_timestamp: Option<u64>,
    #[serde(rename = "maxTimestamp", skip_serializing_if = "Option::is_none")]
    pub max_timestamp: Option<u64>,
    #[serde(rename = "revertingTxHashes", skip_serializing_if = "Option::is_none")]
    pub reverting_tx_hashes: Option<Vec<String>>,
    #[serde(rename = "replacementUuid", skip_serializing_if = "Option::is_none")]
    pub replacement_uuid: Option<String>,
}

impl FlashbotsBundle {
    pub fn from_bundle(bundle: Bundle) -> Self {
        let transactions = bundle.transactions
            .iter()
            .map(|tx| format!("0x{}", hex::encode(ethereum_rlp::encode(&tx.transaction))))
            .collect();
        
        let reverting_tx_hashes = if bundle.reverting_tx_hashes.is_empty() {
            None
        } else {
            Some(bundle.reverting_tx_hashes
                .iter()
                .map(|h| format!("0x{}", hex::encode(h)))
                .collect())
        };
        
        Self {
            transactions,
            block_number: format!("0x{:x}", bundle.block_number),
            min_timestamp: Some(bundle.min_block),
            max_timestamp: Some(bundle.max_block),
            reverting_tx_hashes,
            replacement_uuid: bundle.replacement_uuid,
        }
    }
}

#[derive(Debug, Serialize)]
struct SendBundleRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Vec<FlashbotsBundle>,
}

#[derive(Debug, Deserialize)]
pub struct SendBundleResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: BundleResponse,
}

#[derive(Debug, Deserialize)]
pub struct BundleResponse {
    #[serde(rename = "bundleHash")]
    pub bundle_hash: String,
}

#[derive(Debug, Serialize)]
struct SimulateBundleRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct SimulateResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: SimulationResult,
}

#[derive(Debug, Deserialize)]
pub struct SimulationResult {
    #[serde(rename = "bundleGasPrice")]
    pub bundle_gas_price: String,
    #[serde(rename = "bundleHash")]
    pub bundle_hash: String,
    #[serde(rename = "coinbaseDiff")]
    pub coinbase_diff: String,
    #[serde(rename = "ethSentToCoinbase")]
    pub eth_sent_to_coinbase: String,
    #[serde(rename = "gasFees")]
    pub gas_fees: String,
    pub results: Vec<TxSimResult>,
    #[serde(rename = "stateBlockNumber")]
    pub state_block_number: u64,
    #[serde(rename = "totalGasUsed")]
    pub total_gas_used: u64,
}

#[derive(Debug, Deserialize)]
pub struct TxSimResult {
    #[serde(rename = "coinbaseDiff")]
    pub coinbase_diff: String,
    #[serde(rename = "ethSentToCoinbase")]
    pub eth_sent_to_coinbase: String,
    #[serde(rename = "fromAddress")]
    pub from_address: String,
    #[serde(rename = "gasFees")]
    pub gas_fees: String,
    #[serde(rename = "gasPrice")]
    pub gas_price: String,
    #[serde(rename = "gasUsed")]
    pub gas_used: u64,
    #[serde(rename = "toAddress")]
    pub to_address: Option<String>,
    #[serde(rename = "txHash")]
    pub tx_hash: String,
    pub value: Option<String>,
    pub error: Option<String>,
    pub revert: Option<String>,
}

#[derive(Debug, Serialize)]
struct GetBundleStatsRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct BundleStats {
    #[serde(rename = "isSimulated")]
    pub is_simulated: bool,
    #[serde(rename = "isSentToMiners")]
    pub is_sent_to_miners: bool,
    #[serde(rename = "isHighPriority")]
    pub is_high_priority: bool,
    #[serde(rename = "simulatedAt")]
    pub simulated_at: Option<String>,
    #[serde(rename = "submittedAt")]
    pub submitted_at: Option<String>,
    #[serde(rename = "sentToMinersAt")]
    pub sent_to_miners_at: Option<String>,
}