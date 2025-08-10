use bls12_381::{G1Affine, G1Projective, G2Affine, G2Projective, Gt, Scalar};
use ff::Field;
use group::{Curve, Group};
use pairing::Engine;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlsError {
    #[error("Invalid point encoding")]
    InvalidPoint,
    
    #[error("Invalid scalar")]
    InvalidScalar,
    
    #[error("Point not on curve")]
    PointNotOnCurve,
    
    #[error("Pairing check failed")]
    PairingCheckFailed,
    
    #[error("Invalid input length")]
    InvalidInputLength,
}

pub struct Bls12381;

impl Bls12381 {
    pub fn g1_add(a: &[u8], b: &[u8]) -> Result<Vec<u8>, BlsError> {
        let point_a = Self::decode_g1_point(a)?;
        let point_b = Self::decode_g1_point(b)?;
        
        let result = point_a + point_b;
        Ok(Self::encode_g1_point(&result))
    }

    pub fn g1_mul(point: &[u8], scalar: &[u8]) -> Result<Vec<u8>, BlsError> {
        let p = Self::decode_g1_point(point)?;
        let s = Self::decode_scalar(scalar)?;
        
        let result = p * s;
        Ok(Self::encode_g1_point(&result))
    }

    pub fn g1_multiexp(points: &[u8], scalars: &[u8]) -> Result<Vec<u8>, BlsError> {
        if points.len() % 128 != 0 || scalars.len() % 32 != 0 {
            return Err(BlsError::InvalidInputLength);
        }
        
        let num_pairs = points.len() / 128;
        if num_pairs != scalars.len() / 32 {
            return Err(BlsError::InvalidInputLength);
        }
        
        let mut result = G1Projective::identity();
        
        for i in 0..num_pairs {
            let point_bytes = &points[i * 128..(i + 1) * 128];
            let scalar_bytes = &scalars[i * 32..(i + 1) * 32];
            
            let point = Self::decode_g1_point(point_bytes)?;
            let scalar = Self::decode_scalar(scalar_bytes)?;
            
            result += point * scalar;
        }
        
        Ok(Self::encode_g1_point(&result))
    }

    pub fn g2_add(a: &[u8], b: &[u8]) -> Result<Vec<u8>, BlsError> {
        let point_a = Self::decode_g2_point(a)?;
        let point_b = Self::decode_g2_point(b)?;
        
        let result = point_a + point_b;
        Ok(Self::encode_g2_point(&result))
    }

    pub fn g2_mul(point: &[u8], scalar: &[u8]) -> Result<Vec<u8>, BlsError> {
        let p = Self::decode_g2_point(point)?;
        let s = Self::decode_scalar(scalar)?;
        
        let result = p * s;
        Ok(Self::encode_g2_point(&result))
    }

    pub fn g2_multiexp(points: &[u8], scalars: &[u8]) -> Result<Vec<u8>, BlsError> {
        if points.len() % 256 != 0 || scalars.len() % 32 != 0 {
            return Err(BlsError::InvalidInputLength);
        }
        
        let num_pairs = points.len() / 256;
        if num_pairs != scalars.len() / 32 {
            return Err(BlsError::InvalidInputLength);
        }
        
        let mut result = G2Projective::identity();
        
        for i in 0..num_pairs {
            let point_bytes = &points[i * 256..(i + 1) * 256];
            let scalar_bytes = &scalars[i * 32..(i + 1) * 32];
            
            let point = Self::decode_g2_point(point_bytes)?;
            let scalar = Self::decode_scalar(scalar_bytes)?;
            
            result += point * scalar;
        }
        
        Ok(Self::encode_g2_point(&result))
    }

    pub fn pairing(g1_points: &[u8], g2_points: &[u8]) -> Result<bool, BlsError> {
        if g1_points.len() % 128 != 0 || g2_points.len() % 256 != 0 {
            return Err(BlsError::InvalidInputLength);
        }
        
        let n_g1 = g1_points.len() / 128;
        let n_g2 = g2_points.len() / 256;
        
        if n_g1 != n_g2 {
            return Err(BlsError::InvalidInputLength);
        }
        
        let mut acc = Gt::identity();
        
        for i in 0..n_g1 {
            let g1_bytes = &g1_points[i * 128..(i + 1) * 128];
            let g2_bytes = &g2_points[i * 256..(i + 1) * 256];
            
            let g1 = Self::decode_g1_point(g1_bytes)?;
            let g2 = Self::decode_g2_point(g2_bytes)?;
            
            acc += bls12_381::pairing(&g1.to_affine(), &g2.to_affine());
        }
        
        Ok(acc == Gt::identity())
    }

    pub fn map_to_g1(msg: &[u8]) -> Result<Vec<u8>, BlsError> {
        use sha2::{Sha256, Digest};
        
        let mut hasher = Sha256::new();
        hasher.update(b"BLS12381G1_XMD:SHA-256_SSWU_RO_");
        hasher.update(msg);
        let hash = hasher.finalize();
        
        let mut scalar_bytes = [0u8; 32];
        scalar_bytes.copy_from_slice(&hash);
        
        let scalar = Scalar::from_bytes(&scalar_bytes).unwrap_or(Scalar::zero());
        let point = G1Projective::generator() * scalar;
        
        Ok(Self::encode_g1_point(&point))
    }

    pub fn map_to_g2(msg: &[u8]) -> Result<Vec<u8>, BlsError> {
        use sha2::{Sha256, Digest};
        
        let mut hasher = Sha256::new();
        hasher.update(b"BLS12381G2_XMD:SHA-256_SSWU_RO_");
        hasher.update(msg);
        let hash = hasher.finalize();
        
        let mut scalar_bytes = [0u8; 32];
        scalar_bytes.copy_from_slice(&hash);
        
        let scalar = Scalar::from_bytes(&scalar_bytes).unwrap_or(Scalar::zero());
        let point = G2Projective::generator() * scalar;
        
        Ok(Self::encode_g2_point(&point))
    }

    fn decode_g1_point(data: &[u8]) -> Result<G1Projective, BlsError> {
        if data.len() != 128 {
            return Err(BlsError::InvalidInputLength);
        }
        
        let mut uncompressed = [0u8; 96];
        uncompressed[..48].copy_from_slice(&data[16..64]);
        uncompressed[48..].copy_from_slice(&data[80..128]);
        
        let affine = G1Affine::from_uncompressed(&uncompressed)
            .ok_or(BlsError::InvalidPoint)?;
        
        if !affine.is_on_curve() {
            return Err(BlsError::PointNotOnCurve);
        }
        
        Ok(G1Projective::from(affine))
    }

    fn decode_g2_point(data: &[u8]) -> Result<G2Projective, BlsError> {
        if data.len() != 256 {
            return Err(BlsError::InvalidInputLength);
        }
        
        let mut uncompressed = [0u8; 192];
        uncompressed[..48].copy_from_slice(&data[16..64]);
        uncompressed[48..96].copy_from_slice(&data[80..128]);
        uncompressed[96..144].copy_from_slice(&data[144..192]);
        uncompressed[144..].copy_from_slice(&data[208..256]);
        
        let affine = G2Affine::from_uncompressed(&uncompressed)
            .ok_or(BlsError::InvalidPoint)?;
        
        if !affine.is_on_curve() {
            return Err(BlsError::PointNotOnCurve);
        }
        
        Ok(G2Projective::from(affine))
    }

    fn decode_scalar(data: &[u8]) -> Result<Scalar, BlsError> {
        if data.len() != 32 {
            return Err(BlsError::InvalidInputLength);
        }
        
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(data);
        bytes.reverse();
        
        Scalar::from_bytes(&bytes).ok_or(BlsError::InvalidScalar)
    }

    fn encode_g1_point(point: &G1Projective) -> Vec<u8> {
        let affine = point.to_affine();
        let uncompressed = affine.to_uncompressed();
        
        let mut result = vec![0u8; 128];
        result[16..64].copy_from_slice(&uncompressed[..48]);
        result[80..128].copy_from_slice(&uncompressed[48..]);
        
        result
    }

    fn encode_g2_point(point: &G2Projective) -> Vec<u8> {
        let affine = point.to_affine();
        let uncompressed = affine.to_uncompressed();
        
        let mut result = vec![0u8; 256];
        result[16..64].copy_from_slice(&uncompressed[..48]);
        result[80..128].copy_from_slice(&uncompressed[48..96]);
        result[144..192].copy_from_slice(&uncompressed[96..144]);
        result[208..256].copy_from_slice(&uncompressed[144..]);
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_g1_add() {
        let g1_gen = G1Projective::generator();
        let g1_gen_bytes = Bls12381::encode_g1_point(&g1_gen);
        
        let result = Bls12381::g1_add(&g1_gen_bytes, &g1_gen_bytes).unwrap();
        let expected = Bls12381::encode_g1_point(&(g1_gen + g1_gen));
        
        assert_eq!(result, expected);
    }

    #[test]
    fn test_g1_mul() {
        let g1_gen = G1Projective::generator();
        let g1_gen_bytes = Bls12381::encode_g1_point(&g1_gen);
        
        let scalar = Scalar::from(2u64);
        let mut scalar_bytes = [0u8; 32];
        scalar_bytes[31] = 2;
        
        let result = Bls12381::g1_mul(&g1_gen_bytes, &scalar_bytes).unwrap();
        let expected = Bls12381::encode_g1_point(&(g1_gen * scalar));
        
        assert_eq!(result, expected);
    }

    #[test]
    fn test_pairing() {
        let g1 = G1Projective::generator();
        let g2 = G2Projective::generator();
        
        let g1_bytes = Bls12381::encode_g1_point(&g1);
        let g2_bytes = Bls12381::encode_g2_point(&g2);
        
        let neg_g1 = -g1;
        let neg_g1_bytes = Bls12381::encode_g1_point(&neg_g1);
        
        let mut g1_points = Vec::new();
        g1_points.extend_from_slice(&g1_bytes);
        g1_points.extend_from_slice(&neg_g1_bytes);
        
        let mut g2_points = Vec::new();
        g2_points.extend_from_slice(&g2_bytes);
        g2_points.extend_from_slice(&g2_bytes);
        
        let result = Bls12381::pairing(&g1_points, &g2_points).unwrap();
        assert!(result);
    }
}