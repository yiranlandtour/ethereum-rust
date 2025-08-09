use ethereum_types::H256;
use crate::{Result, RpcError};

pub struct Web3Api {
    client_version: String,
}

impl Web3Api {
    pub fn new(client_version: String) -> Self {
        Self { client_version }
    }
    
    pub async fn client_version(&self) -> Result<String> {
        Ok(self.client_version.clone())
    }
    
    pub async fn sha3(&self, data: String) -> Result<H256> {
        // Decode hex string and compute Keccak256
        let bytes = hex::decode(data.trim_start_matches("0x"))
            .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        
        let hash = ethereum_crypto::keccak256(&bytes);
        Ok(hash)
    }
}