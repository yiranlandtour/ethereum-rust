#!/bin/bash

# Ethereum Rust Node Startup Script

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Ethereum Rust Node Startup${NC}"
echo "=============================="

# Default values
NETWORK="mainnet"
DATA_DIR="./data"
HTTP_PORT=8545
WS_PORT=8546
P2P_PORT=30303

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --network)
            NETWORK="$2"
            shift 2
            ;;
        --datadir)
            DATA_DIR="$2"
            shift 2
            ;;
        --http-port)
            HTTP_PORT="$2"
            shift 2
            ;;
        --ws-port)
            WS_PORT="$2"
            shift 2
            ;;
        --p2p-port)
            P2P_PORT="$2"
            shift 2
            ;;
        --dev)
            NETWORK="dev"
            shift
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# Build the project
echo -e "${YELLOW}Building Ethereum Rust...${NC}"
cargo build --release

# Initialize genesis if needed
if [ ! -d "$DATA_DIR/chaindata" ]; then
    echo -e "${YELLOW}Initializing genesis block...${NC}"
    if [ "$NETWORK" = "dev" ]; then
        ./target/release/ethereum-rust init --genesis genesis.json --datadir "$DATA_DIR"
    fi
fi

# Start the node
echo -e "${GREEN}Starting Ethereum Rust node...${NC}"
echo "Network: $NETWORK"
echo "Data directory: $DATA_DIR"
echo "HTTP RPC: http://localhost:$HTTP_PORT"
echo "WebSocket RPC: ws://localhost:$WS_PORT"
echo "P2P Port: $P2P_PORT"
echo ""

exec ./target/release/ethereum-rust run \
    --network "$NETWORK" \
    --datadir "$DATA_DIR" \
    --http-port "$HTTP_PORT" \
    --ws-port "$WS_PORT" \
    --p2p-port "$P2P_PORT"