use c_kzg::{Bytes32, Bytes48, KzgSettings as CKzgSettings, KzgProof as CKzgProof};
use ethereum_types::{H256, U256};
use thiserror::Error;
use std::sync::Arc;

#[derive(Debug, Error)]
pub enum KzgError {
    #[error("Invalid commitment")]
    InvalidCommitment,
    
    #[error("Invalid proof")]
    InvalidProof,
    
    #[error("Invalid blob")]
    InvalidBlob,
    
    #[error("Verification failed")]
    VerificationFailed,
    
    #[error("Setup not initialized")]
    SetupNotInitialized,
    
    #[error("C-KZG error: {0}")]
    CKzg(String),
}

pub const FIELD_ELEMENTS_PER_BLOB: usize = 4096;
pub const BYTES_PER_FIELD_ELEMENT: usize = 32;
pub const BYTES_PER_BLOB: usize = FIELD_ELEMENTS_PER_BLOB * BYTES_PER_FIELD_ELEMENT;
pub const VERSIONED_HASH_VERSION: u8 = 0x01;

#[derive(Clone)]
pub struct KzgSettings {
    inner: Arc<CKzgSettings>,
}

impl KzgSettings {
    pub fn load_trusted_setup() -> Result<Self, KzgError> {
        let trusted_setup = include_bytes!("../trusted_setup/mainnet.txt");
        
        let settings = CKzgSettings::load_trusted_setup_file(trusted_setup)
            .map_err(|e| KzgError::CKzg(e.to_string()))?;
        
        Ok(Self {
            inner: Arc::new(settings),
        })
    }

    pub fn load_trusted_setup_from_file(path: &str) -> Result<Self, KzgError> {
        let settings = CKzgSettings::load_trusted_setup_file_path(path)
            .map_err(|e| KzgError::CKzg(e.to_string()))?;
        
        Ok(Self {
            inner: Arc::new(settings),
        })
    }

    pub fn blob_to_kzg_commitment(&self, blob: &Blob) -> Result<KzgCommitment, KzgError> {
        let commitment = self.inner
            .blob_to_kzg_commitment(&blob.inner)
            .map_err(|e| KzgError::CKzg(e.to_string()))?;
        
        Ok(KzgCommitment::from_bytes(commitment.to_bytes()))
    }

    pub fn compute_kzg_proof(
        &self,
        blob: &Blob,
        z: &H256,
    ) -> Result<(KzgProof, H256), KzgError> {
        let z_bytes = Bytes32::from(z.as_bytes());
        
        let (proof, y) = self.inner
            .compute_kzg_proof(&blob.inner, &z_bytes)
            .map_err(|e| KzgError::CKzg(e.to_string()))?;
        
        Ok((
            KzgProof::from_bytes(proof.to_bytes()),
            H256::from_slice(&y.to_bytes()),
        ))
    }

    pub fn verify_kzg_proof(
        &self,
        commitment: &KzgCommitment,
        z: &H256,
        y: &H256,
        proof: &KzgProof,
    ) -> Result<bool, KzgError> {
        let commitment_bytes = Bytes48::from(commitment.as_bytes());
        let z_bytes = Bytes32::from(z.as_bytes());
        let y_bytes = Bytes32::from(y.as_bytes());
        let proof_bytes = Bytes48::from(proof.as_bytes());
        
        self.inner
            .verify_kzg_proof(&commitment_bytes, &z_bytes, &y_bytes, &proof_bytes)
            .map_err(|e| KzgError::CKzg(e.to_string()))
    }

    pub fn compute_blob_kzg_proof(
        &self,
        blob: &Blob,
        commitment: &KzgCommitment,
    ) -> Result<KzgProof, KzgError> {
        let commitment_bytes = Bytes48::from(commitment.as_bytes());
        
        let proof = self.inner
            .compute_blob_kzg_proof(&blob.inner, &commitment_bytes)
            .map_err(|e| KzgError::CKzg(e.to_string()))?;
        
        Ok(KzgProof::from_bytes(proof.to_bytes()))
    }

    pub fn verify_blob_kzg_proof(
        &self,
        blob: &Blob,
        commitment: &KzgCommitment,
        proof: &KzgProof,
    ) -> Result<bool, KzgError> {
        let commitment_bytes = Bytes48::from(commitment.as_bytes());
        let proof_bytes = Bytes48::from(proof.as_bytes());
        
        self.inner
            .verify_blob_kzg_proof(&blob.inner, &commitment_bytes, &proof_bytes)
            .map_err(|e| KzgError::CKzg(e.to_string()))
    }

    pub fn verify_blob_kzg_proof_batch(
        &self,
        blobs: &[Blob],
        commitments: &[KzgCommitment],
        proofs: &[KzgProof],
    ) -> Result<bool, KzgError> {
        if blobs.len() != commitments.len() || blobs.len() != proofs.len() {
            return Err(KzgError::InvalidBlob);
        }
        
        let blobs: Vec<_> = blobs.iter().map(|b| b.inner.clone()).collect();
        let commitments: Vec<_> = commitments
            .iter()
            .map(|c| Bytes48::from(c.as_bytes()))
            .collect();
        let proofs: Vec<_> = proofs
            .iter()
            .map(|p| Bytes48::from(p.as_bytes()))
            .collect();
        
        self.inner
            .verify_blob_kzg_proof_batch(&blobs, &commitments, &proofs)
            .map_err(|e| KzgError::CKzg(e.to_string()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KzgCommitment {
    bytes: [u8; 48],
}

impl KzgCommitment {
    pub fn from_bytes(bytes: [u8; 48]) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8; 48] {
        &self.bytes
    }

    pub fn to_versioned_hash(&self) -> H256 {
        use ethereum_crypto::keccak256;
        
        let hash = keccak256(&self.bytes);
        let mut versioned = [0u8; 32];
        versioned[0] = VERSIONED_HASH_VERSION;
        versioned[1..].copy_from_slice(&hash[1..]);
        
        H256::from(versioned)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KzgProof {
    bytes: [u8; 48],
}

impl KzgProof {
    pub fn from_bytes(bytes: [u8; 48]) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8; 48] {
        &self.bytes
    }
}

pub struct Blob {
    inner: c_kzg::Blob,
}

impl Blob {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, KzgError> {
        if bytes.len() != BYTES_PER_BLOB {
            return Err(KzgError::InvalidBlob);
        }
        
        let mut blob_bytes = [0u8; BYTES_PER_BLOB];
        blob_bytes.copy_from_slice(bytes);
        
        let blob = c_kzg::Blob::from_bytes(&blob_bytes)
            .map_err(|e| KzgError::CKzg(e.to_string()))?;
        
        Ok(Self { inner: blob })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.inner.to_bytes().to_vec()
    }
}

pub fn point_evaluation_precompile(
    input: &[u8],
    kzg_settings: &KzgSettings,
) -> Result<Vec<u8>, KzgError> {
    if input.len() != 192 {
        return Err(KzgError::InvalidBlob);
    }
    
    let versioned_hash = H256::from_slice(&input[0..32]);
    let z = H256::from_slice(&input[32..64]);
    let y = H256::from_slice(&input[64..96]);
    
    let mut commitment_bytes = [0u8; 48];
    commitment_bytes.copy_from_slice(&input[96..144]);
    let commitment = KzgCommitment::from_bytes(commitment_bytes);
    
    let mut proof_bytes = [0u8; 48];
    proof_bytes.copy_from_slice(&input[144..192]);
    let proof = KzgProof::from_bytes(proof_bytes);
    
    if commitment.to_versioned_hash() != versioned_hash {
        return Err(KzgError::InvalidCommitment);
    }
    
    let valid = kzg_settings.verify_kzg_proof(&commitment, &z, &y, &proof)?;
    
    if !valid {
        return Err(KzgError::VerificationFailed);
    }
    
    let mut output = vec![0u8; 64];
    output[..32].copy_from_slice(&U256::from(FIELD_ELEMENTS_PER_BLOB).to_be_bytes::<32>());
    output[32..64].copy_from_slice(&U256::from(BYTES_PER_FIELD_ELEMENT).to_be_bytes::<32>());
    
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_versioned_hash() {
        let commitment_bytes = [0x42u8; 48];
        let commitment = KzgCommitment::from_bytes(commitment_bytes);
        let versioned_hash = commitment.to_versioned_hash();
        
        assert_eq!(versioned_hash.as_bytes()[0], VERSIONED_HASH_VERSION);
    }

    #[test]
    fn test_blob_size() {
        let bytes = vec![0u8; BYTES_PER_BLOB];
        let blob = Blob::from_bytes(&bytes).unwrap();
        assert_eq!(blob.to_bytes().len(), BYTES_PER_BLOB);
        
        let invalid_bytes = vec![0u8; BYTES_PER_BLOB - 1];
        assert!(Blob::from_bytes(&invalid_bytes).is_err());
    }
}