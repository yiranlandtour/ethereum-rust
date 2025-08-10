use ethereum_types::{H256, U256};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::{EngineError, Result};
use crate::types::{ForkchoiceStateV1, PayloadStatus, PayloadStatusV1};

pub struct ForkChoiceState {
    pub head: H256,
    pub safe: H256,
    pub finalized: H256,
}

pub struct ForkChoiceUpdate {
    pub head_block: Option<H256>,
    pub safe_block: Option<H256>,
    pub finalized_block: Option<H256>,
}

pub struct ForkChoiceStore {
    head: Arc<RwLock<H256>>,
    safe: Arc<RwLock<H256>>,
    finalized: Arc<RwLock<H256>>,
    blocks: Arc<RwLock<HashMap<H256, BlockInfo>>>,
}

struct BlockInfo {
    pub hash: H256,
    pub parent_hash: H256,
    pub number: u64,
    pub total_difficulty: U256,
    pub validated: bool,
}

impl ForkChoiceStore {
    pub fn new() -> Self {
        Self {
            head: Arc::new(RwLock::new(H256::zero())),
            safe: Arc::new(RwLock::new(H256::zero())),
            finalized: Arc::new(RwLock::new(H256::zero())),
            blocks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn update_forkchoice(&self, state: ForkchoiceStateV1) -> Result<PayloadStatusV1> {
        let mut head = self.head.write().unwrap();
        let mut safe = self.safe.write().unwrap();
        let mut finalized = self.finalized.write().unwrap();

        if !self.is_valid_block(&state.head_block_hash)? {
            return Ok(PayloadStatusV1 {
                status: PayloadStatus::Invalid,
                latest_valid_hash: Some(*finalized),
                validation_error: Some("Invalid head block".to_string()),
            });
        }

        if state.safe_block_hash != H256::zero() && !self.is_valid_block(&state.safe_block_hash)? {
            return Ok(PayloadStatusV1 {
                status: PayloadStatus::Invalid,
                latest_valid_hash: Some(*finalized),
                validation_error: Some("Invalid safe block".to_string()),
            });
        }

        if state.finalized_block_hash != H256::zero() && !self.is_valid_block(&state.finalized_block_hash)? {
            return Ok(PayloadStatusV1 {
                status: PayloadStatus::Invalid,
                latest_valid_hash: Some(*finalized),
                validation_error: Some("Invalid finalized block".to_string()),
            });
        }

        *head = state.head_block_hash;
        *safe = state.safe_block_hash;
        *finalized = state.finalized_block_hash;

        Ok(PayloadStatusV1 {
            status: PayloadStatus::Valid,
            latest_valid_hash: Some(state.head_block_hash),
            validation_error: None,
        })
    }

    pub fn add_block(&self, hash: H256, parent_hash: H256, number: u64, total_difficulty: U256) {
        let mut blocks = self.blocks.write().unwrap();
        blocks.insert(hash, BlockInfo {
            hash,
            parent_hash,
            number,
            total_difficulty,
            validated: false,
        });
    }

    pub fn validate_block(&self, hash: &H256) -> Result<()> {
        let mut blocks = self.blocks.write().unwrap();
        if let Some(block) = blocks.get_mut(hash) {
            block.validated = true;
            Ok(())
        } else {
            Err(EngineError::InvalidForkChoiceState("Block not found".to_string()))
        }
    }

    pub fn is_valid_block(&self, hash: &H256) -> Result<bool> {
        if *hash == H256::zero() {
            return Ok(true);
        }

        let blocks = self.blocks.read().unwrap();
        if let Some(block) = blocks.get(hash) {
            Ok(block.validated)
        } else {
            Ok(false)
        }
    }

    pub fn get_head(&self) -> H256 {
        *self.head.read().unwrap()
    }

    pub fn get_safe(&self) -> H256 {
        *self.safe.read().unwrap()
    }

    pub fn get_finalized(&self) -> H256 {
        *self.finalized.read().unwrap()
    }

    pub fn is_canonical(&self, hash: &H256) -> bool {
        let head = self.head.read().unwrap();
        let blocks = self.blocks.read().unwrap();
        
        let mut current = *hash;
        while current != H256::zero() {
            if current == *head {
                return true;
            }
            
            if let Some(block) = blocks.get(&current) {
                current = block.parent_hash;
            } else {
                break;
            }
        }
        
        false
    }

    pub fn prune_finalized(&self) {
        let finalized = self.finalized.read().unwrap();
        if *finalized == H256::zero() {
            return;
        }

        let mut blocks = self.blocks.write().unwrap();
        let finalized_block = blocks.get(&finalized).cloned();
        
        if let Some(finalized_info) = finalized_block {
            blocks.retain(|_, block| {
                block.number >= finalized_info.number
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forkchoice_update() {
        let store = ForkChoiceStore::new();
        
        let hash1 = H256::from([1u8; 32]);
        let hash2 = H256::from([2u8; 32]);
        let hash3 = H256::from([3u8; 32]);
        
        store.add_block(hash1, H256::zero(), 1, U256::from(100));
        store.add_block(hash2, hash1, 2, U256::from(200));
        store.add_block(hash3, hash2, 3, U256::from(300));
        
        store.validate_block(&hash1).unwrap();
        store.validate_block(&hash2).unwrap();
        store.validate_block(&hash3).unwrap();
        
        let state = ForkchoiceStateV1 {
            head_block_hash: hash3,
            safe_block_hash: hash2,
            finalized_block_hash: hash1,
        };
        
        let status = store.update_forkchoice(state).unwrap();
        assert_eq!(status.status, PayloadStatus::Valid);
        assert_eq!(store.get_head(), hash3);
        assert_eq!(store.get_safe(), hash2);
        assert_eq!(store.get_finalized(), hash1);
    }

    #[test]
    fn test_canonical_chain() {
        let store = ForkChoiceStore::new();
        
        let hash1 = H256::from([1u8; 32]);
        let hash2 = H256::from([2u8; 32]);
        let hash3 = H256::from([3u8; 32]);
        let hash4 = H256::from([4u8; 32]);
        
        store.add_block(hash1, H256::zero(), 1, U256::from(100));
        store.add_block(hash2, hash1, 2, U256::from(200));
        store.add_block(hash3, hash2, 3, U256::from(300));
        store.add_block(hash4, hash1, 2, U256::from(200));
        
        store.validate_block(&hash1).unwrap();
        store.validate_block(&hash2).unwrap();
        store.validate_block(&hash3).unwrap();
        store.validate_block(&hash4).unwrap();
        
        let state = ForkchoiceStateV1 {
            head_block_hash: hash3,
            safe_block_hash: hash2,
            finalized_block_hash: hash1,
        };
        
        store.update_forkchoice(state).unwrap();
        
        assert!(store.is_canonical(&hash3));
        assert!(store.is_canonical(&hash2));
        assert!(store.is_canonical(&hash1));
        assert!(!store.is_canonical(&hash4));
    }
}