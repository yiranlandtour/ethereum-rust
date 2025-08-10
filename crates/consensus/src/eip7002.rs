use ethereum_types::{Address, H256, U256};
use ethereum_crypto::{keccak256, verify_signature};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// EIP-7002: Execution layer triggerable exits
/// Allows validators to trigger exits through the execution layer

// System-level withdrawal request precompile address
pub const WITHDRAWAL_REQUEST_PRECOMPILE_ADDRESS: Address = Address([
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x0A, 0xAA, 0xAA,
]);

// Constants
pub const EXCESS_WITHDRAWAL_REQUESTS_STORAGE_SLOT: U256 = U256([0, 0, 0, 0]);
pub const WITHDRAWAL_REQUEST_COUNT_STORAGE_SLOT: U256 = U256([1, 0, 0, 0]);
pub const WITHDRAWAL_REQUEST_QUEUE_HEAD_STORAGE_SLOT: U256 = U256([2, 0, 0, 0]);
pub const WITHDRAWAL_REQUEST_QUEUE_TAIL_STORAGE_SLOT: U256 = U256([3, 0, 0, 0]);
pub const WITHDRAWAL_REQUEST_QUEUE_STORAGE_OFFSET: U256 = U256([4, 0, 0, 0]);

pub const MAX_WITHDRAWAL_REQUESTS_PER_BLOCK: u64 = 16;
pub const TARGET_WITHDRAWAL_REQUESTS_PER_BLOCK: u64 = 2;
pub const MIN_WITHDRAWAL_REQUEST_FEE: u64 = 1;
pub const WITHDRAWAL_REQUEST_FEE_UPDATE_FRACTION: u64 = 17;

#[derive(Debug, Error)]
pub enum Eip7002Error {
    #[error("Invalid validator pubkey")]
    InvalidPubkey,
    
    #[error("Invalid signature")]
    InvalidSignature,
    
    #[error("Insufficient fee: required {0}, provided {1}")]
    InsufficientFee(U256, U256),
    
    #[error("Too many withdrawal requests")]
    TooManyRequests,
    
    #[error("Validator not found")]
    ValidatorNotFound,
    
    #[error("Validator already exited")]
    AlreadyExited,
    
    #[error("Invalid source address")]
    InvalidSourceAddress,
}

pub type Result<T> = std::result::Result<T, Eip7002Error>;

/// Withdrawal request submitted to the execution layer
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WithdrawalRequest {
    pub source_address: Address,
    pub validator_pubkey: [u8; 48],
    pub amount: U256,
}

impl WithdrawalRequest {
    pub fn new(
        source_address: Address,
        validator_pubkey: [u8; 48],
        amount: U256,
    ) -> Self {
        Self {
            source_address,
            validator_pubkey,
            amount,
        }
    }
    
    pub fn encode(&self) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(20 + 48 + 32);
        encoded.extend_from_slice(self.source_address.as_bytes());
        encoded.extend_from_slice(&self.validator_pubkey);
        encoded.extend_from_slice(&self.amount.to_be_bytes::<32>());
        encoded
    }
    
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < 100 {
            return Err(Eip7002Error::InvalidPubkey);
        }
        
        let mut source_address = [0u8; 20];
        source_address.copy_from_slice(&data[0..20]);
        
        let mut validator_pubkey = [0u8; 48];
        validator_pubkey.copy_from_slice(&data[20..68]);
        
        let mut amount_bytes = [0u8; 32];
        amount_bytes.copy_from_slice(&data[68..100]);
        
        Ok(Self {
            source_address: Address::from(source_address),
            validator_pubkey,
            amount: U256::from_big_endian(&amount_bytes),
        })
    }
    
    pub fn hash(&self) -> H256 {
        keccak256(&self.encode())
    }
}

/// System contract for managing withdrawal requests
pub struct WithdrawalRequestContract {
    requests: Vec<WithdrawalRequest>,
    excess_requests: u64,
    current_fee: U256,
}

impl WithdrawalRequestContract {
    pub fn new() -> Self {
        Self {
            requests: Vec::new(),
            excess_requests: 0,
            current_fee: U256::from(MIN_WITHDRAWAL_REQUEST_FEE),
        }
    }
    
    pub fn add_request(
        &mut self,
        request: WithdrawalRequest,
        fee_provided: U256,
    ) -> Result<()> {
        // Check if fee is sufficient
        if fee_provided < self.current_fee {
            return Err(Eip7002Error::InsufficientFee(
                self.current_fee,
                fee_provided,
            ));
        }
        
        // Check if we haven't exceeded max requests
        if self.requests.len() >= MAX_WITHDRAWAL_REQUESTS_PER_BLOCK as usize {
            return Err(Eip7002Error::TooManyRequests);
        }
        
        self.requests.push(request);
        Ok(())
    }
    
    pub fn get_requests(&self) -> &[WithdrawalRequest] {
        &self.requests
    }
    
    pub fn process_requests(&mut self) -> Vec<WithdrawalRequest> {
        let requests = self.requests.clone();
        self.requests.clear();
        
        // Update excess requests
        self.update_excess_requests(requests.len() as u64);
        
        // Update fee
        self.update_fee();
        
        requests
    }
    
    fn update_excess_requests(&mut self, count: u64) {
        let target = TARGET_WITHDRAWAL_REQUESTS_PER_BLOCK;
        
        if self.excess_requests + count > target {
            self.excess_requests = self.excess_requests + count - target;
        } else {
            self.excess_requests = 0;
        }
    }
    
    fn update_fee(&mut self) {
        self.current_fee = calculate_withdrawal_request_fee(self.excess_requests);
    }
    
    pub fn get_current_fee(&self) -> U256 {
        self.current_fee
    }
    
    pub fn get_excess_requests(&self) -> u64 {
        self.excess_requests
    }
}

/// Calculate the fee for withdrawal requests based on excess
pub fn calculate_withdrawal_request_fee(excess_requests: u64) -> U256 {
    fake_exponential(
        U256::from(MIN_WITHDRAWAL_REQUEST_FEE),
        U256::from(excess_requests),
        U256::from(WITHDRAWAL_REQUEST_FEE_UPDATE_FRACTION),
    )
}

/// Fake exponential function for fee calculation
fn fake_exponential(factor: U256, numerator: U256, denominator: U256) -> U256 {
    let mut output = U256::zero();
    let mut accum = factor * denominator;
    
    let mut i = U256::one();
    while accum > U256::zero() {
        output += accum;
        accum = (accum * numerator) / (denominator * i);
        i += U256::one();
        
        if i > U256::from(256) {
            break;
        }
    }
    
    output / denominator
}

/// Validator exit queue manager
pub struct ExitQueueManager {
    pending_exits: Vec<ValidatorExit>,
    processed_exits: Vec<ValidatorExit>,
    max_exits_per_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatorExit {
    pub validator_index: u64,
    pub validator_pubkey: [u8; 48],
    pub exit_epoch: u64,
    pub withdrawable_epoch: u64,
    pub initiated_by: ExitInitiator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExitInitiator {
    Validator,           // Voluntary exit
    ExecutionLayer,      // EIP-7002 exit
    Protocol,           // Slashing or other protocol-initiated
}

impl ExitQueueManager {
    pub fn new(max_exits_per_epoch: u64) -> Self {
        Self {
            pending_exits: Vec::new(),
            processed_exits: Vec::new(),
            max_exits_per_epoch,
        }
    }
    
    pub fn add_exit_request(
        &mut self,
        validator_index: u64,
        validator_pubkey: [u8; 48],
        current_epoch: u64,
        initiated_by: ExitInitiator,
    ) -> Result<()> {
        // Check if validator already has pending exit
        if self.pending_exits.iter().any(|e| e.validator_index == validator_index) {
            return Err(Eip7002Error::AlreadyExited);
        }
        
        let exit_epoch = self.calculate_exit_epoch(current_epoch);
        let withdrawable_epoch = exit_epoch + 256; // MIN_VALIDATOR_WITHDRAWABILITY_DELAY
        
        let exit = ValidatorExit {
            validator_index,
            validator_pubkey,
            exit_epoch,
            withdrawable_epoch,
            initiated_by,
        };
        
        self.pending_exits.push(exit);
        self.pending_exits.sort_by_key(|e| e.exit_epoch);
        
        Ok(())
    }
    
    fn calculate_exit_epoch(&self, current_epoch: u64) -> u64 {
        // Find the earliest epoch where we can schedule this exit
        let mut exit_epoch = current_epoch + 4; // MIN_EXIT_DELAY
        
        // Count exits already scheduled for each epoch
        loop {
            let exits_in_epoch = self.pending_exits
                .iter()
                .filter(|e| e.exit_epoch == exit_epoch)
                .count() as u64;
            
            if exits_in_epoch < self.max_exits_per_epoch {
                break;
            }
            
            exit_epoch += 1;
        }
        
        exit_epoch
    }
    
    pub fn process_epoch(&mut self, current_epoch: u64) -> Vec<ValidatorExit> {
        let mut processed = Vec::new();
        
        self.pending_exits.retain(|exit| {
            if exit.exit_epoch <= current_epoch {
                processed.push(exit.clone());
                self.processed_exits.push(exit.clone());
                false
            } else {
                true
            }
        });
        
        processed
    }
    
    pub fn get_pending_exits(&self) -> &[ValidatorExit] {
        &self.pending_exits
    }
    
    pub fn get_exit_queue_length(&self) -> usize {
        self.pending_exits.len()
    }
}

/// Precompiled contract for withdrawal requests
pub struct WithdrawalRequestPrecompile;

impl WithdrawalRequestPrecompile {
    pub fn execute(input: &[u8], value: U256) -> Result<Vec<u8>> {
        // Input format: 20 bytes source address + 48 bytes validator pubkey
        if input.len() != 68 {
            return Err(Eip7002Error::InvalidPubkey);
        }
        
        let mut source_address = [0u8; 20];
        source_address.copy_from_slice(&input[0..20]);
        
        let mut validator_pubkey = [0u8; 48];
        validator_pubkey.copy_from_slice(&input[20..68]);
        
        let request = WithdrawalRequest::new(
            Address::from(source_address),
            validator_pubkey,
            value,
        );
        
        // Return encoded request
        Ok(request.encode())
    }
    
    pub fn required_gas(input: &[u8]) -> U256 {
        // Base cost + per-byte cost
        U256::from(10000) + U256::from(input.len() * 10)
    }
}

/// Storage layout for withdrawal request queue
pub struct WithdrawalRequestQueue {
    head: u64,
    tail: u64,
    count: u64,
    excess: u64,
    requests: Vec<WithdrawalRequest>,
}

impl WithdrawalRequestQueue {
    pub fn new() -> Self {
        Self {
            head: 0,
            tail: 0,
            count: 0,
            excess: 0,
            requests: Vec::new(),
        }
    }
    
    pub fn enqueue(&mut self, request: WithdrawalRequest) -> Result<()> {
        if self.count >= MAX_WITHDRAWAL_REQUESTS_PER_BLOCK {
            return Err(Eip7002Error::TooManyRequests);
        }
        
        self.requests.push(request);
        self.tail += 1;
        self.count += 1;
        
        Ok(())
    }
    
    pub fn dequeue(&mut self) -> Option<WithdrawalRequest> {
        if self.count == 0 {
            return None;
        }
        
        let request = self.requests.remove(0);
        self.head += 1;
        self.count -= 1;
        
        Some(request)
    }
    
    pub fn dequeue_up_to(&mut self, max_count: u64) -> Vec<WithdrawalRequest> {
        let mut result = Vec::new();
        let count = self.count.min(max_count);
        
        for _ in 0..count {
            if let Some(request) = self.dequeue() {
                result.push(request);
            }
        }
        
        // Update excess
        if result.len() as u64 > TARGET_WITHDRAWAL_REQUESTS_PER_BLOCK {
            self.excess += result.len() as u64 - TARGET_WITHDRAWAL_REQUESTS_PER_BLOCK;
        } else if self.excess > 0 {
            self.excess = self.excess.saturating_sub(
                TARGET_WITHDRAWAL_REQUESTS_PER_BLOCK - result.len() as u64
            );
        }
        
        result
    }
    
    pub fn get_fee(&self) -> U256 {
        calculate_withdrawal_request_fee(self.excess)
    }
    
    pub fn size(&self) -> u64 {
        self.count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_withdrawal_request_encoding() {
        let request = WithdrawalRequest::new(
            Address::from([1u8; 20]),
            [2u8; 48],
            U256::from(32_000_000_000u64),
        );
        
        let encoded = request.encode();
        assert_eq!(encoded.len(), 100);
        
        let decoded = WithdrawalRequest::decode(&encoded).unwrap();
        assert_eq!(decoded, request);
    }
    
    #[test]
    fn test_withdrawal_request_fee() {
        let fee0 = calculate_withdrawal_request_fee(0);
        assert_eq!(fee0, U256::from(MIN_WITHDRAWAL_REQUEST_FEE));
        
        let fee10 = calculate_withdrawal_request_fee(10);
        assert!(fee10 > fee0);
        
        let fee100 = calculate_withdrawal_request_fee(100);
        assert!(fee100 > fee10);
    }
    
    #[test]
    fn test_exit_queue_manager() {
        let mut manager = ExitQueueManager::new(4);
        
        manager.add_exit_request(
            1,
            [1u8; 48],
            100,
            ExitInitiator::ExecutionLayer,
        ).unwrap();
        
        assert_eq!(manager.get_exit_queue_length(), 1);
        
        let processed = manager.process_epoch(104);
        assert_eq!(processed.len(), 1);
        assert_eq!(manager.get_exit_queue_length(), 0);
    }
    
    #[test]
    fn test_withdrawal_request_queue() {
        let mut queue = WithdrawalRequestQueue::new();
        
        let request = WithdrawalRequest::new(
            Address::from([1u8; 20]),
            [2u8; 48],
            U256::from(1_000_000_000u64),
        );
        
        queue.enqueue(request.clone()).unwrap();
        assert_eq!(queue.size(), 1);
        
        let dequeued = queue.dequeue().unwrap();
        assert_eq!(dequeued, request);
        assert_eq!(queue.size(), 0);
    }
    
    #[test]
    fn test_contract_fee_updates() {
        let mut contract = WithdrawalRequestContract::new();
        
        let initial_fee = contract.get_current_fee();
        assert_eq!(initial_fee, U256::from(MIN_WITHDRAWAL_REQUEST_FEE));
        
        // Add many requests to increase excess
        for i in 0..10 {
            let request = WithdrawalRequest::new(
                Address::from([i as u8; 20]),
                [i as u8; 48],
                U256::from(1_000_000_000u64),
            );
            contract.add_request(request, initial_fee).unwrap();
        }
        
        contract.process_requests();
        
        let new_fee = contract.get_current_fee();
        assert!(new_fee > initial_fee);
    }
}