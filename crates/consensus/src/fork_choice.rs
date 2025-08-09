use ethereum_types::{H256, U256};
use ethereum_core::{Block, Header};
use ethereum_storage::Database;
use std::sync::Arc;
use std::collections::{HashMap, HashSet};

use crate::{Result, ConsensusError};

/// Fork choice rule for selecting canonical chain
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkChoiceRule {
    /// Longest chain (most work)
    LongestChain,
    /// GHOST (Greedy Heaviest Observed SubTree)
    GHOST,
    /// LMD-GHOST (Latest Message Driven GHOST) for PoS
    LMDGHOST,
    /// Casper FFG finality
    CasperFFG,
}

/// Fork choice implementation
pub struct ForkChoice<D: Database> {
    db: Arc<D>,
    rule: ForkChoiceRule,
    blocks: HashMap<H256, BlockInfo>,
    children: HashMap<H256, Vec<H256>>,
    attestations: HashMap<H256, Vec<Attestation>>,
}

#[derive(Debug, Clone)]
struct BlockInfo {
    header: Header,
    total_difficulty: U256,
    weight: u64,
    justified: bool,
    finalized: bool,
}

#[derive(Debug, Clone)]
struct Attestation {
    validator: ethereum_types::Address,
    target_block: H256,
    source_epoch: u64,
    target_epoch: u64,
}

impl<D: Database> ForkChoice<D> {
    pub fn new(db: Arc<D>) -> Self {
        Self {
            db,
            rule: ForkChoiceRule::LongestChain,
            blocks: HashMap::new(),
            children: HashMap::new(),
            attestations: HashMap::new(),
        }
    }
    
    /// Set fork choice rule
    pub fn set_rule(&mut self, rule: ForkChoiceRule) {
        self.rule = rule;
    }
    
    /// Select canonical head from competing blocks
    pub async fn select_head(&self, blocks: Vec<Block>) -> Result<Block> {
        if blocks.is_empty() {
            return Err(ConsensusError::ForkChoiceError(
                "No blocks to select from".to_string()
            ));
        }
        
        if blocks.len() == 1 {
            return Ok(blocks.into_iter().next().unwrap());
        }
        
        match self.rule {
            ForkChoiceRule::LongestChain => {
                self.select_longest_chain(blocks)
            }
            ForkChoiceRule::GHOST => {
                self.select_ghost(blocks).await
            }
            ForkChoiceRule::LMDGHOST => {
                self.select_lmd_ghost(blocks).await
            }
            ForkChoiceRule::CasperFFG => {
                self.select_casper_ffg(blocks).await
            }
        }
    }
    
    /// Select head using longest chain rule
    fn select_longest_chain(&self, blocks: Vec<Block>) -> Result<Block> {
        let mut best_block = blocks[0].clone();
        let mut best_difficulty = self.get_total_difficulty(&best_block.header)?;
        
        for block in blocks.into_iter().skip(1) {
            let difficulty = self.get_total_difficulty(&block.header)?;
            
            if difficulty > best_difficulty {
                best_block = block;
                best_difficulty = difficulty;
            } else if difficulty == best_difficulty {
                // Tie-breaker: lower hash wins
                if block.header.hash() < best_block.header.hash() {
                    best_block = block;
                }
            }
        }
        
        Ok(best_block)
    }
    
    /// Select head using GHOST rule
    async fn select_ghost(&self, blocks: Vec<Block>) -> Result<Block> {
        // Build tree of blocks
        let mut tree = self.build_block_tree(&blocks)?;
        
        // Start from genesis or common ancestor
        let mut current = self.find_common_ancestor(&blocks)?;
        
        // Follow heaviest subtree
        while let Some(children) = tree.get(&current) {
            if children.is_empty() {
                break;
            }
            
            let mut heaviest_child = children[0].clone();
            let mut heaviest_weight = self.calculate_subtree_weight(&heaviest_child, &tree)?;
            
            for child in children.iter().skip(1) {
                let weight = self.calculate_subtree_weight(child, &tree)?;
                if weight > heaviest_weight {
                    heaviest_child = child.clone();
                    heaviest_weight = weight;
                }
            }
            
            current = heaviest_child;
        }
        
        // Find block with hash `current`
        blocks.into_iter()
            .find(|b| b.header.hash() == current)
            .ok_or_else(|| ConsensusError::ForkChoiceError(
                "Selected block not found".to_string()
            ))
    }
    
    /// Select head using LMD-GHOST (for PoS)
    async fn select_lmd_ghost(&self, blocks: Vec<Block>) -> Result<Block> {
        // Get latest attestations from validators
        let attestations = self.get_latest_attestations().await?;
        
        // Build weighted tree based on attestations
        let mut weights: HashMap<H256, u64> = HashMap::new();
        
        for attestation in attestations {
            *weights.entry(attestation.target_block).or_insert(0) += 1;
        }
        
        // Follow path with most attestations
        let mut current = self.find_common_ancestor(&blocks)?;
        let tree = self.build_block_tree(&blocks)?;
        
        while let Some(children) = tree.get(&current) {
            if children.is_empty() {
                break;
            }
            
            let mut best_child = children[0].clone();
            let mut best_weight = weights.get(&best_child).copied().unwrap_or(0);
            
            for child in children.iter().skip(1) {
                let weight = weights.get(child).copied().unwrap_or(0);
                if weight > best_weight {
                    best_child = child.clone();
                    best_weight = weight;
                }
            }
            
            current = best_child;
        }
        
        blocks.into_iter()
            .find(|b| b.header.hash() == current)
            .ok_or_else(|| ConsensusError::ForkChoiceError(
                "Selected block not found".to_string()
            ))
    }
    
    /// Select head using Casper FFG finality
    async fn select_casper_ffg(&self, blocks: Vec<Block>) -> Result<Block> {
        // Find latest justified checkpoint
        let justified_checkpoint = self.get_justified_checkpoint().await?;
        
        // Only consider blocks descending from justified checkpoint
        let valid_blocks: Vec<Block> = blocks.into_iter()
            .filter(|b| self.is_descendant_of(&b.header, &justified_checkpoint).unwrap_or(false))
            .collect();
        
        if valid_blocks.is_empty() {
            return Err(ConsensusError::ForkChoiceError(
                "No valid blocks from justified checkpoint".to_string()
            ));
        }
        
        // Apply LMD-GHOST from justified checkpoint
        self.select_lmd_ghost(valid_blocks).await
    }
    
    /// Build block tree from list of blocks
    fn build_block_tree(&self, blocks: &[Block]) -> Result<HashMap<H256, Vec<H256>>> {
        let mut tree: HashMap<H256, Vec<H256>> = HashMap::new();
        
        for block in blocks {
            let parent = block.header.parent_hash;
            let hash = block.header.hash();
            
            tree.entry(parent)
                .or_insert_with(Vec::new)
                .push(hash);
        }
        
        Ok(tree)
    }
    
    /// Find common ancestor of blocks
    fn find_common_ancestor(&self, blocks: &[Block]) -> Result<H256> {
        if blocks.is_empty() {
            return Err(ConsensusError::ForkChoiceError(
                "No blocks provided".to_string()
            ));
        }
        
        // Build set of ancestors for first block
        let mut ancestors = HashSet::new();
        let mut current = blocks[0].header.parent_hash;
        
        while current != H256::zero() {
            ancestors.insert(current);
            
            // Get parent of current
            if let Some(info) = self.blocks.get(&current) {
                current = info.header.parent_hash;
            } else {
                break;
            }
        }
        
        // Find first common ancestor with other blocks
        for block in blocks.iter().skip(1) {
            current = block.header.parent_hash;
            
            while current != H256::zero() && !ancestors.contains(&current) {
                if let Some(info) = self.blocks.get(&current) {
                    current = info.header.parent_hash;
                } else {
                    break;
                }
            }
        }
        
        Ok(current)
    }
    
    /// Calculate weight of subtree rooted at given block
    fn calculate_subtree_weight(
        &self,
        root: &H256,
        tree: &HashMap<H256, Vec<H256>>,
    ) -> Result<u64> {
        let mut weight = 1u64; // Weight of root itself
        
        if let Some(children) = tree.get(root) {
            for child in children {
                weight += self.calculate_subtree_weight(child, tree)?;
            }
        }
        
        Ok(weight)
    }
    
    /// Get total difficulty for a block
    fn get_total_difficulty(&self, header: &Header) -> Result<U256> {
        // In real implementation, would fetch from database
        // For now, use block number as proxy
        Ok(header.number * U256::from(1_000_000))
    }
    
    /// Get latest attestations from validators
    async fn get_latest_attestations(&self) -> Result<Vec<Attestation>> {
        // In real implementation, would fetch from attestation pool
        Ok(vec![])
    }
    
    /// Get latest justified checkpoint
    async fn get_justified_checkpoint(&self) -> Result<H256> {
        // In real implementation, would fetch from consensus state
        Ok(H256::zero())
    }
    
    /// Check if a block is descendant of another
    fn is_descendant_of(&self, block: &Header, ancestor: &H256) -> Result<bool> {
        let mut current = block.parent_hash;
        
        while current != H256::zero() {
            if current == *ancestor {
                return Ok(true);
            }
            
            if let Some(info) = self.blocks.get(&current) {
                current = info.header.parent_hash;
            } else {
                break;
            }
        }
        
        Ok(false)
    }
    
    /// Add block to fork choice
    pub fn add_block(&mut self, block: Block, total_difficulty: U256) {
        let hash = block.header.hash();
        let parent = block.header.parent_hash;
        
        let info = BlockInfo {
            header: block.header,
            total_difficulty,
            weight: 1,
            justified: false,
            finalized: false,
        };
        
        self.blocks.insert(hash, info);
        self.children.entry(parent)
            .or_insert_with(Vec::new)
            .push(hash);
    }
    
    /// Mark block as justified
    pub fn mark_justified(&mut self, block_hash: H256) {
        if let Some(info) = self.blocks.get_mut(&block_hash) {
            info.justified = true;
        }
    }
    
    /// Mark block as finalized
    pub fn mark_finalized(&mut self, block_hash: H256) {
        if let Some(info) = self.blocks.get_mut(&block_hash) {
            info.finalized = true;
            info.justified = true;
        }
    }
    
    /// Prune old blocks from fork choice
    pub fn prune(&mut self, finalized_hash: H256) {
        let mut to_keep = HashSet::new();
        let mut queue = vec![finalized_hash];
        
        // Keep finalized block and all descendants
        while let Some(hash) = queue.pop() {
            to_keep.insert(hash);
            
            if let Some(children) = self.children.get(&hash) {
                queue.extend(children.iter().copied());
            }
        }
        
        // Remove blocks not in to_keep set
        self.blocks.retain(|hash, _| to_keep.contains(hash));
        self.children.retain(|hash, _| to_keep.contains(hash));
        self.attestations.retain(|hash, _| to_keep.contains(hash));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fork_choice_rule() {
        assert_eq!(ForkChoiceRule::LongestChain, ForkChoiceRule::LongestChain);
        assert_ne!(ForkChoiceRule::GHOST, ForkChoiceRule::LMDGHOST);
    }
}