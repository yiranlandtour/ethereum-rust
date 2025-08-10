use ethereum_types::H256;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{Result, VerkleError};
use crate::commitment::Commitment;

/// Verkle proof for a single key-value pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerkleProof {
    pub key: Vec<u8>,
    pub value: Option<Vec<u8>>,
    pub proof_nodes: Vec<(Vec<u8>, Commitment)>,
    pub root_commitment: Commitment,
}

impl VerkleProof {
    pub fn new(
        key: Vec<u8>,
        value: Option<Vec<u8>>,
        proof_nodes: Vec<(Vec<u8>, Commitment)>,
        root_commitment: Commitment,
    ) -> Self {
        Self {
            key,
            value,
            proof_nodes,
            root_commitment,
        }
    }
    
    pub fn is_inclusion_proof(&self) -> bool {
        self.value.is_some()
    }
    
    pub fn is_exclusion_proof(&self) -> bool {
        self.value.is_none()
    }
    
    pub fn size(&self) -> usize {
        self.proof_nodes.len()
    }
}

/// Proof verifier
pub struct ProofVerifier {
    commitment_verifier: Box<dyn CommitmentVerifier>,
}

impl ProofVerifier {
    pub fn new(commitment_verifier: Box<dyn CommitmentVerifier>) -> Self {
        Self {
            commitment_verifier,
        }
    }
    
    pub fn verify(&self, proof: &VerkleProof) -> Result<bool> {
        // Verify the proof path
        let mut current_commitment = if let Some(value) = &proof.value {
            self.commitment_verifier.compute_leaf_commitment(value)?
        } else {
            Commitment::default()
        };
        
        // Reconstruct root from proof nodes
        for (path_segment, sibling_commitment) in proof.proof_nodes.iter().rev() {
            current_commitment = self.commitment_verifier.combine_commitments(
                &current_commitment,
                sibling_commitment,
                path_segment,
            )?;
        }
        
        // Compare with claimed root
        Ok(current_commitment.value == proof.root_commitment.value)
    }
    
    pub fn verify_batch(&self, proofs: &[VerkleProof]) -> Result<Vec<bool>> {
        let mut results = Vec::new();
        
        for proof in proofs {
            results.push(self.verify(proof)?);
        }
        
        Ok(results)
    }
}

pub trait CommitmentVerifier: Send + Sync {
    fn compute_leaf_commitment(&self, value: &[u8]) -> Result<Commitment>;
    
    fn combine_commitments(
        &self,
        left: &Commitment,
        right: &Commitment,
        path: &[u8],
    ) -> Result<Commitment>;
}

/// Multi-proof for efficient batch verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiProof {
    pub keys: Vec<Vec<u8>>,
    pub values: Vec<Option<Vec<u8>>>,
    pub commitments: HashMap<Vec<u8>, Commitment>,
    pub root_commitment: Commitment,
}

impl MultiProof {
    pub fn new(
        keys: Vec<Vec<u8>>,
        values: Vec<Option<Vec<u8>>>,
        root_commitment: Commitment,
    ) -> Self {
        Self {
            keys,
            values,
            commitments: HashMap::new(),
            root_commitment,
        }
    }
    
    pub fn add_commitment(&mut self, path: Vec<u8>, commitment: Commitment) {
        self.commitments.insert(path, commitment);
    }
    
    pub fn size(&self) -> usize {
        self.keys.len()
    }
    
    pub fn proof_size(&self) -> usize {
        self.commitments.len()
    }
}

/// Proof aggregator for combining multiple proofs
pub struct ProofAggregator {
    proofs: Vec<VerkleProof>,
}

impl ProofAggregator {
    pub fn new() -> Self {
        Self {
            proofs: Vec::new(),
        }
    }
    
    pub fn add_proof(&mut self, proof: VerkleProof) {
        self.proofs.push(proof);
    }
    
    pub fn aggregate(&self) -> Result<MultiProof> {
        if self.proofs.is_empty() {
            return Err(VerkleError::InvalidProof("No proofs to aggregate".to_string()));
        }
        
        let root_commitment = self.proofs[0].root_commitment.clone();
        
        // Verify all proofs have the same root
        for proof in &self.proofs[1..] {
            if proof.root_commitment.value != root_commitment.value {
                return Err(VerkleError::InvalidProof(
                    "Proofs have different root commitments".to_string()
                ));
            }
        }
        
        let mut multi_proof = MultiProof::new(
            self.proofs.iter().map(|p| p.key.clone()).collect(),
            self.proofs.iter().map(|p| p.value.clone()).collect(),
            root_commitment,
        );
        
        // Combine all proof nodes
        for proof in &self.proofs {
            for (path, commitment) in &proof.proof_nodes {
                multi_proof.add_commitment(path.clone(), commitment.clone());
            }
        }
        
        Ok(multi_proof)
    }
}

/// Proof builder for creating proofs
pub struct ProofBuilder {
    key: Vec<u8>,
    value: Option<Vec<u8>>,
    nodes: Vec<(Vec<u8>, Commitment)>,
}

impl ProofBuilder {
    pub fn new(key: Vec<u8>) -> Self {
        Self {
            key,
            value: None,
            nodes: Vec::new(),
        }
    }
    
    pub fn set_value(mut self, value: Vec<u8>) -> Self {
        self.value = Some(value);
        self
    }
    
    pub fn add_node(mut self, path: Vec<u8>, commitment: Commitment) -> Self {
        self.nodes.push((path, commitment));
        self
    }
    
    pub fn build(self, root_commitment: Commitment) -> VerkleProof {
        VerkleProof::new(
            self.key,
            self.value,
            self.nodes,
            root_commitment,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_proof_builder() {
        let key = vec![1, 2, 3];
        let value = vec![4, 5, 6];
        let root = Commitment::default();
        
        let proof = ProofBuilder::new(key.clone())
            .set_value(value.clone())
            .add_node(vec![1], Commitment::default())
            .add_node(vec![2], Commitment::default())
            .build(root.clone());
        
        assert_eq!(proof.key, key);
        assert_eq!(proof.value, Some(value));
        assert_eq!(proof.proof_nodes.len(), 2);
        assert!(proof.is_inclusion_proof());
    }
    
    #[test]
    fn test_proof_aggregator() {
        let root = Commitment::default();
        
        let proof1 = VerkleProof::new(
            vec![1],
            Some(vec![10]),
            vec![(vec![0], Commitment::default())],
            root.clone(),
        );
        
        let proof2 = VerkleProof::new(
            vec![2],
            Some(vec![20]),
            vec![(vec![0], Commitment::default())],
            root.clone(),
        );
        
        let mut aggregator = ProofAggregator::new();
        aggregator.add_proof(proof1);
        aggregator.add_proof(proof2);
        
        let multi_proof = aggregator.aggregate().unwrap();
        assert_eq!(multi_proof.size(), 2);
    }
}