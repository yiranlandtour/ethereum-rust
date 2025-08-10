use ethereum_types::{H256, U256, Address};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::{Result, VerkleError};
use crate::commitment::Commitment;
use crate::proof::VerkleProof;

/// Verkle witness for stateless execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerkleWitness {
    /// Root commitment of the state tree
    pub state_root: Commitment,
    
    /// Accessed accounts
    pub accounts: HashMap<Address, AccountWitness>,
    
    /// Accessed storage slots
    pub storage: HashMap<Address, HashMap<H256, H256>>,
    
    /// Code chunks accessed
    pub code_chunks: HashMap<H256, Vec<CodeChunk>>,
    
    /// Proofs for all accessed values
    pub proofs: Vec<VerkleProof>,
    
    /// Gas used for witness access
    pub gas_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountWitness {
    pub nonce: U256,
    pub balance: U256,
    pub code_hash: H256,
    pub code_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    pub offset: u64,
    pub data: Vec<u8>,
}

impl VerkleWitness {
    pub fn new(state_root: Commitment) -> Self {
        Self {
            state_root,
            accounts: HashMap::new(),
            storage: HashMap::new(),
            code_chunks: HashMap::new(),
            proofs: Vec::new(),
            gas_used: 0,
        }
    }
    
    pub fn add_account(&mut self, address: Address, account: AccountWitness) {
        self.accounts.insert(address, account);
        self.gas_used += WITNESS_ACCOUNT_COST;
    }
    
    pub fn add_storage(&mut self, address: Address, slot: H256, value: H256) {
        self.storage
            .entry(address)
            .or_insert_with(HashMap::new)
            .insert(slot, value);
        self.gas_used += WITNESS_STORAGE_COST;
    }
    
    pub fn add_code_chunk(&mut self, code_hash: H256, chunk: CodeChunk) {
        self.code_chunks
            .entry(code_hash)
            .or_insert_with(Vec::new)
            .push(chunk);
        self.gas_used += WITNESS_CODE_CHUNK_COST;
    }
    
    pub fn add_proof(&mut self, proof: VerkleProof) {
        self.proofs.push(proof);
    }
    
    pub fn size(&self) -> usize {
        let mut size = 32; // state root
        
        // Account data
        size += self.accounts.len() * (20 + 32 * 3 + 8); // address + nonce + balance + code_hash + code_size
        
        // Storage data
        for (_, slots) in &self.storage {
            size += slots.len() * 64; // key + value
        }
        
        // Code chunks
        for (_, chunks) in &self.code_chunks {
            for chunk in chunks {
                size += 8 + chunk.data.len(); // offset + data
            }
        }
        
        // Proofs
        for proof in &self.proofs {
            size += proof.key.len() + proof.value.as_ref().map_or(0, |v| v.len());
            size += proof.proof_nodes.len() * 64; // rough estimate
        }
        
        size
    }
    
    pub fn verify(&self) -> Result<bool> {
        // Verify all proofs
        for proof in &self.proofs {
            if proof.root_commitment.value != self.state_root.value {
                return Ok(false);
            }
        }
        
        Ok(true)
    }
}

// Gas costs for witness access
const WITNESS_ACCOUNT_COST: u64 = 2500;
const WITNESS_STORAGE_COST: u64 = 2100;
const WITNESS_CODE_CHUNK_COST: u64 = 200;

/// Witness builder for constructing witnesses during execution
pub struct WitnessBuilder {
    witness: VerkleWitness,
    accessed_keys: HashSet<Vec<u8>>,
    access_list: AccessList,
}

impl WitnessBuilder {
    pub fn new(state_root: Commitment) -> Self {
        Self {
            witness: VerkleWitness::new(state_root),
            accessed_keys: HashSet::new(),
            access_list: AccessList::new(),
        }
    }
    
    pub fn access_account(&mut self, address: Address) -> Result<()> {
        if self.access_list.has_account(&address) {
            return Ok(());
        }
        
        // Mark as accessed
        self.access_list.add_account(address);
        
        // Add to witness (in production, fetch from state)
        let account = AccountWitness {
            nonce: U256::zero(),
            balance: U256::zero(),
            code_hash: H256::zero(),
            code_size: 0,
        };
        
        self.witness.add_account(address, account);
        
        Ok(())
    }
    
    pub fn access_storage(&mut self, address: Address, slot: H256) -> Result<()> {
        if self.access_list.has_storage(&address, &slot) {
            return Ok(());
        }
        
        // Mark as accessed
        self.access_list.add_storage(address, slot);
        
        // Add to witness (in production, fetch from state)
        self.witness.add_storage(address, slot, H256::zero());
        
        Ok(())
    }
    
    pub fn access_code(&mut self, address: Address, offset: u64, size: u64) -> Result<()> {
        // Calculate which chunks are needed
        let start_chunk = offset / 31;
        let end_chunk = (offset + size - 1) / 31;
        
        for chunk_index in start_chunk..=end_chunk {
            if self.access_list.has_code_chunk(&address, chunk_index) {
                continue;
            }
            
            // Mark as accessed
            self.access_list.add_code_chunk(address, chunk_index);
            
            // Add to witness (in production, fetch from state)
            let chunk = CodeChunk {
                offset: chunk_index * 31,
                data: vec![0u8; 31],
            };
            
            self.witness.add_code_chunk(H256::zero(), chunk);
        }
        
        Ok(())
    }
    
    pub fn build(self) -> VerkleWitness {
        self.witness
    }
}

/// Access list for tracking accessed state
#[derive(Debug, Clone)]
struct AccessList {
    accounts: HashSet<Address>,
    storage: HashMap<Address, HashSet<H256>>,
    code_chunks: HashMap<Address, HashSet<u64>>,
}

impl AccessList {
    fn new() -> Self {
        Self {
            accounts: HashSet::new(),
            storage: HashMap::new(),
            code_chunks: HashMap::new(),
        }
    }
    
    fn add_account(&mut self, address: Address) {
        self.accounts.insert(address);
    }
    
    fn has_account(&self, address: &Address) -> bool {
        self.accounts.contains(address)
    }
    
    fn add_storage(&mut self, address: Address, slot: H256) {
        self.storage
            .entry(address)
            .or_insert_with(HashSet::new)
            .insert(slot);
    }
    
    fn has_storage(&self, address: &Address, slot: &H256) -> bool {
        self.storage
            .get(address)
            .map_or(false, |slots| slots.contains(slot))
    }
    
    fn add_code_chunk(&mut self, address: Address, chunk_index: u64) {
        self.code_chunks
            .entry(address)
            .or_insert_with(HashSet::new)
            .insert(chunk_index);
    }
    
    fn has_code_chunk(&self, address: &Address, chunk_index: u64) -> bool {
        self.code_chunks
            .get(address)
            .map_or(false, |chunks| chunks.contains(&chunk_index))
    }
}

/// Witness aggregator for combining multiple witnesses
pub struct WitnessAggregator {
    witnesses: Vec<VerkleWitness>,
}

impl WitnessAggregator {
    pub fn new() -> Self {
        Self {
            witnesses: Vec::new(),
        }
    }
    
    pub fn add_witness(&mut self, witness: VerkleWitness) {
        self.witnesses.push(witness);
    }
    
    pub fn aggregate(self) -> Result<VerkleWitness> {
        if self.witnesses.is_empty() {
            return Err(VerkleError::InvalidProof("No witnesses to aggregate".to_string()));
        }
        
        let mut aggregated = VerkleWitness::new(self.witnesses[0].state_root.clone());
        
        for witness in self.witnesses {
            // Merge accounts
            for (address, account) in witness.accounts {
                aggregated.accounts.insert(address, account);
            }
            
            // Merge storage
            for (address, slots) in witness.storage {
                let storage = aggregated.storage.entry(address).or_insert_with(HashMap::new);
                for (slot, value) in slots {
                    storage.insert(slot, value);
                }
            }
            
            // Merge code chunks
            for (hash, chunks) in witness.code_chunks {
                aggregated.code_chunks
                    .entry(hash)
                    .or_insert_with(Vec::new)
                    .extend(chunks);
            }
            
            // Merge proofs
            aggregated.proofs.extend(witness.proofs);
            
            // Sum gas
            aggregated.gas_used += witness.gas_used;
        }
        
        Ok(aggregated)
    }
}

/// Witness validator for checking witness completeness
pub struct WitnessValidator {
    required_accounts: HashSet<Address>,
    required_storage: HashMap<Address, HashSet<H256>>,
    required_code: HashSet<H256>,
}

impl WitnessValidator {
    pub fn new() -> Self {
        Self {
            required_accounts: HashSet::new(),
            required_storage: HashMap::new(),
            required_code: HashSet::new(),
        }
    }
    
    pub fn require_account(&mut self, address: Address) {
        self.required_accounts.insert(address);
    }
    
    pub fn require_storage(&mut self, address: Address, slot: H256) {
        self.required_storage
            .entry(address)
            .or_insert_with(HashSet::new)
            .insert(slot);
    }
    
    pub fn require_code(&mut self, code_hash: H256) {
        self.required_code.insert(code_hash);
    }
    
    pub fn validate(&self, witness: &VerkleWitness) -> Result<bool> {
        // Check all required accounts are present
        for address in &self.required_accounts {
            if !witness.accounts.contains_key(address) {
                return Ok(false);
            }
        }
        
        // Check all required storage is present
        for (address, slots) in &self.required_storage {
            if let Some(witness_storage) = witness.storage.get(address) {
                for slot in slots {
                    if !witness_storage.contains_key(slot) {
                        return Ok(false);
                    }
                }
            } else {
                return Ok(false);
            }
        }
        
        // Check all required code is present
        for code_hash in &self.required_code {
            if !witness.code_chunks.contains_key(code_hash) {
                return Ok(false);
            }
        }
        
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_witness_builder() {
        let root = Commitment::default();
        let mut builder = WitnessBuilder::new(root);
        
        let address = Address::random();
        builder.access_account(address).unwrap();
        builder.access_storage(address, H256::random()).unwrap();
        
        let witness = builder.build();
        assert!(witness.accounts.contains_key(&address));
        assert!(witness.storage.contains_key(&address));
    }
    
    #[test]
    fn test_witness_aggregator() {
        let root = Commitment::default();
        
        let mut witness1 = VerkleWitness::new(root.clone());
        witness1.add_account(Address::random(), AccountWitness {
            nonce: U256::from(1),
            balance: U256::from(100),
            code_hash: H256::zero(),
            code_size: 0,
        });
        
        let mut witness2 = VerkleWitness::new(root);
        witness2.add_account(Address::random(), AccountWitness {
            nonce: U256::from(2),
            balance: U256::from(200),
            code_hash: H256::zero(),
            code_size: 0,
        });
        
        let mut aggregator = WitnessAggregator::new();
        aggregator.add_witness(witness1);
        aggregator.add_witness(witness2);
        
        let aggregated = aggregator.aggregate().unwrap();
        assert_eq!(aggregated.accounts.len(), 2);
    }
}