use ethereum_types::{H256, U256};
use crate::bls::{Bls12381, BlsError};
use crate::kzg::{KzgSettings, point_evaluation_precompile};

pub const BLS12_381_G1_ADD: u64 = 0x0a;
pub const BLS12_381_G1_MUL: u64 = 0x0b;
pub const BLS12_381_G1_MULTIEXP: u64 = 0x0c;
pub const BLS12_381_G2_ADD: u64 = 0x0d;
pub const BLS12_381_G2_MUL: u64 = 0x0e;
pub const BLS12_381_G2_MULTIEXP: u64 = 0x0f;
pub const BLS12_381_PAIRING: u64 = 0x10;
pub const BLS12_381_MAP_TO_G1: u64 = 0x11;
pub const BLS12_381_MAP_TO_G2: u64 = 0x12;
pub const KZG_POINT_EVALUATION: u64 = 0x0a;

pub trait PrecompiledContract {
    fn execute(&self, input: &[u8], gas_limit: U256) -> Result<(Vec<u8>, U256), String>;
    fn required_gas(&self, input: &[u8]) -> U256;
}

pub struct Bls12381Add;

impl PrecompiledContract for Bls12381Add {
    fn execute(&self, input: &[u8], gas_limit: U256) -> Result<(Vec<u8>, U256), String> {
        let gas_cost = self.required_gas(input);
        if gas_cost > gas_limit {
            return Err("Out of gas".to_string());
        }
        
        let mut padded = vec![0u8; 256];
        padded[..input.len().min(256)].copy_from_slice(&input[..input.len().min(256)]);
        
        let a = &padded[0..128];
        let b = &padded[128..256];
        
        let result = Bls12381::g1_add(a, b)
            .map_err(|e| format!("BLS error: {:?}", e))?;
        
        Ok((result, gas_cost))
    }
    
    fn required_gas(&self, _input: &[u8]) -> U256 {
        U256::from(500)
    }
}

pub struct Bls12381Mul;

impl PrecompiledContract for Bls12381Mul {
    fn execute(&self, input: &[u8], gas_limit: U256) -> Result<(Vec<u8>, U256), String> {
        let gas_cost = self.required_gas(input);
        if gas_cost > gas_limit {
            return Err("Out of gas".to_string());
        }
        
        let mut padded = vec![0u8; 160];
        padded[..input.len().min(160)].copy_from_slice(&input[..input.len().min(160)]);
        
        let point = &padded[0..128];
        let scalar = &padded[128..160];
        
        let result = Bls12381::g1_mul(point, scalar)
            .map_err(|e| format!("BLS error: {:?}", e))?;
        
        Ok((result, gas_cost))
    }
    
    fn required_gas(&self, _input: &[u8]) -> U256 {
        U256::from(12000)
    }
}

pub struct Bls12381Pairing;

impl PrecompiledContract for Bls12381Pairing {
    fn execute(&self, input: &[u8], gas_limit: U256) -> Result<(Vec<u8>, U256), String> {
        let gas_cost = self.required_gas(input);
        if gas_cost > gas_limit {
            return Err("Out of gas".to_string());
        }
        
        if input.len() % 384 != 0 {
            return Err("Invalid input length for pairing".to_string());
        }
        
        let n_pairs = input.len() / 384;
        let mut g1_points = Vec::with_capacity(n_pairs * 128);
        let mut g2_points = Vec::with_capacity(n_pairs * 256);
        
        for i in 0..n_pairs {
            let offset = i * 384;
            g1_points.extend_from_slice(&input[offset..offset + 128]);
            g2_points.extend_from_slice(&input[offset + 128..offset + 384]);
        }
        
        let valid = Bls12381::pairing(&g1_points, &g2_points)
            .map_err(|e| format!("BLS error: {:?}", e))?;
        
        let mut result = vec![0u8; 32];
        if valid {
            result[31] = 1;
        }
        
        Ok((result, gas_cost))
    }
    
    fn required_gas(&self, input: &[u8]) -> U256 {
        let n_pairs = input.len() / 384;
        U256::from(43000 + n_pairs * 65000)
    }
}

pub struct Bls12381MapToG1;

impl PrecompiledContract for Bls12381MapToG1 {
    fn execute(&self, input: &[u8], gas_limit: U256) -> Result<(Vec<u8>, U256), String> {
        let gas_cost = self.required_gas(input);
        if gas_cost > gas_limit {
            return Err("Out of gas".to_string());
        }
        
        let result = Bls12381::map_to_g1(input)
            .map_err(|e| format!("BLS error: {:?}", e))?;
        
        Ok((result, gas_cost))
    }
    
    fn required_gas(&self, _input: &[u8]) -> U256 {
        U256::from(5500)
    }
}

pub struct Bls12381MapToG2;

impl PrecompiledContract for Bls12381MapToG2 {
    fn execute(&self, input: &[u8], gas_limit: U256) -> Result<(Vec<u8>, U256), String> {
        let gas_cost = self.required_gas(input);
        if gas_cost > gas_limit {
            return Err("Out of gas".to_string());
        }
        
        let result = Bls12381::map_to_g2(input)
            .map_err(|e| format!("BLS error: {:?}", e))?;
        
        Ok((result, gas_cost))
    }
    
    fn required_gas(&self, _input: &[u8]) -> U256 {
        U256::from(75000)
    }
}

pub struct KzgPointEvaluation {
    settings: KzgSettings,
}

impl KzgPointEvaluation {
    pub fn new() -> Result<Self, String> {
        let settings = KzgSettings::load_trusted_setup()
            .map_err(|e| format!("Failed to load trusted setup: {:?}", e))?;
        
        Ok(Self { settings })
    }
}

impl PrecompiledContract for KzgPointEvaluation {
    fn execute(&self, input: &[u8], gas_limit: U256) -> Result<(Vec<u8>, U256), String> {
        let gas_cost = self.required_gas(input);
        if gas_cost > gas_limit {
            return Err("Out of gas".to_string());
        }
        
        let result = point_evaluation_precompile(input, &self.settings)
            .map_err(|e| format!("KZG error: {:?}", e))?;
        
        Ok((result, gas_cost))
    }
    
    fn required_gas(&self, _input: &[u8]) -> U256 {
        U256::from(50000)
    }
}

pub fn is_precompiled(address: u64) -> bool {
    matches!(
        address,
        0x01..=0x09 |
        BLS12_381_G1_ADD |
        BLS12_381_G1_MUL |
        BLS12_381_G1_MULTIEXP |
        BLS12_381_G2_ADD |
        BLS12_381_G2_MUL |
        BLS12_381_G2_MULTIEXP |
        BLS12_381_PAIRING |
        BLS12_381_MAP_TO_G1 |
        BLS12_381_MAP_TO_G2 |
        KZG_POINT_EVALUATION
    )
}

pub fn get_precompiled(address: u64) -> Option<Box<dyn PrecompiledContract>> {
    match address {
        BLS12_381_G1_ADD => Some(Box::new(Bls12381Add)),
        BLS12_381_G1_MUL => Some(Box::new(Bls12381Mul)),
        BLS12_381_PAIRING => Some(Box::new(Bls12381Pairing)),
        BLS12_381_MAP_TO_G1 => Some(Box::new(Bls12381MapToG1)),
        BLS12_381_MAP_TO_G2 => Some(Box::new(Bls12381MapToG2)),
        KZG_POINT_EVALUATION => KzgPointEvaluation::new().ok().map(|k| Box::new(k) as Box<dyn PrecompiledContract>),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bls_add_gas() {
        let bls_add = Bls12381Add;
        let gas = bls_add.required_gas(&[]);
        assert_eq!(gas, U256::from(500));
    }

    #[test]
    fn test_bls_mul_gas() {
        let bls_mul = Bls12381Mul;
        let gas = bls_mul.required_gas(&[]);
        assert_eq!(gas, U256::from(12000));
    }

    #[test]
    fn test_pairing_gas() {
        let pairing = Bls12381Pairing;
        let input = vec![0u8; 384 * 2];
        let gas = pairing.required_gas(&input);
        assert_eq!(gas, U256::from(43000 + 2 * 65000));
    }

    #[test]
    fn test_is_precompiled() {
        assert!(is_precompiled(BLS12_381_G1_ADD));
        assert!(is_precompiled(BLS12_381_PAIRING));
        assert!(is_precompiled(KZG_POINT_EVALUATION));
        assert!(!is_precompiled(0x100));
    }
}