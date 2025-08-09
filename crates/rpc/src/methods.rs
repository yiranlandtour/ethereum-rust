use std::sync::Arc;
use serde_json::Value;
use ethereum_storage::Database;
use ethereum_core::Block;
use ethereum_types::{H256, U256};

use crate::{RpcRequest, RpcError, Result};
use crate::eth::EthApi;
use crate::net::NetApi;
use crate::web3::Web3Api;

pub struct RpcHandler {
    eth_api: Arc<EthApi>,
    net_api: Arc<NetApi>,
    web3_api: Arc<Web3Api>,
}

impl RpcHandler {
    pub fn new<D: Database + 'static>(
        db: Arc<D>,
        chain_id: u64,
        client_version: String,
    ) -> Self {
        let eth_api = Arc::new(EthApi::new(db.clone()));
        let net_api = Arc::new(NetApi::new(chain_id));
        let web3_api = Arc::new(Web3Api::new(client_version));
        
        Self {
            eth_api,
            net_api,
            web3_api,
        }
    }
    
    pub async fn handle_request(&self, request: RpcRequest) -> Result<Value> {
        let method_parts: Vec<&str> = request.method.split('_').collect();
        
        if method_parts.len() < 2 {
            return Err(RpcError::MethodNotFound(request.method));
        }
        
        let namespace = method_parts[0];
        let method = method_parts[1..].join("_");
        let params = request.params.unwrap_or(Value::Null);
        
        match namespace {
            "eth" => self.handle_eth_method(&method, params).await,
            "net" => self.handle_net_method(&method, params).await,
            "web3" => self.handle_web3_method(&method, params).await,
            _ => Err(RpcError::MethodNotFound(request.method)),
        }
    }
    
    async fn handle_eth_method(&self, method: &str, params: Value) -> Result<Value> {
        match method {
            "blockNumber" => {
                let block_number = self.eth_api.block_number().await?;
                Ok(serde_json::to_value(block_number)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "getBalance" => {
                let params: Vec<Value> = serde_json::from_value(params)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                if params.len() < 1 {
                    return Err(RpcError::InvalidParams("Missing address parameter".to_string()));
                }
                
                let address = serde_json::from_value(params[0].clone())
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                let block_number = if params.len() > 1 {
                    Some(serde_json::from_value(params[1].clone())
                        .map_err(|e| RpcError::InvalidParams(e.to_string()))?)
                } else {
                    None
                };
                
                let balance = self.eth_api.get_balance(address, block_number).await?;
                Ok(serde_json::to_value(balance)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "getTransactionCount" => {
                let params: Vec<Value> = serde_json::from_value(params)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                if params.len() < 1 {
                    return Err(RpcError::InvalidParams("Missing address parameter".to_string()));
                }
                
                let address = serde_json::from_value(params[0].clone())
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                let block_number = if params.len() > 1 {
                    Some(serde_json::from_value(params[1].clone())
                        .map_err(|e| RpcError::InvalidParams(e.to_string()))?)
                } else {
                    None
                };
                
                let count = self.eth_api.get_transaction_count(address, block_number).await?;
                Ok(serde_json::to_value(count)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "getBlockByHash" => {
                let params: Vec<Value> = serde_json::from_value(params)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                if params.len() < 2 {
                    return Err(RpcError::InvalidParams("Missing parameters".to_string()));
                }
                
                let hash = serde_json::from_value(params[0].clone())
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                let full_txs = serde_json::from_value(params[1].clone())
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                let block = self.eth_api.get_block_by_hash(hash, full_txs).await?;
                Ok(serde_json::to_value(block)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "getBlockByNumber" => {
                let params: Vec<Value> = serde_json::from_value(params)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                if params.len() < 2 {
                    return Err(RpcError::InvalidParams("Missing parameters".to_string()));
                }
                
                let number = serde_json::from_value(params[0].clone())
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                let full_txs = serde_json::from_value(params[1].clone())
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                let block = self.eth_api.get_block_by_number(number, full_txs).await?;
                Ok(serde_json::to_value(block)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "getTransactionByHash" => {
                let params: Vec<Value> = serde_json::from_value(params)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                if params.is_empty() {
                    return Err(RpcError::InvalidParams("Missing hash parameter".to_string()));
                }
                
                let hash = serde_json::from_value(params[0].clone())
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                let tx = self.eth_api.get_transaction_by_hash(hash).await?;
                Ok(serde_json::to_value(tx)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "getTransactionReceipt" => {
                let params: Vec<Value> = serde_json::from_value(params)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                if params.is_empty() {
                    return Err(RpcError::InvalidParams("Missing hash parameter".to_string()));
                }
                
                let hash = serde_json::from_value(params[0].clone())
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                let receipt = self.eth_api.get_transaction_receipt(hash).await?;
                Ok(serde_json::to_value(receipt)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "call" => {
                let params: Vec<Value> = serde_json::from_value(params)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                if params.is_empty() {
                    return Err(RpcError::InvalidParams("Missing call request".to_string()));
                }
                
                let call_request = serde_json::from_value(params[0].clone())
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                let block_number = if params.len() > 1 {
                    Some(serde_json::from_value(params[1].clone())
                        .map_err(|e| RpcError::InvalidParams(e.to_string()))?)
                } else {
                    None
                };
                
                let result = self.eth_api.call(call_request, block_number).await?;
                Ok(serde_json::to_value(result)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "estimateGas" => {
                let params: Vec<Value> = serde_json::from_value(params)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                if params.is_empty() {
                    return Err(RpcError::InvalidParams("Missing call request".to_string()));
                }
                
                let call_request = serde_json::from_value(params[0].clone())
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                let gas = self.eth_api.estimate_gas(call_request).await?;
                Ok(serde_json::to_value(gas)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "gasPrice" => {
                let price = self.eth_api.gas_price().await?;
                Ok(serde_json::to_value(price)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "chainId" => {
                let chain_id = self.eth_api.chain_id().await?;
                Ok(serde_json::to_value(chain_id)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "syncing" => {
                let syncing = self.eth_api.syncing().await?;
                Ok(serde_json::to_value(syncing)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "mining" => {
                let mining = self.eth_api.mining().await?;
                Ok(serde_json::to_value(mining)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "hashrate" => {
                let hashrate = self.eth_api.hashrate().await?;
                Ok(serde_json::to_value(hashrate)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "accounts" => {
                let accounts = self.eth_api.accounts().await?;
                Ok(serde_json::to_value(accounts)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "sendRawTransaction" => {
                let params: Vec<Value> = serde_json::from_value(params)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                if params.is_empty() {
                    return Err(RpcError::InvalidParams("Missing transaction data".to_string()));
                }
                
                let tx_data: String = serde_json::from_value(params[0].clone())
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                let hash = self.eth_api.send_raw_transaction(tx_data).await?;
                Ok(serde_json::to_value(hash)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            _ => Err(RpcError::MethodNotFound(format!("eth_{}", method))),
        }
    }
    
    async fn handle_net_method(&self, method: &str, _params: Value) -> Result<Value> {
        match method {
            "version" => {
                let version = self.net_api.version().await?;
                Ok(serde_json::to_value(version)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "peerCount" => {
                let count = self.net_api.peer_count().await?;
                Ok(serde_json::to_value(count)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "listening" => {
                let listening = self.net_api.listening().await?;
                Ok(serde_json::to_value(listening)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            _ => Err(RpcError::MethodNotFound(format!("net_{}", method))),
        }
    }
    
    async fn handle_web3_method(&self, method: &str, params: Value) -> Result<Value> {
        match method {
            "clientVersion" => {
                let version = self.web3_api.client_version().await?;
                Ok(serde_json::to_value(version)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            "sha3" => {
                let params: Vec<Value> = serde_json::from_value(params)
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                if params.is_empty() {
                    return Err(RpcError::InvalidParams("Missing data parameter".to_string()));
                }
                
                let data: String = serde_json::from_value(params[0].clone())
                    .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
                
                let hash = self.web3_api.sha3(data).await?;
                Ok(serde_json::to_value(hash)
                    .map_err(|e| RpcError::InternalError(e.to_string()))?)
            }
            _ => Err(RpcError::MethodNotFound(format!("web3_{}", method))),
        }
    }
}