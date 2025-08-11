# Multi-stage build for optimized image size and security

# Stage 1: Builder
FROM rust:1.75-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    clang \
    llvm \
    libclang-dev \
    build-essential \
    git \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /usr/src/ethereum-rust

# Copy Cargo files for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/*/Cargo.toml crates/*/

# Build dependencies only (for caching)
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src target/release/ethereum-rust*

# Copy source code
COPY . .

# Build the application
RUN cargo build --release --all-features

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 -s /bin/bash ethereum && \
    mkdir -p /data /config && \
    chown -R ethereum:ethereum /data /config

# Copy binary from builder
COPY --from=builder /usr/src/ethereum-rust/target/release/ethereum-rust /usr/local/bin/ethereum-rust

# Copy default configuration
COPY --from=builder /usr/src/ethereum-rust/config/default.toml /config/default.toml

# Set user
USER ethereum

# Expose ports
# JSON-RPC HTTP
EXPOSE 8545
# JSON-RPC WebSocket
EXPOSE 8546
# P2P
EXPOSE 30303
# Discovery
EXPOSE 30303/udp
# Metrics
EXPOSE 9090
# Engine API
EXPOSE 8551

# Volume for blockchain data
VOLUME ["/data"]

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ethereum-rust health || exit 1

# Default command
ENTRYPOINT ["ethereum-rust"]
CMD ["run", "--config", "/config/default.toml", "--data-dir", "/data"]