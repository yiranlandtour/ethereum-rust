#!/bin/bash

#############################################
# Ethereum Rust - Testnet Deployment Script
#############################################

set -euo pipefail

# Configuration
NETWORK="${NETWORK:-sepolia}"
NODE_NAME="${NODE_NAME:-ethereum-rust-testnet}"
DATA_DIR="${DATA_DIR:-/data/testnet}"
CONFIG_DIR="${CONFIG_DIR:-/config/testnet}"
LOG_DIR="${LOG_DIR:-/var/log/ethereum-rust}"
MONITORING_ENABLED="${MONITORING_ENABLED:-true}"
METRICS_PORT="${METRICS_PORT:-9090}"
RPC_PORT="${RPC_PORT:-8545}"
WS_PORT="${WS_PORT:-8546}"
P2P_PORT="${P2P_PORT:-30303}"
ENGINE_PORT="${ENGINE_PORT:-8551}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Logging
log() {
    echo -e "${GREEN}[$(date '+%Y-%m-%d %H:%M:%S')]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
    exit 1
}

warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

# Banner
show_banner() {
    cat << "EOF"
 _____ _   _                                  ____           _   
| ____| |_| |__   ___ _ __ ___ _   _ _ __ ___|  _ \ _   _ ___| |_ 
|  _| | __| '_ \ / _ \ '__/ _ \ | | | '_ ` _ \ |_) | | | / __| __|
| |___| |_| | | |  __/ | |  __/ |_| | | | | | |  _ <| |_| \__ \ |_ 
|_____|\__|_| |_|\___|_|  \___|\__,_|_| |_| |_|_| \_\\__,_|___/\__|
                                                                    
            Testnet Deployment Script v1.0
EOF
}

# Check prerequisites
check_prerequisites() {
    log "Checking prerequisites..."
    
    # Check if running as root
    if [[ $EUID -eq 0 ]]; then
        warning "Running as root is not recommended"
    fi
    
    # Check required tools
    local tools=("docker" "docker-compose" "curl" "jq" "git")
    for tool in "${tools[@]}"; do
        if ! command -v "$tool" &> /dev/null; then
            error "$tool is required but not installed"
        fi
    done
    
    # Check Docker daemon
    if ! docker info &> /dev/null; then
        error "Docker daemon is not running"
    fi
    
    # Check network connectivity
    if ! curl -s --head https://api.github.com > /dev/null; then
        warning "Cannot reach GitHub API - network issues?"
    fi
    
    # Check disk space
    local available_space=$(df "$DATA_DIR" 2>/dev/null | awk 'NR==2 {print $4}' || df / | awk 'NR==2 {print $4}')
    local required_space=$((100 * 1024 * 1024)) # 100GB in KB
    if [[ $available_space -lt $required_space ]]; then
        error "Insufficient disk space. Required: 100GB, Available: $((available_space / 1024 / 1024))GB"
    fi
    
    log "Prerequisites check completed"
}

# Setup directories
setup_directories() {
    log "Setting up directories..."
    
    mkdir -p "$DATA_DIR" "$CONFIG_DIR" "$LOG_DIR"
    mkdir -p "$DATA_DIR"/{db,consensus,engine}
    mkdir -p "$CONFIG_DIR"/{keys,certs}
    
    # Set permissions
    chmod 700 "$CONFIG_DIR/keys"
    
    log "Directories created"
}

# Generate JWT secret
generate_jwt_secret() {
    log "Generating JWT secret..."
    
    if [[ ! -f "$CONFIG_DIR/jwt.hex" ]]; then
        openssl rand -hex 32 > "$CONFIG_DIR/jwt.hex"
        chmod 600 "$CONFIG_DIR/jwt.hex"
        info "JWT secret generated at $CONFIG_DIR/jwt.hex"
    else
        info "JWT secret already exists"
    fi
}

# Create configuration
create_config() {
    log "Creating configuration for $NETWORK..."
    
    cat > "$CONFIG_DIR/config.toml" << EOF
# Ethereum Rust - Testnet Configuration
[node]
name = "$NODE_NAME"
chain = "$NETWORK"
data_dir = "$DATA_DIR"
archive = false

[network]
listen_addr = "0.0.0.0:$P2P_PORT"
max_peers = 50
min_peers = 5
discovery = true
discovery_version = "v5"
nat = "any"

$(get_boot_nodes "$NETWORK")

[execution]
parallel = true
workers = 4
jit = true
cache_size = 2048

[consensus]
engine_endpoint = "0.0.0.0:$ENGINE_PORT"
jwt_secret = "$CONFIG_DIR/jwt.hex"
ssf_enabled = false  # Not enabled on testnet yet

[storage]
engine = "rocksdb"
path = "$DATA_DIR/db"
cache = 1024
compression = "zstd"

[storage.history_expiry]
enabled = true
retention = "90d"
min_blocks = 128

[rpc]
[rpc.http]
enabled = true
host = "0.0.0.0"
port = $RPC_PORT
apis = ["eth", "net", "web3", "debug", "trace", "txpool"]
cors = ["*"]
max_connections = 100

[rpc.ws]
enabled = true
host = "0.0.0.0"
port = $WS_PORT
max_connections = 100

[mempool]
max_size = 5000
min_gas_price = 1000000000

[mev]
enabled = false  # Disabled for testnet

[metrics]
enabled = $MONITORING_ENABLED
endpoint = "0.0.0.0:$METRICS_PORT"

[logging]
level = "info"
file = "$LOG_DIR/ethereum-rust.log"
max_size = 100
max_backups = 10
EOF
    
    log "Configuration created at $CONFIG_DIR/config.toml"
}

# Get network-specific boot nodes
get_boot_nodes() {
    local network=$1
    
    case "$network" in
        sepolia)
            cat << 'EOF'
boot_nodes = [
    "enode://4e5e92199ee224a01932a377160aa432f31d0b351f84ab413a8e0a42f4f36476f8fb1cbe914af0d9aef0d51665c214cf653c651c4bbd9d5550a934f241f1682b@138.197.51.181:30303",
    "enode://143e11fb766781d22d92a2e33f8f104cddae4411a122295ed1fdb6638de96a6ce65f5b7c964ba3763bba27961738fef7d3ecc739268f3e5e771fb4c87b6234ba@146.190.1.103:30303",
    "enode://8b61dc2d06c3f96fddcbebb0efb29e60d3598616275a29418d7e4f258e51820a0de020401650a756f9e62b3e49f8529dc9ed3ae7bb538e22400cbc40eb331993@170.64.250.88:30303"
]
EOF
            ;;
        holesky)
            cat << 'EOF'
boot_nodes = [
    "enode://a86d6c98cf916745330739b87e32db8097df265dd4ba973e4097b9cc2ba24d3c07f77db96e52e16ec50b54bd648e81c59e1cf1798e8d5db9074fb8bcea4b20e5@194.61.28.32:30303",
    "enode://7a7768e56e9d6393c6b0002e92c11e679d6b18f956c4f690e2e4e0b0b7797f5c7b8c04f93b5e0c8e82e8a4c8f9c4a6f8d6e1c8e9b0e5e7e8e4e0b0b7797f5c7@135.181.140.168:30303"
]
EOF
            ;;
        mainnet)
            error "Mainnet deployment not allowed with this script"
            ;;
        *)
            error "Unknown network: $network"
            ;;
    esac
}

# Setup consensus client
setup_consensus_client() {
    log "Setting up consensus client (Lighthouse)..."
    
    cat > "$CONFIG_DIR/docker-compose-consensus.yml" << EOF
version: '3.8'

services:
  lighthouse:
    image: sigp/lighthouse:latest
    container_name: ${NODE_NAME}-consensus
    restart: unless-stopped
    ports:
      - "9000:9000"
      - "5052:5052"
    volumes:
      - $DATA_DIR/consensus:/data
      - $CONFIG_DIR/jwt.hex:/jwt.hex:ro
    environment:
      - NETWORK=$NETWORK
    command:
      - lighthouse
      - bn
      - --network=$NETWORK
      - --datadir=/data
      - --http
      - --http-address=0.0.0.0
      - --execution-endpoint=http://ethereum-rust:$ENGINE_PORT
      - --execution-jwt=/jwt.hex
      - --checkpoint-sync-url=$(get_checkpoint_sync_url $NETWORK)
      - --disable-deposit-contract-sync
    networks:
      - ethereum-testnet

networks:
  ethereum-testnet:
    external: true
EOF
    
    log "Consensus client configuration created"
}

# Get checkpoint sync URL
get_checkpoint_sync_url() {
    local network=$1
    
    case "$network" in
        sepolia)
            echo "https://sepolia.beaconstate.info"
            ;;
        holesky)
            echo "https://holesky.beaconstate.info"
            ;;
        *)
            echo ""
            ;;
    esac
}

# Setup execution client
setup_execution_client() {
    log "Setting up execution client..."
    
    cat > "$CONFIG_DIR/docker-compose.yml" << EOF
version: '3.8'

services:
  ethereum-rust:
    image: ethereum/rust-ethereum:latest
    container_name: ${NODE_NAME}-execution
    build:
      context: /home/felix/pro/test-ybtc/ethereum-rust
      dockerfile: Dockerfile
    restart: unless-stopped
    ports:
      - "$RPC_PORT:$RPC_PORT"
      - "$WS_PORT:$WS_PORT"
      - "$P2P_PORT:$P2P_PORT"
      - "$P2P_PORT:$P2P_PORT/udp"
      - "$ENGINE_PORT:$ENGINE_PORT"
      - "$METRICS_PORT:$METRICS_PORT"
    volumes:
      - $DATA_DIR:/data
      - $CONFIG_DIR:/config:ro
      - $LOG_DIR:/logs
    environment:
      - RUST_LOG=info
      - NETWORK=$NETWORK
    command:
      - run
      - --config=/config/config.toml
    networks:
      - ethereum-testnet
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:$RPC_PORT"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s

networks:
  ethereum-testnet:
    driver: bridge
EOF
    
    log "Execution client configuration created"
}

# Setup monitoring
setup_monitoring() {
    if [[ "$MONITORING_ENABLED" != "true" ]]; then
        log "Monitoring disabled, skipping setup"
        return
    fi
    
    log "Setting up monitoring stack..."
    
    cat > "$CONFIG_DIR/docker-compose-monitoring.yml" << EOF
version: '3.8'

services:
  prometheus:
    image: prom/prometheus:latest
    container_name: ${NODE_NAME}-prometheus
    restart: unless-stopped
    ports:
      - "9091:9090"
    volumes:
      - $CONFIG_DIR/prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - prometheus-data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
    networks:
      - ethereum-testnet

  grafana:
    image: grafana/grafana:latest
    container_name: ${NODE_NAME}-grafana
    restart: unless-stopped
    ports:
      - "3000:3000"
    volumes:
      - grafana-data:/var/lib/grafana
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=testnet123
      - GF_INSTALL_PLUGINS=grafana-piechart-panel
    networks:
      - ethereum-testnet

  node-exporter:
    image: prom/node-exporter:latest
    container_name: ${NODE_NAME}-node-exporter
    restart: unless-stopped
    ports:
      - "9100:9100"
    volumes:
      - /proc:/host/proc:ro
      - /sys:/host/sys:ro
      - /:/rootfs:ro
    command:
      - '--path.procfs=/host/proc'
      - '--path.sysfs=/host/sys'
      - '--path.rootfs=/rootfs'
    networks:
      - ethereum-testnet

volumes:
  prometheus-data:
  grafana-data:

networks:
  ethereum-testnet:
    external: true
EOF
    
    # Create Prometheus configuration
    cat > "$CONFIG_DIR/prometheus.yml" << EOF
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'ethereum-rust'
    static_configs:
      - targets: ['ethereum-rust:$METRICS_PORT']

  - job_name: 'lighthouse'
    static_configs:
      - targets: ['lighthouse:5054']

  - job_name: 'node-exporter'
    static_configs:
      - targets: ['node-exporter:9100']
EOF
    
    log "Monitoring configuration created"
}

# Deploy node
deploy_node() {
    log "Deploying $NODE_NAME on $NETWORK..."
    
    # Create network
    docker network create ethereum-testnet 2>/dev/null || true
    
    # Build and start execution client
    log "Starting execution client..."
    cd "$CONFIG_DIR"
    docker-compose up -d
    
    # Wait for execution client to be ready
    log "Waiting for execution client to be ready..."
    local max_attempts=30
    local attempt=0
    while [[ $attempt -lt $max_attempts ]]; do
        if curl -s -X POST "http://localhost:$RPC_PORT" \
            -H "Content-Type: application/json" \
            -d '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' &> /dev/null; then
            log "Execution client is ready"
            break
        fi
        sleep 5
        ((attempt++))
    done
    
    if [[ $attempt -ge $max_attempts ]]; then
        error "Execution client failed to start"
    fi
    
    # Start consensus client
    log "Starting consensus client..."
    docker-compose -f docker-compose-consensus.yml up -d
    
    # Start monitoring if enabled
    if [[ "$MONITORING_ENABLED" == "true" ]]; then
        log "Starting monitoring stack..."
        docker-compose -f docker-compose-monitoring.yml up -d
    fi
    
    log "Deployment completed successfully!"
}

# Verify deployment
verify_deployment() {
    log "Verifying deployment..."
    
    local checks_passed=0
    local checks_total=0
    
    # Check execution client
    ((checks_total++))
    if docker ps | grep -q "${NODE_NAME}-execution"; then
        log "✓ Execution client is running"
        ((checks_passed++))
    else
        error "✗ Execution client is not running"
    fi
    
    # Check consensus client
    ((checks_total++))
    if docker ps | grep -q "${NODE_NAME}-consensus"; then
        log "✓ Consensus client is running"
        ((checks_passed++))
    else
        error "✗ Consensus client is not running"
    fi
    
    # Check RPC endpoint
    ((checks_total++))
    if curl -s -X POST "http://localhost:$RPC_PORT" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' | jq -r '.result' &> /dev/null; then
        log "✓ RPC endpoint is responding"
        ((checks_passed++))
    else
        warning "✗ RPC endpoint is not responding"
    fi
    
    # Check WebSocket endpoint
    ((checks_total++))
    if echo "" | timeout 2 nc -z localhost "$WS_PORT" &> /dev/null; then
        log "✓ WebSocket endpoint is open"
        ((checks_passed++))
    else
        warning "✗ WebSocket endpoint is not open"
    fi
    
    # Check metrics endpoint
    if [[ "$MONITORING_ENABLED" == "true" ]]; then
        ((checks_total++))
        if curl -s "http://localhost:$METRICS_PORT/metrics" | grep -q "ethereum_"; then
            log "✓ Metrics endpoint is working"
            ((checks_passed++))
        else
            warning "✗ Metrics endpoint is not working"
        fi
    fi
    
    # Check sync status
    ((checks_total++))
    local sync_status=$(curl -s -X POST "http://localhost:$RPC_PORT" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' | jq -r '.result')
    
    if [[ "$sync_status" != "false" ]]; then
        log "✓ Node is syncing (this is normal for initial deployment)"
        ((checks_passed++))
    else
        log "✓ Node is fully synced"
        ((checks_passed++))
    fi
    
    log "Verification completed: $checks_passed/$checks_total checks passed"
    
    if [[ $checks_passed -ne $checks_total ]]; then
        warning "Some checks failed. Please review the logs."
    fi
}

# Show status
show_status() {
    echo ""
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║               Ethereum Rust Testnet Status                ║"
    echo "╠════════════════════════════════════════════════════════════╣"
    
    # Get block number
    local block_number=$(curl -s -X POST "http://localhost:$RPC_PORT" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
        | jq -r '.result' | xargs printf "%d\n" 2>/dev/null || echo "N/A")
    
    # Get peer count
    local peer_count=$(curl -s -X POST "http://localhost:$RPC_PORT" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}' \
        | jq -r '.result' | xargs printf "%d\n" 2>/dev/null || echo "N/A")
    
    # Get sync status
    local sync_status=$(curl -s -X POST "http://localhost:$RPC_PORT" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' \
        | jq -r '.result' 2>/dev/null)
    
    if [[ "$sync_status" == "false" ]]; then
        sync_status="Synced"
    elif [[ -n "$sync_status" ]]; then
        local current_block=$(echo "$sync_status" | jq -r '.currentBlock' | xargs printf "%d\n" 2>/dev/null || echo "0")
        local highest_block=$(echo "$sync_status" | jq -r '.highestBlock' | xargs printf "%d\n" 2>/dev/null || echo "0")
        if [[ "$highest_block" -gt 0 ]]; then
            local progress=$((current_block * 100 / highest_block))
            sync_status="Syncing ($progress%)"
        else
            sync_status="Syncing"
        fi
    else
        sync_status="Unknown"
    fi
    
    printf "║ %-20s: %-36s ║\n" "Network" "$NETWORK"
    printf "║ %-20s: %-36s ║\n" "Node Name" "$NODE_NAME"
    printf "║ %-20s: %-36s ║\n" "Block Number" "$block_number"
    printf "║ %-20s: %-36s ║\n" "Peer Count" "$peer_count"
    printf "║ %-20s: %-36s ║\n" "Sync Status" "$sync_status"
    printf "║ %-20s: %-36s ║\n" "RPC Endpoint" "http://localhost:$RPC_PORT"
    printf "║ %-20s: %-36s ║\n" "WebSocket Endpoint" "ws://localhost:$WS_PORT"
    
    if [[ "$MONITORING_ENABLED" == "true" ]]; then
        printf "║ %-20s: %-36s ║\n" "Metrics" "http://localhost:$METRICS_PORT/metrics"
        printf "║ %-20s: %-36s ║\n" "Grafana" "http://localhost:3000 (admin/testnet123)"
    fi
    
    echo "╚════════════════════════════════════════════════════════════╝"
    echo ""
}

# Cleanup function
cleanup() {
    log "Cleaning up..."
    
    cd "$CONFIG_DIR"
    docker-compose down
    docker-compose -f docker-compose-consensus.yml down
    
    if [[ "$MONITORING_ENABLED" == "true" ]]; then
        docker-compose -f docker-compose-monitoring.yml down
    fi
    
    docker network rm ethereum-testnet 2>/dev/null || true
    
    log "Cleanup completed"
}

# Main function
main() {
    show_banner
    
    case "${1:-deploy}" in
        deploy)
            check_prerequisites
            setup_directories
            generate_jwt_secret
            create_config
            setup_execution_client
            setup_consensus_client
            setup_monitoring
            deploy_node
            verify_deployment
            show_status
            
            info "Deployment completed! Monitor logs with:"
            info "  docker logs -f ${NODE_NAME}-execution"
            info "  docker logs -f ${NODE_NAME}-consensus"
            ;;
        
        status)
            show_status
            ;;
        
        stop)
            log "Stopping testnet node..."
            cd "$CONFIG_DIR"
            docker-compose stop
            docker-compose -f docker-compose-consensus.yml stop
            if [[ "$MONITORING_ENABLED" == "true" ]]; then
                docker-compose -f docker-compose-monitoring.yml stop
            fi
            log "Node stopped"
            ;;
        
        start)
            log "Starting testnet node..."
            cd "$CONFIG_DIR"
            docker-compose start
            docker-compose -f docker-compose-consensus.yml start
            if [[ "$MONITORING_ENABLED" == "true" ]]; then
                docker-compose -f docker-compose-monitoring.yml start
            fi
            sleep 5
            show_status
            ;;
        
        restart)
            $0 stop
            sleep 2
            $0 start
            ;;
        
        logs)
            docker logs -f "${NODE_NAME}-execution"
            ;;
        
        cleanup)
            read -p "This will remove all testnet data. Are you sure? (y/N) " -n 1 -r
            echo
            if [[ $REPLY =~ ^[Yy]$ ]]; then
                cleanup
                rm -rf "$DATA_DIR" "$CONFIG_DIR" "$LOG_DIR"
                log "All testnet data removed"
            fi
            ;;
        
        help)
            echo "Usage: $0 [command]"
            echo ""
            echo "Commands:"
            echo "  deploy   - Deploy new testnet node (default)"
            echo "  status   - Show node status"
            echo "  start    - Start stopped node"
            echo "  stop     - Stop running node"
            echo "  restart  - Restart node"
            echo "  logs     - Show execution client logs"
            echo "  cleanup  - Remove all testnet data"
            echo "  help     - Show this help message"
            ;;
        
        *)
            error "Unknown command: $1"
            ;;
    esac
}

# Handle errors
trap 'error "Script failed on line $LINENO"' ERR

# Run main function
main "$@"