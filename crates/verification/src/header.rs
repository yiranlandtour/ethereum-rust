use ethereum_types::{H256, U256};
use ethereum_core::Header;
use ethereum_storage::Database;
use std::sync::Arc;

use crate::{Result, VerificationError};

/// Header verifier
pub struct HeaderVerifier<D: Database> {
    db: Arc<D>,
}

impl<D: Database> HeaderVerifier<D> {
    pub fn new(db: Arc<D>) -> Self {
        Self { db }
    }
    
    /// Verify header
    pub async fn verify(&self, header: &Header) -> Result<()> {
        // Verify basic header constraints
        self.verify_basic_constraints(header)?;
        
        // Verify parent exists (except for genesis)
        if header.number > U256::zero() {
            self.verify_parent_exists(header)?;
            
            // Verify against parent
            self.verify_against_parent(header).await?;
        }
        
        // Verify timestamp
        self.verify_timestamp(header)?;
        
        // Verify gas limit
        self.verify_gas_limit(header)?;
        
        // Verify extra data
        self.verify_extra_data(header)?;
        
        Ok(())
    }
    
    /// Verify basic header constraints
    fn verify_basic_constraints(&self, header: &Header) -> Result<()> {
        // Gas used must not exceed gas limit
        if header.gas_used > header.gas_limit {
            return Err(VerificationError::InvalidHeader(
                format!("Gas used {} exceeds gas limit {}", 
                        header.gas_used, header.gas_limit)
            ));
        }
        
        // Block number overflow check
        if header.number == U256::MAX {
            return Err(VerificationError::InvalidHeader(
                "Block number overflow".to_string()
            ));
        }
        
        Ok(())
    }
    
    /// Verify parent exists
    fn verify_parent_exists(&self, header: &Header) -> Result<()> {
        let parent_key = format!("header:{}", hex::encode(header.parent_hash));
        
        if self.db.get(parent_key.as_bytes())?.is_none() {
            return Err(VerificationError::ParentNotFound);
        }
        
        Ok(())
    }
    
    /// Verify against parent header
    async fn verify_against_parent(&self, header: &Header) -> Result<()> {
        // Get parent header
        let parent = self.get_parent_header(header)?;
        
        // Block number must be parent + 1
        if header.number != parent.number + U256::one() {
            return Err(VerificationError::InvalidHeader(
                format!("Invalid block number: expected {}, got {}", 
                        parent.number + U256::one(), header.number)
            ));
        }
        
        // Timestamp must be greater than parent
        if header.timestamp <= parent.timestamp {
            return Err(VerificationError::InvalidHeader(
                "Timestamp not greater than parent".to_string()
            ));
        }
        
        // Gas limit adjustment check (EIP-1559)
        self.verify_gas_limit_adjustment(header, &parent)?;
        
        Ok(())
    }
    
    /// Get parent header
    fn get_parent_header(&self, header: &Header) -> Result<Header> {
        let parent_key = format!("header:{}", hex::encode(header.parent_hash));
        
        let parent_data = self.db.get(parent_key.as_bytes())?
            .ok_or(VerificationError::ParentNotFound)?;
        
        bincode::deserialize(&parent_data)
            .map_err(|_| VerificationError::InvalidHeader(
                "Failed to deserialize parent header".to_string()
            ))
    }
    
    /// Verify timestamp
    fn verify_timestamp(&self, header: &Header) -> Result<()> {
        // Timestamp must not be zero
        if header.timestamp == 0 {
            return Err(VerificationError::InvalidHeader(
                "Timestamp cannot be zero".to_string()
            ));
        }
        
        // Timestamp must not be too far in the future
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        const MAX_FUTURE_TIME: u64 = 900; // 15 minutes
        if header.timestamp > current_time + MAX_FUTURE_TIME {
            return Err(VerificationError::InvalidHeader(
                format!("Timestamp too far in future: {} > {}", 
                        header.timestamp, current_time + MAX_FUTURE_TIME)
            ));
        }
        
        Ok(())
    }
    
    /// Verify gas limit
    fn verify_gas_limit(&self, header: &Header) -> Result<()> {
        const MIN_GAS_LIMIT: u64 = 5000;
        const MAX_GAS_LIMIT: u64 = 0x7fffffffffffffff;
        
        if header.gas_limit < U256::from(MIN_GAS_LIMIT) {
            return Err(VerificationError::InvalidHeader(
                format!("Gas limit too low: {} < {}", header.gas_limit, MIN_GAS_LIMIT)
            ));
        }
        
        if header.gas_limit > U256::from(MAX_GAS_LIMIT) {
            return Err(VerificationError::InvalidHeader(
                format!("Gas limit too high: {} > {}", header.gas_limit, MAX_GAS_LIMIT)
            ));
        }
        
        Ok(())
    }
    
    /// Verify gas limit adjustment
    fn verify_gas_limit_adjustment(&self, header: &Header, parent: &Header) -> Result<()> {
        // Gas limit can only change by 1/1024 of parent gas limit
        let parent_gas = parent.gas_limit.as_u64();
        let adjustment_limit = parent_gas / 1024;
        
        let current_gas = header.gas_limit.as_u64();
        
        if current_gas > parent_gas {
            let increase = current_gas - parent_gas;
            if increase > adjustment_limit {
                return Err(VerificationError::InvalidHeader(
                    format!("Gas limit increase too large: {} > {}", 
                            increase, adjustment_limit)
                ));
            }
        } else {
            let decrease = parent_gas - current_gas;
            if decrease > adjustment_limit {
                return Err(VerificationError::InvalidHeader(
                    format!("Gas limit decrease too large: {} > {}", 
                            decrease, adjustment_limit)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Verify extra data
    fn verify_extra_data(&self, header: &Header) -> Result<()> {
        // Extra data size limit (without seal)
        const MAX_EXTRA_DATA_SIZE: usize = 32;
        
        // For sealed blocks, extra data includes signature (65 bytes)
        // So we check the non-signature part
        let extra_size = if header.extra_data.len() > 65 {
            header.extra_data.len() - 65
        } else {
            header.extra_data.len()
        };
        
        if extra_size > MAX_EXTRA_DATA_SIZE {
            return Err(VerificationError::InvalidHeader(
                format!("Extra data too large: {} > {}", 
                        extra_size, MAX_EXTRA_DATA_SIZE)
            ));
        }
        
        Ok(())
    }
    
    /// Verify header chain
    pub async fn verify_chain(&self, headers: &[Header]) -> Result<()> {
        if headers.is_empty() {
            return Ok(());
        }
        
        // Verify first header
        self.verify(&headers[0]).await?;
        
        // Verify subsequent headers form a chain
        for i in 1..headers.len() {
            self.verify(&headers[i]).await?;
            
            // Check that headers are connected
            if headers[i].parent_hash != headers[i - 1].hash() {
                return Err(VerificationError::InvalidHeader(
                    "Headers do not form a chain".to_string()
                ));
            }
        }
        
        Ok(())
    }
}