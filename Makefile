.PHONY: help build test clean docker run dev setup lint security bench docs release

# Default target
help:
	@echo "Ethereum Rust - Development Commands"
	@echo ""
	@echo "Setup & Build:"
	@echo "  make setup       - Install development dependencies"
	@echo "  make build       - Build the project in release mode"
	@echo "  make clean       - Clean build artifacts"
	@echo ""
	@echo "Testing:"
	@echo "  make test        - Run all tests"
	@echo "  make test-unit   - Run unit tests only"
	@echo "  make test-e2e    - Run end-to-end tests"
	@echo "  make coverage    - Generate test coverage report"
	@echo "  make bench       - Run benchmarks"
	@echo ""
	@echo "Development:"
	@echo "  make dev         - Run in development mode with hot reload"
	@echo "  make run         - Run the node with default config"
	@echo "  make fmt         - Format code"
	@echo "  make lint        - Run linters (clippy)"
	@echo "  make check       - Run all checks (fmt, lint, test)"
	@echo ""
	@echo "Security:"
	@echo "  make security    - Run security scans"
	@echo "  make audit       - Run cargo audit"
	@echo "  make sbom        - Generate SBOM"
	@echo ""
	@echo "Docker:"
	@echo "  make docker      - Build Docker image"
	@echo "  make docker-run  - Run with docker-compose"
	@echo "  make docker-stop - Stop docker-compose"
	@echo "  make docker-logs - View docker logs"
	@echo ""
	@echo "Documentation:"
	@echo "  make docs        - Generate documentation"
	@echo "  make docs-serve  - Serve documentation locally"
	@echo ""
	@echo "Release:"
	@echo "  make release     - Create release build"
	@echo "  make dist        - Create distribution packages"

# Setup development environment
setup:
	@echo "Setting up development environment..."
	rustup update stable
	rustup component add rustfmt clippy
	cargo install cargo-tarpaulin cargo-audit cargo-license cargo-outdated cargo-watch
	cargo install mdbook mdbook-mermaid
	@echo "Installing pre-commit hooks..."
	echo '#!/bin/sh\nmake check' > .git/hooks/pre-commit
	chmod +x .git/hooks/pre-commit
	@echo "Setup complete!"

# Build commands
build:
	@echo "Building Ethereum Rust..."
	cargo build --release --all-features

build-dev:
	cargo build --all-features

clean:
	cargo clean
	rm -rf target/
	rm -rf node_modules/
	docker-compose down -v

# Testing commands
test:
	@echo "Running all tests..."
	cargo test --all-features --workspace

test-unit:
	@echo "Running unit tests..."
	cargo test --lib --all-features --workspace

test-e2e:
	@echo "Running end-to-end tests..."
	cargo test --test e2e_test --all-features

test-integration:
	@echo "Running integration tests..."
	cargo test --test '*' --all-features

coverage:
	@echo "Generating test coverage..."
	cargo tarpaulin --all-features --workspace --timeout 600 --out html --output-dir target/coverage
	@echo "Coverage report generated at target/coverage/index.html"

# Development commands
dev:
	@echo "Starting development server with hot reload..."
	cargo watch -x 'run -- --dev'

run:
	./target/release/ethereum-rust run

run-mainnet:
	./target/release/ethereum-rust run --network mainnet

run-sepolia:
	./target/release/ethereum-rust run --network sepolia

# Code quality
fmt:
	@echo "Formatting code..."
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

lint:
	@echo "Running clippy..."
	cargo clippy --all-features --workspace --tests -- -D warnings

check: fmt-check lint test
	@echo "All checks passed!"

# Security
security: audit
	@echo "Running security scans..."
	cargo audit
	@echo "Checking for outdated dependencies..."
	cargo outdated

audit:
	cargo audit

sbom:
	@echo "Generating SBOM..."
	cargo sbom > sbom.json

# Benchmarks
bench:
	@echo "Running benchmarks..."
	cargo bench --all-features --workspace

bench-compare:
	cargo bench --all-features --workspace -- --save-baseline current
	@echo "Baseline saved. Make changes and run 'make bench-diff' to compare"

bench-diff:
	cargo bench --all-features --workspace -- --baseline current

# Docker
docker:
	@echo "Building Docker image..."
	docker build -t ethereum/rust-ethereum:latest .

docker-run:
	@echo "Starting services with docker-compose..."
	docker-compose up -d
	@echo "Services started. Grafana: http://localhost:3000 (admin/ethereum)"

docker-stop:
	docker-compose down

docker-logs:
	docker-compose logs -f ethereum-rust

docker-clean:
	docker-compose down -v
	docker rmi ethereum/rust-ethereum:latest

# Documentation
docs:
	@echo "Generating documentation..."
	cargo doc --all-features --no-deps --workspace
	mdbook build docs/

docs-serve:
	@echo "Serving documentation at http://localhost:3000"
	cargo doc --all-features --no-deps --workspace --open &
	mdbook serve docs/ -p 3001

# Release
release:
	@echo "Creating release build..."
	cargo build --release --all-features
	strip target/release/ethereum-rust
	@echo "Release binary: target/release/ethereum-rust"

dist:
	@echo "Creating distribution packages..."
	mkdir -p dist
	# Linux x86_64
	cargo build --release --target x86_64-unknown-linux-gnu
	tar czf dist/ethereum-rust-linux-x86_64.tar.gz -C target/x86_64-unknown-linux-gnu/release ethereum-rust
	# macOS x86_64
	cargo build --release --target x86_64-apple-darwin
	tar czf dist/ethereum-rust-macos-x86_64.tar.gz -C target/x86_64-apple-darwin/release ethereum-rust
	# macOS ARM64
	cargo build --release --target aarch64-apple-darwin
	tar czf dist/ethereum-rust-macos-arm64.tar.gz -C target/aarch64-apple-darwin/release ethereum-rust
	@echo "Distribution packages created in dist/"

# Database management
db-reset:
	@echo "Resetting database..."
	rm -rf data/
	mkdir -p data/

db-backup:
	@echo "Creating database backup..."
	tar czf backup-$(shell date +%Y%m%d-%H%M%S).tar.gz data/

# Monitoring
monitor-start:
	@echo "Starting monitoring stack..."
	docker-compose up -d prometheus grafana

monitor-stop:
	docker-compose stop prometheus grafana

# Network testing
testnet-local:
	@echo "Starting local testnet..."
	./scripts/start-local-testnet.sh

testnet-stop:
	./scripts/stop-local-testnet.sh

# Performance profiling
profile:
	@echo "Running with profiler..."
	CARGO_PROFILE_RELEASE_DEBUG=true cargo build --release
	perf record -g ./target/release/ethereum-rust run --dev
	perf report

flamegraph:
	@echo "Generating flamegraph..."
	cargo install flamegraph
	cargo flamegraph --bin ethereum-rust -- run --dev

# CI/CD
ci-test:
	cargo test --all-features --workspace --release

ci-build:
	cargo build --all-features --workspace --release

ci-lint:
	cargo fmt --all -- --check
	cargo clippy --all-features --workspace --tests -- -D warnings

# Version management
version:
	@grep '^version' Cargo.toml | head -1 | cut -d'"' -f2

bump-patch:
	cargo set-version --bump patch

bump-minor:
	cargo set-version --bump minor

bump-major:
	cargo set-version --bump major