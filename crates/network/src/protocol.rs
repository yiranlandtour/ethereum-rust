// use ethereum_types::{H256, U256}; // Unused imports
// use ethereum_rlp::{Encode, Decode}; // Unused imports
use std::fmt;

use crate::Result;

pub const ETH_PROTOCOL_VERSION: u8 = 68;
pub const SNAP_PROTOCOL_VERSION: u8 = 1;

#[derive(Debug, Clone)]
pub struct Protocol {
    pub name: String,
    pub version: u8,
    pub message_count: u8,
}

impl Protocol {
    pub fn eth() -> Self {
        Self {
            name: "eth".to_string(),
            version: ETH_PROTOCOL_VERSION,
            message_count: 17,
        }
    }
    
    pub fn snap() -> Self {
        Self {
            name: "snap".to_string(),
            version: SNAP_PROTOCOL_VERSION,
            message_count: 8,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Capability {
    pub name: String,
    pub version: u8,
}

impl Capability {
    pub fn new(name: String, version: u8) -> Self {
        Self { name, version }
    }
    
    pub fn eth() -> Self {
        Self::new("eth".to_string(), ETH_PROTOCOL_VERSION)
    }
    
    pub fn snap() -> Self {
        Self::new("snap".to_string(), SNAP_PROTOCOL_VERSION)
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.name, self.version)
    }
}

pub struct ProtocolHandler {
    pub protocol: Protocol,
}

impl ProtocolHandler {
    pub fn new(protocol: Protocol) -> Self {
        Self { protocol }
    }
    
    pub fn handle_message(&self, msg_id: u8, data: &[u8]) -> Result<()> {
        match self.protocol.name.as_str() {
            "eth" => self.handle_eth_message(msg_id, data),
            "snap" => self.handle_snap_message(msg_id, data),
            _ => Ok(()),
        }
    }
    
    fn handle_eth_message(&self, msg_id: u8, _data: &[u8]) -> Result<()> {
        match msg_id {
            0x00 => {}, // Status
            0x01 => {}, // NewBlockHashes
            0x02 => {}, // Transactions
            0x03 => {}, // GetBlockHeaders
            0x04 => {}, // BlockHeaders
            0x05 => {}, // GetBlockBodies
            0x06 => {}, // BlockBodies
            0x07 => {}, // NewBlock
            0x08 => {}, // NewPooledTransactionHashes
            0x09 => {}, // GetPooledTransactions
            0x0a => {}, // PooledTransactions
            0x0d => {}, // GetNodeData
            0x0e => {}, // NodeData
            0x0f => {}, // GetReceipts
            0x10 => {}, // Receipts
            _ => {},
        }
        Ok(())
    }
    
    fn handle_snap_message(&self, msg_id: u8, _data: &[u8]) -> Result<()> {
        match msg_id {
            0x00 => {}, // GetAccountRange
            0x01 => {}, // AccountRange
            0x02 => {}, // GetStorageRanges
            0x03 => {}, // StorageRanges
            0x04 => {}, // GetByteCodes
            0x05 => {}, // ByteCodes
            0x06 => {}, // GetTrieNodes
            0x07 => {}, // TrieNodes
            _ => {},
        }
        Ok(())
    }
}