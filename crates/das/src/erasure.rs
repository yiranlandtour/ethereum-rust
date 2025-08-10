use reed_solomon_erasure::{galois_8::ReedSolomon, Error as ReedSolomonError};
use std::collections::HashMap;
use tracing::{debug, info};

use crate::{DASError, Result};

/// Erasure coded data
#[derive(Debug, Clone)]
pub struct CodedData {
    pub columns: Vec<Vec<u8>>,
    pub data_shards: usize,
    pub parity_shards: usize,
}

impl CodedData {
    pub fn new(data_shards: usize, parity_shards: usize) -> Self {
        Self {
            columns: Vec::new(),
            data_shards,
            parity_shards,
        }
    }
    
    pub fn total_shards(&self) -> usize {
        self.data_shards + self.parity_shards
    }
    
    pub fn get_column(&self, index: usize) -> Option<Vec<u8>> {
        self.columns.get(index).cloned()
    }
    
    pub fn set_column(&mut self, index: usize, data: Vec<u8>) {
        if index >= self.columns.len() {
            self.columns.resize(index + 1, Vec::new());
        }
        self.columns[index] = data;
    }
    
    pub fn is_complete(&self) -> bool {
        self.columns.len() == self.total_shards() &&
            self.columns.iter().all(|c| !c.is_empty())
    }
    
    pub fn available_shards(&self) -> Vec<usize> {
        self.columns
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_empty())
            .map(|(i, _)| i)
            .collect()
    }
    
    pub fn missing_shards(&self) -> Vec<usize> {
        (0..self.total_shards())
            .filter(|&i| i >= self.columns.len() || self.columns[i].is_empty())
            .collect()
    }
}

/// Erasure coding implementation for PeerDAS
pub struct ErasureCoding {
    reed_solomon: ReedSolomon,
    data_shards: usize,
    parity_shards: usize,
    shard_size: usize,
}

impl ErasureCoding {
    pub fn new(data_shards: usize, parity_shards: usize) -> Result<Self> {
        let reed_solomon = ReedSolomon::new(data_shards, parity_shards)
            .map_err(|e| DASError::InvalidData(format!("Failed to create Reed-Solomon: {}", e)))?;
        
        Ok(Self {
            reed_solomon,
            data_shards,
            parity_shards,
            shard_size: 0, // Will be determined during encoding
        })
    }
    
    /// Encode data into erasure coded columns
    pub fn encode(&self, data: &[u8]) -> Result<CodedData> {
        debug!("Encoding {} bytes into {} data shards and {} parity shards",
            data.len(), self.data_shards, self.parity_shards);
        
        // Calculate shard size (round up division)
        let shard_size = (data.len() + self.data_shards - 1) / self.data_shards;
        
        // Prepare data shards
        let mut shards = Vec::with_capacity(self.data_shards + self.parity_shards);
        
        for i in 0..self.data_shards {
            let start = i * shard_size;
            let end = ((i + 1) * shard_size).min(data.len());
            
            let mut shard = Vec::with_capacity(shard_size);
            if start < data.len() {
                shard.extend_from_slice(&data[start..end]);
            }
            
            // Pad the last shard if necessary
            shard.resize(shard_size, 0);
            shards.push(shard);
        }
        
        // Add parity shards (initially empty)
        for _ in 0..self.parity_shards {
            shards.push(vec![0u8; shard_size]);
        }
        
        // Encode to generate parity shards
        self.reed_solomon.encode(&mut shards)
            .map_err(|e| DASError::InvalidData(format!("Encoding failed: {}", e)))?;
        
        let mut coded = CodedData::new(self.data_shards, self.parity_shards);
        coded.columns = shards;
        
        info!("Successfully encoded {} bytes into {} columns of {} bytes each",
            data.len(), coded.total_shards(), shard_size);
        
        Ok(coded)
    }
    
    /// Reconstruct data from available columns
    pub fn reconstruct(&self, mut columns: HashMap<usize, Vec<u8>>) -> Result<Vec<u8>> {
        debug!("Reconstructing from {} available columns", columns.len());
        
        if columns.len() < self.data_shards {
            return Err(DASError::InsufficientSamples(
                columns.len(),
                self.data_shards,
            ));
        }
        
        // Determine shard size from available columns
        let shard_size = columns.values()
            .next()
            .ok_or_else(|| DASError::InvalidData("No columns available".to_string()))?
            .len();
        
        // Prepare shards array with proper ordering
        let mut shards = Vec::with_capacity(self.data_shards + self.parity_shards);
        let mut shard_present = Vec::with_capacity(self.data_shards + self.parity_shards);
        
        for i in 0..(self.data_shards + self.parity_shards) {
            if let Some(column) = columns.remove(&i) {
                if column.len() != shard_size {
                    return Err(DASError::InvalidData(
                        format!("Inconsistent shard size: expected {}, got {}", shard_size, column.len())
                    ));
                }
                shards.push(column);
                shard_present.push(true);
            } else {
                shards.push(vec![0u8; shard_size]);
                shard_present.push(false);
            }
        }
        
        // Reconstruct missing shards
        self.reed_solomon.reconstruct(&mut shards, &shard_present)
            .map_err(|e| DASError::ReconstructionFailed(format!("Reed-Solomon reconstruction failed: {}", e)))?;
        
        // Extract original data from data shards
        let mut reconstructed = Vec::with_capacity(self.data_shards * shard_size);
        for shard in shards.iter().take(self.data_shards) {
            reconstructed.extend_from_slice(shard);
        }
        
        // Remove padding (zeros at the end)
        while reconstructed.last() == Some(&0) {
            reconstructed.pop();
        }
        
        info!("Successfully reconstructed {} bytes", reconstructed.len());
        
        Ok(reconstructed)
    }
    
    /// Verify integrity of coded data
    pub fn verify(&self, coded: &CodedData) -> Result<bool> {
        if coded.columns.len() != self.data_shards + self.parity_shards {
            return Ok(false);
        }
        
        let mut shards = coded.columns.clone();
        
        match self.reed_solomon.verify(&mut shards) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    
    /// Repair missing columns in coded data
    pub fn repair(&self, coded: &mut CodedData) -> Result<()> {
        if coded.available_shards().len() < self.data_shards {
            return Err(DASError::InsufficientSamples(
                coded.available_shards().len(),
                self.data_shards,
            ));
        }
        
        let mut columns_map = HashMap::new();
        for (i, column) in coded.columns.iter().enumerate() {
            if !column.is_empty() {
                columns_map.insert(i, column.clone());
            }
        }
        
        // Reconstruct all data
        let reconstructed = self.reconstruct(columns_map)?;
        
        // Re-encode to get all columns
        let repaired = self.encode(&reconstructed)?;
        
        // Update missing columns
        for i in coded.missing_shards() {
            if let Some(column) = repaired.get_column(i) {
                coded.set_column(i, column);
            }
        }
        
        Ok(())
    }
}

/// Extended erasure coding with systematic layout
pub struct SystematicErasureCoding {
    inner: ErasureCoding,
}

impl SystematicErasureCoding {
    pub fn new(data_shards: usize, parity_shards: usize) -> Result<Self> {
        Ok(Self {
            inner: ErasureCoding::new(data_shards, parity_shards)?,
        })
    }
    
    /// Encode with systematic layout (data shards unchanged, parity shards appended)
    pub fn encode_systematic(&self, data: &[u8]) -> Result<CodedData> {
        self.inner.encode(data)
    }
    
    /// Check if we can reconstruct using only systematic (data) shards
    pub fn can_reconstruct_systematic(&self, available_indices: &[usize]) -> bool {
        let systematic_count = available_indices.iter()
            .filter(|&&i| i < self.inner.data_shards)
            .count();
        
        systematic_count == self.inner.data_shards
    }
    
    /// Reconstruct using only systematic shards if possible
    pub fn reconstruct_systematic(&self, columns: HashMap<usize, Vec<u8>>) -> Result<Vec<u8>> {
        // Check if all data shards are available
        let has_all_data = (0..self.inner.data_shards)
            .all(|i| columns.contains_key(&i));
        
        if has_all_data {
            // Fast path: just concatenate data shards
            let mut reconstructed = Vec::new();
            for i in 0..self.inner.data_shards {
                if let Some(shard) = columns.get(&i) {
                    reconstructed.extend_from_slice(shard);
                }
            }
            
            // Remove padding
            while reconstructed.last() == Some(&0) {
                reconstructed.pop();
            }
            
            Ok(reconstructed)
        } else {
            // Fall back to regular reconstruction
            self.inner.reconstruct(columns)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_erasure_coding_roundtrip() {
        let data = b"Hello, PeerDAS erasure coding!";
        let coder = ErasureCoding::new(2, 2).unwrap();
        
        // Encode
        let coded = coder.encode(data).unwrap();
        assert_eq!(coded.total_shards(), 4);
        
        // Reconstruct from all columns
        let mut columns = HashMap::new();
        for i in 0..4 {
            columns.insert(i, coded.columns[i].clone());
        }
        let reconstructed = coder.reconstruct(columns).unwrap();
        assert_eq!(&reconstructed, data);
    }
    
    #[test]
    fn test_erasure_coding_with_losses() {
        let data = b"Testing erasure coding with column losses";
        let coder = ErasureCoding::new(3, 2).unwrap();
        
        // Encode
        let coded = coder.encode(data).unwrap();
        
        // Simulate losing 2 columns (but keeping minimum required)
        let mut columns = HashMap::new();
        columns.insert(0, coded.columns[0].clone());
        columns.insert(2, coded.columns[2].clone());
        columns.insert(4, coded.columns[4].clone());
        
        let reconstructed = coder.reconstruct(columns).unwrap();
        assert_eq!(&reconstructed, data);
    }
    
    #[test]
    fn test_insufficient_columns() {
        let coder = ErasureCoding::new(3, 2).unwrap();
        
        let mut columns = HashMap::new();
        columns.insert(0, vec![1, 2, 3]);
        columns.insert(1, vec![4, 5, 6]);
        
        // Only 2 columns, need at least 3
        let result = coder.reconstruct(columns);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_systematic_fast_path() {
        let data = b"Systematic erasure coding test";
        let coder = SystematicErasureCoding::new(2, 1).unwrap();
        
        // Encode
        let coded = coder.encode_systematic(data).unwrap();
        
        // Reconstruct using only data shards (systematic)
        let mut columns = HashMap::new();
        columns.insert(0, coded.columns[0].clone());
        columns.insert(1, coded.columns[1].clone());
        
        assert!(coder.can_reconstruct_systematic(&[0, 1]));
        let reconstructed = coder.reconstruct_systematic(columns).unwrap();
        assert_eq!(&reconstructed, data);
    }
}