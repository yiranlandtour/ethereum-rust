# Contributing to Ethereum Rust

Thank you for your interest in contributing to Ethereum Rust! We welcome contributions from everyone. This document provides guidelines and instructions for contributing to the project.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [How to Contribute](#how-to-contribute)
- [Coding Standards](#coding-standards)
- [Testing](#testing)
- [Documentation](#documentation)
- [Pull Request Process](#pull-request-process)
- [Security](#security)

## Code of Conduct

We are committed to providing a welcoming and inclusive environment. Please read and follow our Code of Conduct:

- Be respectful and inclusive
- Welcome newcomers and help them get started
- Focus on constructive criticism
- Accept feedback gracefully
- Prioritize the project's best interests

## Getting Started

1. **Fork the repository** on GitHub
2. **Clone your fork** locally:
   ```bash
   git clone https://github.com/YOUR_USERNAME/ethereum-rust.git
   cd ethereum-rust
   ```
3. **Add upstream remote**:
   ```bash
   git remote add upstream https://github.com/ethereum/rust-ethereum.git
   ```
4. **Create a feature branch**:
   ```bash
   git checkout -b feature/your-feature-name
   ```

## Development Setup

### Prerequisites

- Rust 1.75 or later
- Git
- Make (optional but recommended)
- Docker (for integration testing)

### Initial Setup

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install development tools
make setup

# Or manually:
rustup component add rustfmt clippy
cargo install cargo-watch cargo-tarpaulin cargo-audit
```

### Building the Project

```bash
# Build in debug mode
cargo build

# Build in release mode
cargo build --release

# Build with all features
cargo build --all-features
```

### Running Tests

```bash
# Run all tests
make test

# Run specific test
cargo test test_name

# Run with coverage
make coverage
```

## How to Contribute

### Reporting Bugs

1. **Check existing issues** to avoid duplicates
2. **Create a new issue** with:
   - Clear, descriptive title
   - Steps to reproduce
   - Expected vs actual behavior
   - System information
   - Relevant logs or screenshots

### Suggesting Features

1. **Open a discussion** first for major features
2. **Create a feature request** with:
   - Problem statement
   - Proposed solution
   - Alternative approaches considered
   - Impact on existing functionality

### Contributing Code

#### Areas We Need Help

- **Performance optimizations**
- **Test coverage improvements**
- **Documentation**
- **Bug fixes**
- **Feature implementations**
- **Security enhancements**

#### Finding Issues

Look for issues labeled:
- `good-first-issue` - Great for newcomers
- `help-wanted` - Community help needed
- `bug` - Bug fixes
- `enhancement` - New features
- `documentation` - Documentation improvements

## Coding Standards

### Rust Style Guide

We follow the official Rust style guide with these additions:

```rust
// Use explicit imports
use std::collections::HashMap;
use ethereum_types::{H256, U256};

// Group imports: std, external, internal
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::types::Block;

// Document public APIs
/// Processes a block through the execution engine.
///
/// # Arguments
/// * `block` - The block to process
///
/// # Returns
/// * `Result<Receipt>` - Processing receipt or error
pub fn process_block(block: &Block) -> Result<Receipt> {
    // Implementation
}

// Use descriptive variable names
let block_number = 12345;  // Good
let bn = 12345;            // Avoid

// Handle errors explicitly
let result = operation()?;  // Good
let result = operation().unwrap();  // Avoid in production

// Use const for constants
const MAX_BLOCK_SIZE: usize = 1_000_000;

// Prefer iterators over loops
let sum: u64 = values.iter().sum();  // Good
```

### Commit Messages

Follow conventional commits:

```
type(scope): description

[optional body]

[optional footer]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation
- `test`: Testing
- `perf`: Performance
- `refactor`: Code refactoring
- `style`: Formatting
- `ci`: CI/CD changes
- `chore`: Maintenance

Examples:
```
feat(evm): implement EIP-1559 gas pricing
fix(network): resolve peer connection timeout
docs(api): update RPC method documentation
test(consensus): add SSF finality tests
perf(storage): optimize database queries
```

### Code Review Checklist

Before submitting PR, ensure:

- [ ] Code compiles without warnings
- [ ] All tests pass
- [ ] New tests added for new features
- [ ] Documentation updated
- [ ] Formatting checked (`cargo fmt`)
- [ ] Linting passed (`cargo clippy`)
- [ ] No security vulnerabilities (`cargo audit`)
- [ ] Performance impact considered
- [ ] Breaking changes documented

## Testing

### Test Categories

1. **Unit Tests** - Test individual functions
   ```rust
   #[test]
   fn test_block_validation() {
       let block = create_test_block();
       assert!(validate_block(&block).is_ok());
   }
   ```

2. **Integration Tests** - Test module interactions
   ```rust
   #[tokio::test]
   async fn test_transaction_processing() {
       let node = setup_test_node().await;
       let tx = create_transaction();
       assert!(node.process_transaction(tx).await.is_ok());
   }
   ```

3. **End-to-End Tests** - Test complete workflows
   ```rust
   #[tokio::test]
   async fn test_block_production() {
       let network = setup_test_network().await;
       let block = network.produce_block().await;
       assert!(network.finalize_block(block).await.is_ok());
   }
   ```

### Writing Tests

- Test both success and failure cases
- Use descriptive test names
- Keep tests focused and independent
- Mock external dependencies
- Use property-based testing for complex logic

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_transaction_serialization(tx in any::<Transaction>()) {
        let encoded = tx.encode();
        let decoded = Transaction::decode(&encoded).unwrap();
        assert_eq!(tx, decoded);
    }
}
```

## Documentation

### Code Documentation

- Document all public APIs
- Include examples in doc comments
- Explain complex algorithms
- Document assumptions and invariants

```rust
/// Executes transactions in parallel while maintaining state consistency.
///
/// This function uses optimistic concurrency control to execute multiple
/// transactions simultaneously, rolling back conflicting transactions.
///
/// # Example
/// ```
/// let transactions = vec![tx1, tx2, tx3];
/// let results = execute_parallel(transactions).await?;
/// ```
///
/// # Panics
/// Panics if the worker thread pool is not initialized.
pub async fn execute_parallel(txs: Vec<Transaction>) -> Result<Vec<Receipt>> {
    // Implementation
}
```

### README Updates

Update README.md when:
- Adding new features
- Changing configuration
- Modifying build process
- Adding dependencies

## Pull Request Process

### Before Submitting

1. **Sync with upstream**:
   ```bash
   git fetch upstream
   git rebase upstream/main
   ```

2. **Run checks**:
   ```bash
   make check  # Runs fmt, clippy, and tests
   ```

3. **Update documentation**

4. **Add tests** for new functionality

### PR Guidelines

1. **Title**: Clear, concise description
2. **Description**: Include:
   - Problem being solved
   - Approach taken
   - Testing performed
   - Breaking changes
   - Related issues

3. **Size**: Keep PRs focused and manageable
   - Prefer multiple small PRs over one large PR
   - Separate refactoring from feature changes

### Review Process

1. **Automated checks** must pass
2. **Code review** by maintainers
3. **Address feedback** promptly
4. **Squash commits** before merge

### After Merge

- Delete your feature branch
- Update your fork:
  ```bash
  git checkout main
  git pull upstream main
  git push origin main
  ```

## Security

### Reporting Vulnerabilities

**DO NOT** create public issues for security vulnerabilities.

Email security@ethereum-rust.org with:
- Description of vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### Security Best Practices

- Never commit secrets or keys
- Validate all inputs
- Use safe arithmetic operations
- Handle errors explicitly
- Follow principle of least privilege
- Keep dependencies updated

## Development Tips

### Useful Commands

```bash
# Watch for changes and rebuild
cargo watch -x build

# Run specific test with output
cargo test test_name -- --nocapture

# Check for outdated dependencies
cargo outdated

# Generate documentation
cargo doc --open

# Profile performance
cargo build --release && perf record ./target/release/ethereum-rust
```

### Debugging

```rust
// Use debug prints during development
dbg!(&variable);

// Use tracing for production
use tracing::{debug, info, warn, error};
info!("Processing block {}", block_number);

// Set log level
RUST_LOG=debug cargo run
```

### Performance

- Profile before optimizing
- Benchmark critical paths
- Consider memory allocation
- Use appropriate data structures
- Leverage parallelism where beneficial

## Getting Help

- **Discord**: [Join our community](https://discord.gg/ethereum-rust)
- **GitHub Discussions**: Ask questions and share ideas
- **Documentation**: [docs.ethereum-rust.org](https://docs.ethereum-rust.org)
- **Office Hours**: Thursdays 3PM UTC

## Recognition

Contributors are recognized in:
- [CONTRIBUTORS.md](CONTRIBUTORS.md)
- Release notes
- Project documentation

Thank you for contributing to Ethereum Rust! 🚀