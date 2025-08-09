# Ethereum Rust

A complete Ethereum implementation in Rust, providing a full node implementation with support for the Ethereum protocol.

## Features

### Core Components
- **Full Node Implementation**: Complete Ethereum node with P2P networking, transaction pool, and state management
- **EVM**: Ethereum Virtual Machine implementation for smart contract execution
- **Storage**: RocksDB and in-memory database backends for blockchain storage
- **Consensus**: Support for Proof of Stake (PoS) and Proof of Authority (Clique)
- **Networking**: DevP2P protocol implementation with RLPx encryption and Discovery v4
- **JSON-RPC API**: Full Ethereum JSON-RPC API support (eth, net, web3, debug, trace)
- **Account Management**: HD wallet support, keystore management, transaction signing
- **Monitoring**: Prometheus metrics, health checks, and alerting system

## Architecture

For detailed architecture documentation, see [ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Project Structure

```
ethereum-rust/
├── src/                    # Main binary
├── crates/                 # Workspace crates
│   ├── types/             # Core Ethereum types
│   ├── rlp/               # RLP encoding/decoding
│   ├── crypto/            # Cryptographic primitives
│   ├── core/              # Core blockchain logic
│   ├── consensus/         # Consensus engines
│   ├── network/           # P2P networking
│   ├── storage/           # Database and storage
│   ├── rpc/               # JSON-RPC APIs
│   └── evm/               # Ethereum Virtual Machine
└── docs/                   # Documentation
```

## Getting Started

### Prerequisites

- Rust 1.75 or later
- Cargo

### Building

```bash
cargo build --release
```

### Running Tests

```bash
cargo test --workspace
```

### Running the Node

```bash
# Run on mainnet (default)
cargo run --release -- run

# Run on a specific network
cargo run --release -- run --network goerli

# Run with custom ports
cargo run --release -- run --http-port 8545 --ws-port 8546 --p2p-port 30303
```

### CLI Commands

#### Initialize Genesis

```bash
cargo run --release -- init --genesis genesis.json --datadir ./data
```

#### Account Management

```bash
# Create new account
cargo run --release -- account new

# List accounts
cargo run --release -- account list

# Import private key
cargo run --release -- account import --key private_key.txt
```

#### Database Utilities

```bash
# Inspect database
cargo run --release -- db inspect

# Prune database
cargo run --release -- db prune
```

## Development Phases

1. **Phase 1: Foundation** (In Progress)
   - Basic type system ✓
   - RLP encoding/decoding ✓
   - Cryptographic primitives
   - Database abstraction

2. **Phase 2: Core Components**
   - Blockchain structure
   - Transaction types
   - State management
   - Merkle Patricia Trie

3. **Phase 3: EVM Implementation**
   - EVM interpreter
   - Opcode implementation
   - Precompiled contracts

4. **Phase 4: Networking**
   - P2P framework
   - Discovery protocols
   - Wire protocol

5. **Phase 5: Consensus**
   - Proof of Stake
   - Fork choice rules
   - Beacon chain integration

6. **Phase 6: RPC & APIs**
   - JSON-RPC server
   - Ethereum APIs
   - WebSocket support

7. **Phase 7: Integration & Testing**
   - End-to-end testing
   - Performance optimization
   - Security audits

8. **Phase 8: Production Readiness**
   - Mainnet testing
   - Deployment tools
   - Launch preparation

## Contributing

Contributions are welcome! Please read our contributing guidelines before submitting PRs.

## License

This project is dual-licensed under MIT and Apache 2.0 licenses.

## Resources

- [Ethereum Yellow Paper](https://ethereum.github.io/yellowpaper/paper.pdf)
- [Ethereum Improvement Proposals (EIPs)](https://eips.ethereum.org/)
- [Go-Ethereum Implementation](https://github.com/ethereum/go-ethereum)

## Acknowledgments

This implementation is inspired by the original go-ethereum client and aims to provide a Rust-based alternative while maintaining full protocol compatibility.