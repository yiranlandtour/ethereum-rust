#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    // 0x00 - 0x0F: Stop and Arithmetic Operations
    STOP = 0x00,
    ADD = 0x01,
    MUL = 0x02,
    SUB = 0x03,
    DIV = 0x04,
    SDIV = 0x05,
    MOD = 0x06,
    SMOD = 0x07,
    ADDMOD = 0x08,
    MULMOD = 0x09,
    EXP = 0x0a,
    SIGNEXTEND = 0x0b,

    // 0x10 - 0x1F: Comparison & Bitwise Logic Operations
    LT = 0x10,
    GT = 0x11,
    SLT = 0x12,
    SGT = 0x13,
    EQ = 0x14,
    ISZERO = 0x15,
    AND = 0x16,
    OR = 0x17,
    XOR = 0x18,
    NOT = 0x19,
    BYTE = 0x1a,
    SHL = 0x1b,
    SHR = 0x1c,
    SAR = 0x1d,

    // 0x20 - 0x2F: SHA3
    KECCAK256 = 0x20,

    // 0x30 - 0x3F: Environmental Information
    ADDRESS = 0x30,
    BALANCE = 0x31,
    ORIGIN = 0x32,
    CALLER = 0x33,
    CALLVALUE = 0x34,
    CALLDATALOAD = 0x35,
    CALLDATASIZE = 0x36,
    CALLDATACOPY = 0x37,
    CODESIZE = 0x38,
    CODECOPY = 0x39,
    GASPRICE = 0x3a,
    EXTCODESIZE = 0x3b,
    EXTCODECOPY = 0x3c,
    RETURNDATASIZE = 0x3d,
    RETURNDATACOPY = 0x3e,
    EXTCODEHASH = 0x3f,

    // 0x40 - 0x4F: Block Information
    BLOCKHASH = 0x40,
    COINBASE = 0x41,
    TIMESTAMP = 0x42,
    NUMBER = 0x43,
    DIFFICULTY = 0x44,
    GASLIMIT = 0x45,
    CHAINID = 0x46,
    SELFBALANCE = 0x47,
    BASEFEE = 0x48,
    BLOBHASH = 0x49,
    BLOBBASEFEE = 0x4a,

    // 0x50 - 0x5F: Stack, Memory, Storage and Flow Operations
    POP = 0x50,
    MLOAD = 0x51,
    MSTORE = 0x52,
    MSTORE8 = 0x53,
    SLOAD = 0x54,
    SSTORE = 0x55,
    JUMP = 0x56,
    JUMPI = 0x57,
    PC = 0x58,
    MSIZE = 0x59,
    GAS = 0x5a,
    JUMPDEST = 0x5b,
    TLOAD = 0x5c,
    TSTORE = 0x5d,
    MCOPY = 0x5e,

    // 0x60 - 0x7F: Push Operations
    PUSH0 = 0x5f,
    PUSH1 = 0x60,
    PUSH2 = 0x61,
    PUSH3 = 0x62,
    PUSH4 = 0x63,
    PUSH5 = 0x64,
    PUSH6 = 0x65,
    PUSH7 = 0x66,
    PUSH8 = 0x67,
    PUSH9 = 0x68,
    PUSH10 = 0x69,
    PUSH11 = 0x6a,
    PUSH12 = 0x6b,
    PUSH13 = 0x6c,
    PUSH14 = 0x6d,
    PUSH15 = 0x6e,
    PUSH16 = 0x6f,
    PUSH17 = 0x70,
    PUSH18 = 0x71,
    PUSH19 = 0x72,
    PUSH20 = 0x73,
    PUSH21 = 0x74,
    PUSH22 = 0x75,
    PUSH23 = 0x76,
    PUSH24 = 0x77,
    PUSH25 = 0x78,
    PUSH26 = 0x79,
    PUSH27 = 0x7a,
    PUSH28 = 0x7b,
    PUSH29 = 0x7c,
    PUSH30 = 0x7d,
    PUSH31 = 0x7e,
    PUSH32 = 0x7f,

    // 0x80 - 0x8F: Duplication Operations
    DUP1 = 0x80,
    DUP2 = 0x81,
    DUP3 = 0x82,
    DUP4 = 0x83,
    DUP5 = 0x84,
    DUP6 = 0x85,
    DUP7 = 0x86,
    DUP8 = 0x87,
    DUP9 = 0x88,
    DUP10 = 0x89,
    DUP11 = 0x8a,
    DUP12 = 0x8b,
    DUP13 = 0x8c,
    DUP14 = 0x8d,
    DUP15 = 0x8e,
    DUP16 = 0x8f,

    // 0x90 - 0x9F: Exchange Operations
    SWAP1 = 0x90,
    SWAP2 = 0x91,
    SWAP3 = 0x92,
    SWAP4 = 0x93,
    SWAP5 = 0x94,
    SWAP6 = 0x95,
    SWAP7 = 0x96,
    SWAP8 = 0x97,
    SWAP9 = 0x98,
    SWAP10 = 0x99,
    SWAP11 = 0x9a,
    SWAP12 = 0x9b,
    SWAP13 = 0x9c,
    SWAP14 = 0x9d,
    SWAP15 = 0x9e,
    SWAP16 = 0x9f,

    // 0xA0 - 0xAF: Logging Operations
    LOG0 = 0xa0,
    LOG1 = 0xa1,
    LOG2 = 0xa2,
    LOG3 = 0xa3,
    LOG4 = 0xa4,

    // 0xF0 - 0xFF: System Operations
    CREATE = 0xf0,
    CALL = 0xf1,
    CALLCODE = 0xf2,
    RETURN = 0xf3,
    DELEGATECALL = 0xf4,
    CREATE2 = 0xf5,
    STATICCALL = 0xfa,
    REVERT = 0xfd,
    INVALID = 0xfe,
    SELFDESTRUCT = 0xff,
}

impl Opcode {
    pub fn from_u8(byte: u8) -> Option<Self> {
        match byte {
            0x00 => Some(Self::STOP),
            0x01 => Some(Self::ADD),
            0x02 => Some(Self::MUL),
            0x03 => Some(Self::SUB),
            0x04 => Some(Self::DIV),
            0x05 => Some(Self::SDIV),
            0x06 => Some(Self::MOD),
            0x07 => Some(Self::SMOD),
            0x08 => Some(Self::ADDMOD),
            0x09 => Some(Self::MULMOD),
            0x0a => Some(Self::EXP),
            0x0b => Some(Self::SIGNEXTEND),
            0x10 => Some(Self::LT),
            0x11 => Some(Self::GT),
            0x12 => Some(Self::SLT),
            0x13 => Some(Self::SGT),
            0x14 => Some(Self::EQ),
            0x15 => Some(Self::ISZERO),
            0x16 => Some(Self::AND),
            0x17 => Some(Self::OR),
            0x18 => Some(Self::XOR),
            0x19 => Some(Self::NOT),
            0x1a => Some(Self::BYTE),
            0x1b => Some(Self::SHL),
            0x1c => Some(Self::SHR),
            0x1d => Some(Self::SAR),
            0x20 => Some(Self::KECCAK256),
            0x30 => Some(Self::ADDRESS),
            0x31 => Some(Self::BALANCE),
            0x32 => Some(Self::ORIGIN),
            0x33 => Some(Self::CALLER),
            0x34 => Some(Self::CALLVALUE),
            0x35 => Some(Self::CALLDATALOAD),
            0x36 => Some(Self::CALLDATASIZE),
            0x37 => Some(Self::CALLDATACOPY),
            0x38 => Some(Self::CODESIZE),
            0x39 => Some(Self::CODECOPY),
            0x3a => Some(Self::GASPRICE),
            0x3b => Some(Self::EXTCODESIZE),
            0x3c => Some(Self::EXTCODECOPY),
            0x3d => Some(Self::RETURNDATASIZE),
            0x3e => Some(Self::RETURNDATACOPY),
            0x3f => Some(Self::EXTCODEHASH),
            0x40 => Some(Self::BLOCKHASH),
            0x41 => Some(Self::COINBASE),
            0x42 => Some(Self::TIMESTAMP),
            0x43 => Some(Self::NUMBER),
            0x44 => Some(Self::DIFFICULTY),
            0x45 => Some(Self::GASLIMIT),
            0x46 => Some(Self::CHAINID),
            0x47 => Some(Self::SELFBALANCE),
            0x48 => Some(Self::BASEFEE),
            0x49 => Some(Self::BLOBHASH),
            0x4a => Some(Self::BLOBBASEFEE),
            0x50 => Some(Self::POP),
            0x51 => Some(Self::MLOAD),
            0x52 => Some(Self::MSTORE),
            0x53 => Some(Self::MSTORE8),
            0x54 => Some(Self::SLOAD),
            0x55 => Some(Self::SSTORE),
            0x56 => Some(Self::JUMP),
            0x57 => Some(Self::JUMPI),
            0x58 => Some(Self::PC),
            0x59 => Some(Self::MSIZE),
            0x5a => Some(Self::GAS),
            0x5b => Some(Self::JUMPDEST),
            0x5c => Some(Self::TLOAD),
            0x5d => Some(Self::TSTORE),
            0x5e => Some(Self::MCOPY),
            0x5f => Some(Self::PUSH0),
            0x60..=0x7f => Some(unsafe { std::mem::transmute(byte) }),
            0x80..=0x8f => Some(unsafe { std::mem::transmute(byte) }),
            0x90..=0x9f => Some(unsafe { std::mem::transmute(byte) }),
            0xa0..=0xa4 => Some(unsafe { std::mem::transmute(byte) }),
            0xf0 => Some(Self::CREATE),
            0xf1 => Some(Self::CALL),
            0xf2 => Some(Self::CALLCODE),
            0xf3 => Some(Self::RETURN),
            0xf4 => Some(Self::DELEGATECALL),
            0xf5 => Some(Self::CREATE2),
            0xfa => Some(Self::STATICCALL),
            0xfd => Some(Self::REVERT),
            0xfe => Some(Self::INVALID),
            0xff => Some(Self::SELFDESTRUCT),
            _ => None,
        }
    }

    pub fn is_push(&self) -> bool {
        let byte = *self as u8;
        byte >= Self::PUSH1 as u8 && byte <= Self::PUSH32 as u8
    }

    pub fn push_bytes(&self) -> Option<usize> {
        if self.is_push() {
            Some((*self as u8 - Self::PUSH1 as u8 + 1) as usize)
        } else {
            None
        }
    }

    pub fn stack_inputs(&self) -> usize {
        match self {
            Self::STOP => 0,
            Self::ADD | Self::MUL | Self::SUB | Self::DIV | Self::SDIV | Self::MOD | Self::SMOD => 2,
            Self::ADDMOD | Self::MULMOD => 3,
            Self::EXP | Self::SIGNEXTEND => 2,
            Self::LT | Self::GT | Self::SLT | Self::SGT | Self::EQ => 2,
            Self::ISZERO | Self::NOT => 1,
            Self::AND | Self::OR | Self::XOR => 2,
            Self::BYTE => 2,
            Self::SHL | Self::SHR | Self::SAR => 2,
            Self::KECCAK256 => 2,
            Self::ADDRESS | Self::ORIGIN | Self::CALLER | Self::CALLVALUE => 0,
            Self::CALLDATALOAD => 1,
            Self::CALLDATASIZE | Self::CODESIZE => 0,
            Self::CALLDATACOPY | Self::CODECOPY => 3,
            Self::GASPRICE => 0,
            Self::EXTCODESIZE | Self::BALANCE | Self::EXTCODEHASH => 1,
            Self::EXTCODECOPY => 4,
            Self::RETURNDATASIZE => 0,
            Self::RETURNDATACOPY => 3,
            Self::BLOCKHASH => 1,
            Self::COINBASE | Self::TIMESTAMP | Self::NUMBER | Self::DIFFICULTY | Self::GASLIMIT => 0,
            Self::CHAINID | Self::SELFBALANCE | Self::BASEFEE => 0,
            Self::BLOBHASH => 1,
            Self::BLOBBASEFEE => 0,
            Self::POP => 1,
            Self::MLOAD | Self::SLOAD => 1,
            Self::MSTORE | Self::SSTORE => 2,
            Self::MSTORE8 => 2,
            Self::JUMP => 1,
            Self::JUMPI => 2,
            Self::PC | Self::MSIZE | Self::GAS => 0,
            Self::JUMPDEST => 0,
            Self::TLOAD => 1,
            Self::TSTORE => 2,
            Self::MCOPY => 3,
            Self::PUSH0 | Self::PUSH1 | Self::PUSH2 | Self::PUSH3 | Self::PUSH4 |
            Self::PUSH5 | Self::PUSH6 | Self::PUSH7 | Self::PUSH8 | Self::PUSH9 |
            Self::PUSH10 | Self::PUSH11 | Self::PUSH12 | Self::PUSH13 | Self::PUSH14 |
            Self::PUSH15 | Self::PUSH16 | Self::PUSH17 | Self::PUSH18 | Self::PUSH19 |
            Self::PUSH20 | Self::PUSH21 | Self::PUSH22 | Self::PUSH23 | Self::PUSH24 |
            Self::PUSH25 | Self::PUSH26 | Self::PUSH27 | Self::PUSH28 | Self::PUSH29 |
            Self::PUSH30 | Self::PUSH31 | Self::PUSH32 => 0,
            op if *op as u8 >= Self::DUP1 as u8 && *op as u8 <= Self::DUP16 as u8 => {
                (*op as u8 - Self::DUP1 as u8 + 1) as usize
            }
            op if *op as u8 >= Self::SWAP1 as u8 && *op as u8 <= Self::SWAP16 as u8 => {
                (*op as u8 - Self::SWAP1 as u8 + 2) as usize
            }
            Self::LOG0 => 2,
            Self::LOG1 => 3,
            Self::LOG2 => 4,
            Self::LOG3 => 5,
            Self::LOG4 => 6,
            Self::CREATE => 3,
            Self::CALL | Self::CALLCODE => 7,
            Self::RETURN | Self::REVERT => 2,
            Self::DELEGATECALL | Self::STATICCALL => 6,
            Self::CREATE2 => 4,
            Self::SELFDESTRUCT => 1,
            Self::INVALID => 0,
            _ => 0,
        }
    }

    pub fn stack_outputs(&self) -> usize {
        match self {
            Self::STOP | Self::JUMP | Self::JUMPI | Self::RETURN | Self::REVERT | 
            Self::SELFDESTRUCT | Self::SSTORE | Self::POP | Self::MSTORE | Self::MSTORE8 |
            Self::CALLDATACOPY | Self::CODECOPY | Self::EXTCODECOPY | Self::RETURNDATACOPY |
            Self::LOG0 | Self::LOG1 | Self::LOG2 | Self::LOG3 | Self::LOG4 | Self::JUMPDEST |
            Self::TSTORE | Self::MCOPY => 0,
            Self::PUSH0 | Self::PUSH1 | Self::PUSH2 | Self::PUSH3 | Self::PUSH4 |
            Self::PUSH5 | Self::PUSH6 | Self::PUSH7 | Self::PUSH8 | Self::PUSH9 |
            Self::PUSH10 | Self::PUSH11 | Self::PUSH12 | Self::PUSH13 | Self::PUSH14 |
            Self::PUSH15 | Self::PUSH16 | Self::PUSH17 | Self::PUSH18 | Self::PUSH19 |
            Self::PUSH20 | Self::PUSH21 | Self::PUSH22 | Self::PUSH23 | Self::PUSH24 |
            Self::PUSH25 | Self::PUSH26 | Self::PUSH27 | Self::PUSH28 | Self::PUSH29 |
            Self::PUSH30 | Self::PUSH31 | Self::PUSH32 => 1,
            op if *op as u8 >= Self::DUP1 as u8 && *op as u8 <= Self::DUP16 as u8 => {
                (*op as u8 - Self::DUP1 as u8 + 2) as usize
            }
            op if *op as u8 >= Self::SWAP1 as u8 && *op as u8 <= Self::SWAP16 as u8 => {
                (*op as u8 - Self::SWAP1 as u8 + 2) as usize
            }
            Self::CREATE | Self::CREATE2 | Self::CALL | Self::CALLCODE | 
            Self::DELEGATECALL | Self::STATICCALL => 1,
            Self::INVALID => 0,
            _ => 1,
        }
    }
}