use ethereum_types::{H256, U256};
use ethereum_crypto::{keccak256, secp256k1_recover};
use num_bigint::BigUint;
use sha2::{Sha256, Digest};
use ripemd::Ripemd160;

use crate::{EvmResult, EvmError};

/// Precompiled contract addresses
pub const ECRECOVER_ADDRESS: u64 = 0x01;
pub const SHA256_ADDRESS: u64 = 0x02;
pub const RIPEMD160_ADDRESS: u64 = 0x03;
pub const IDENTITY_ADDRESS: u64 = 0x04;
pub const MODEXP_ADDRESS: u64 = 0x05;
pub const ALT_BN128_ADD_ADDRESS: u64 = 0x06;
pub const ALT_BN128_MUL_ADDRESS: u64 = 0x07;
pub const ALT_BN128_PAIRING_ADDRESS: u64 = 0x08;
pub const BLAKE2F_ADDRESS: u64 = 0x09;

pub trait PrecompiledContract {
    fn execute(&self, input: &[u8], gas_limit: U256) -> EvmResult<(Vec<u8>, U256)>;
    fn required_gas(&self, input: &[u8]) -> U256;
}

/// ECRECOVER - Elliptic curve digital signature algorithm (ECDSA) public key recovery
pub struct EcRecover;

impl PrecompiledContract for EcRecover {
    fn execute(&self, input: &[u8], gas_limit: U256) -> EvmResult<(Vec<u8>, U256)> {
        let gas_cost = self.required_gas(input);
        if gas_cost > gas_limit {
            return Err(EvmError::OutOfGas);
        }
        
        // Input is 128 bytes:
        // [0-31]   hash
        // [32-63]  v
        // [64-95]  r
        // [96-127] s
        
        if input.len() < 128 {
            // Invalid input, return empty
            return Ok((vec![0u8; 32], gas_cost));
        }
        
        let hash = H256::from_slice(&input[0..32]);
        let v = U256::from_big_endian(&input[32..64]);
        let r = U256::from_big_endian(&input[64..96]);
        let s = U256::from_big_endian(&input[96..128]);
        
        // v should be 27 or 28 (or 0 or 1 for some implementations)
        let recovery_id = if v == U256::from(27) || v == U256::zero() {
            0
        } else if v == U256::from(28) || v == U256::one() {
            1
        } else {
            // Invalid v value
            return Ok((vec![0u8; 32], gas_cost));
        };
        
        // Convert r and s to bytes
        let mut r_bytes = [0u8; 32];
        let mut s_bytes = [0u8; 32];
        r.to_big_endian(&mut r_bytes);
        s.to_big_endian(&mut s_bytes);
        
        // Attempt recovery
        match secp256k1_recover(&hash, recovery_id, &r_bytes, &s_bytes) {
            Ok(pubkey) => {
                // Return the Ethereum address (last 20 bytes of keccak256(pubkey))
                let hash = keccak256(&pubkey);
                let mut result = vec![0u8; 12];
                result.extend_from_slice(&hash[12..32]);
                Ok((result, gas_cost))
            }
            Err(_) => {
                // Recovery failed, return empty
                Ok((vec![0u8; 32], gas_cost))
            }
        }
    }
    
    fn required_gas(&self, _input: &[u8]) -> U256 {
        U256::from(3000)
    }
}

/// SHA256 hash function
pub struct Sha256Hash;

impl PrecompiledContract for Sha256Hash {
    fn execute(&self, input: &[u8], gas_limit: U256) -> EvmResult<(Vec<u8>, U256)> {
        let gas_cost = self.required_gas(input);
        if gas_cost > gas_limit {
            return Err(EvmError::OutOfGas);
        }
        
        let mut hasher = Sha256::new();
        hasher.update(input);
        let result = hasher.finalize();
        
        Ok((result.to_vec(), gas_cost))
    }
    
    fn required_gas(&self, input: &[u8]) -> U256 {
        U256::from(60) + U256::from(12) * U256::from((input.len() + 31) / 32)
    }
}

/// RIPEMD160 hash function
pub struct Ripemd160Hash;

impl PrecompiledContract for Ripemd160Hash {
    fn execute(&self, input: &[u8], gas_limit: U256) -> EvmResult<(Vec<u8>, U256)> {
        let gas_cost = self.required_gas(input);
        if gas_cost > gas_limit {
            return Err(EvmError::OutOfGas);
        }
        
        let mut hasher = Ripemd160::new();
        hasher.update(input);
        let result = hasher.finalize();
        
        // Pad to 32 bytes
        let mut output = vec![0u8; 12];
        output.extend_from_slice(&result);
        
        Ok((output, gas_cost))
    }
    
    fn required_gas(&self, input: &[u8]) -> U256 {
        U256::from(600) + U256::from(120) * U256::from((input.len() + 31) / 32)
    }
}

/// Identity function - returns input as output
pub struct Identity;

impl PrecompiledContract for Identity {
    fn execute(&self, input: &[u8], gas_limit: U256) -> EvmResult<(Vec<u8>, U256)> {
        let gas_cost = self.required_gas(input);
        if gas_cost > gas_limit {
            return Err(EvmError::OutOfGas);
        }
        
        Ok((input.to_vec(), gas_cost))
    }
    
    fn required_gas(&self, input: &[u8]) -> U256 {
        U256::from(15) + U256::from(3) * U256::from((input.len() + 31) / 32)
    }
}

/// Modular exponentiation
pub struct ModExp;

impl PrecompiledContract for ModExp {
    fn execute(&self, input: &[u8], gas_limit: U256) -> EvmResult<(Vec<u8>, U256)> {
        // Extract lengths
        let base_len = if input.len() >= 32 {
            U256::from_big_endian(&input[0..32]).as_usize()
        } else {
            0
        };
        
        let exp_len = if input.len() >= 64 {
            U256::from_big_endian(&input[32..64]).as_usize()
        } else {
            0
        };
        
        let mod_len = if input.len() >= 96 {
            U256::from_big_endian(&input[64..96]).as_usize()
        } else {
            0
        };
        
        let gas_cost = self.calculate_gas_cost(base_len, exp_len, mod_len, input);
        if gas_cost > gas_limit {
            return Err(EvmError::OutOfGas);
        }
        
        // Extract base, exponent, and modulus
        let data_start = 96;
        
        let base = if input.len() > data_start && base_len > 0 {
            let end = std::cmp::min(data_start + base_len, input.len());
            BigUint::from_bytes_be(&input[data_start..end])
        } else {
            BigUint::from(0u32)
        };
        
        let exp_start = data_start + base_len;
        let exp = if input.len() > exp_start && exp_len > 0 {
            let end = std::cmp::min(exp_start + exp_len, input.len());
            BigUint::from_bytes_be(&input[exp_start..end])
        } else {
            BigUint::from(0u32)
        };
        
        let mod_start = exp_start + exp_len;
        let modulus = if input.len() > mod_start && mod_len > 0 {
            let end = std::cmp::min(mod_start + mod_len, input.len());
            BigUint::from_bytes_be(&input[mod_start..end])
        } else {
            BigUint::from(0u32)
        };
        
        // Perform modular exponentiation
        let result = if modulus == BigUint::from(0u32) {
            BigUint::from(0u32)
        } else {
            base.modpow(&exp, &modulus)
        };
        
        // Convert result to bytes with proper padding
        let mut result_bytes = result.to_bytes_be();
        if result_bytes.len() < mod_len {
            let mut padded = vec![0u8; mod_len - result_bytes.len()];
            padded.extend_from_slice(&result_bytes);
            result_bytes = padded;
        }
        
        Ok((result_bytes, gas_cost))
    }
    
    fn required_gas(&self, input: &[u8]) -> U256 {
        // Simplified gas calculation
        let base_len = if input.len() >= 32 {
            U256::from_big_endian(&input[0..32])
        } else {
            U256::zero()
        };
        
        let exp_len = if input.len() >= 64 {
            U256::from_big_endian(&input[32..64])
        } else {
            U256::zero()
        };
        
        let mod_len = if input.len() >= 96 {
            U256::from_big_endian(&input[64..96])
        } else {
            U256::zero()
        };
        
        self.calculate_gas_cost(
            base_len.as_usize(),
            exp_len.as_usize(),
            mod_len.as_usize(),
            input
        )
    }
}

impl ModExp {
    fn calculate_gas_cost(&self, base_len: usize, exp_len: usize, mod_len: usize, _input: &[u8]) -> U256 {
        // Simplified gas calculation for modexp
        let max_len = std::cmp::max(base_len, mod_len);
        let words = (max_len + 31) / 32;
        let exp_cost = if exp_len == 0 {
            U256::from(1)
        } else {
            U256::from(exp_len) * U256::from(8)
        };
        
        U256::from(words * words) * exp_cost / U256::from(3)
    }
}

/// BN128 Addition (alt_bn128_add)
pub struct Bn128Add;

impl PrecompiledContract for Bn128Add {
    fn execute(&self, input: &[u8], gas_limit: U256) -> EvmResult<(Vec<u8>, U256)> {
        let gas_cost = self.required_gas(input);
        if gas_cost > gas_limit {
            return Err(EvmError::OutOfGas);
        }
        
        // This is a placeholder implementation
        // Real implementation would perform elliptic curve point addition on BN128
        
        // Input should be 128 bytes (two points, each 64 bytes)
        // Output is 64 bytes (one point)
        
        Ok((vec![0u8; 64], gas_cost))
    }
    
    fn required_gas(&self, _input: &[u8]) -> U256 {
        U256::from(150) // Istanbul hard fork gas cost
    }
}

/// BN128 Multiplication (alt_bn128_mul)
pub struct Bn128Mul;

impl PrecompiledContract for Bn128Mul {
    fn execute(&self, input: &[u8], gas_limit: U256) -> EvmResult<(Vec<u8>, U256)> {
        let gas_cost = self.required_gas(input);
        if gas_cost > gas_limit {
            return Err(EvmError::OutOfGas);
        }
        
        // This is a placeholder implementation
        // Real implementation would perform elliptic curve scalar multiplication on BN128
        
        // Input should be 96 bytes (one point + scalar)
        // Output is 64 bytes (one point)
        
        Ok((vec![0u8; 64], gas_cost))
    }
    
    fn required_gas(&self, _input: &[u8]) -> U256 {
        U256::from(6000) // Istanbul hard fork gas cost
    }
}

/// BN128 Pairing check (alt_bn128_pairing)
pub struct Bn128Pairing;

impl PrecompiledContract for Bn128Pairing {
    fn execute(&self, input: &[u8], gas_limit: U256) -> EvmResult<(Vec<u8>, U256)> {
        let gas_cost = self.required_gas(input);
        if gas_cost > gas_limit {
            return Err(EvmError::OutOfGas);
        }
        
        // This is a placeholder implementation
        // Real implementation would perform pairing check on BN128
        
        // Return 1 for true, 0 for false
        Ok((vec![0u8; 32], gas_cost))
    }
    
    fn required_gas(&self, input: &[u8]) -> U256 {
        let k = input.len() / 192;
        U256::from(45000) + U256::from(34000) * U256::from(k) // Istanbul hard fork gas cost
    }
}

/// BLAKE2F compression function
pub struct Blake2f;

impl PrecompiledContract for Blake2f {
    fn execute(&self, input: &[u8], gas_limit: U256) -> EvmResult<(Vec<u8>, U256)> {
        let gas_cost = self.required_gas(input);
        if gas_cost > gas_limit {
            return Err(EvmError::OutOfGas);
        }
        
        if input.len() != 213 {
            return Err(EvmError::InvalidInput);
        }
        
        // This is a placeholder implementation
        // Real implementation would perform BLAKE2F compression
        
        Ok((vec![0u8; 64], gas_cost))
    }
    
    fn required_gas(&self, input: &[u8]) -> U256 {
        if input.len() >= 4 {
            let rounds = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
            U256::from(rounds)
        } else {
            U256::zero()
        }
    }
}

/// Get precompiled contract by address
pub fn get_precompiled(address: u64) -> Option<Box<dyn PrecompiledContract>> {
    match address {
        ECRECOVER_ADDRESS => Some(Box::new(EcRecover)),
        SHA256_ADDRESS => Some(Box::new(Sha256Hash)),
        RIPEMD160_ADDRESS => Some(Box::new(Ripemd160Hash)),
        IDENTITY_ADDRESS => Some(Box::new(Identity)),
        MODEXP_ADDRESS => Some(Box::new(ModExp)),
        ALT_BN128_ADD_ADDRESS => Some(Box::new(Bn128Add)),
        ALT_BN128_MUL_ADDRESS => Some(Box::new(Bn128Mul)),
        ALT_BN128_PAIRING_ADDRESS => Some(Box::new(Bn128Pairing)),
        BLAKE2F_ADDRESS => Some(Box::new(Blake2f)),
        _ => None,
    }
}

/// Check if an address is a precompiled contract
pub fn is_precompiled(address: u64) -> bool {
    address >= ECRECOVER_ADDRESS && address <= BLAKE2F_ADDRESS
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_identity_precompile() {
        let identity = Identity;
        let input = b"hello world";
        let gas_limit = U256::from(1000);
        
        let (output, gas_used) = identity.execute(input, gas_limit).unwrap();
        assert_eq!(output, input);
        assert_eq!(gas_used, identity.required_gas(input));
    }
    
    #[test]
    fn test_sha256_precompile() {
        let sha256 = Sha256Hash;
        let input = b"hello world";
        let gas_limit = U256::from(1000);
        
        let (output, gas_used) = sha256.execute(input, gas_limit).unwrap();
        assert_eq!(output.len(), 32);
        assert_eq!(gas_used, sha256.required_gas(input));
    }
    
    #[test]
    fn test_ripemd160_precompile() {
        let ripemd = Ripemd160Hash;
        let input = b"hello world";
        let gas_limit = U256::from(1000);
        
        let (output, gas_used) = ripemd.execute(input, gas_limit).unwrap();
        assert_eq!(output.len(), 32); // Padded to 32 bytes
        assert_eq!(gas_used, ripemd.required_gas(input));
    }
}