#!/bin/bash

#############################################
# Ethereum Rust - Disaster Recovery Script
#############################################

set -euo pipefail

# Configuration
ETHEREUM_DATA_DIR="${ETHEREUM_DATA_DIR:-/data}"
BACKUP_DIR="${BACKUP_DIR:-/backup}"
RESTORE_DIR="${RESTORE_DIR:-/restore}"
LOG_FILE="${LOG_FILE:-/var/log/ethereum-rust-dr.log}"
NOTIFICATION_WEBHOOK="${NOTIFICATION_WEBHOOK:-}"
S3_BUCKET="${S3_BUCKET:-}"
ENCRYPTION_KEY="${ENCRYPTION_KEY:-}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Logging function
log() {
    echo -e "$(date '+%Y-%m-%d %H:%M:%S') - $1" | tee -a "$LOG_FILE"
}

# Error handling
error_exit() {
    echo -e "${RED}ERROR: $1${NC}" >&2
    send_notification "ERROR" "$1"
    exit 1
}

# Send notification
send_notification() {
    local level=$1
    local message=$2
    
    if [[ -n "$NOTIFICATION_WEBHOOK" ]]; then
        curl -X POST "$NOTIFICATION_WEBHOOK" \
            -H "Content-Type: application/json" \
            -d "{\"level\":\"$level\",\"message\":\"$message\",\"timestamp\":\"$(date -Iseconds)\"}" \
            2>/dev/null || true
    fi
}

# Check prerequisites
check_prerequisites() {
    log "${GREEN}Checking prerequisites...${NC}"
    
    # Check required tools
    local tools=("docker" "tar" "gzip" "openssl" "jq")
    for tool in "${tools[@]}"; do
        if ! command -v "$tool" &> /dev/null; then
            error_exit "$tool is required but not installed"
        fi
    done
    
    # Check AWS CLI if S3 is configured
    if [[ -n "$S3_BUCKET" ]]; then
        if ! command -v aws &> /dev/null; then
            error_exit "AWS CLI is required for S3 backup but not installed"
        fi
    fi
    
    # Check directories
    mkdir -p "$BACKUP_DIR" "$RESTORE_DIR" "$(dirname "$LOG_FILE")"
    
    log "${GREEN}Prerequisites check completed${NC}"
}

# Perform health check
health_check() {
    log "Performing health check..."
    
    # Check if node is running
    if ! docker ps | grep -q ethereum-rust; then
        return 1
    fi
    
    # Check RPC endpoint
    if ! curl -s -X POST http://localhost:8545 \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' \
        > /dev/null 2>&1; then
        return 1
    fi
    
    # Check disk space
    local available_space=$(df "$ETHEREUM_DATA_DIR" | awk 'NR==2 {print $4}')
    local required_space=$((50 * 1024 * 1024)) # 50GB in KB
    if [[ $available_space -lt $required_space ]]; then
        log "${YELLOW}Warning: Low disk space ($(($available_space / 1024 / 1024))GB available)${NC}"
    fi
    
    return 0
}

# Create backup
create_backup() {
    local backup_type=${1:-full}
    local timestamp=$(date +%Y%m%d_%H%M%S)
    local backup_name="ethereum-rust-backup-${backup_type}-${timestamp}"
    local backup_path="${BACKUP_DIR}/${backup_name}.tar.gz"
    
    log "${GREEN}Creating $backup_type backup: $backup_name${NC}"
    send_notification "INFO" "Starting $backup_type backup"
    
    # Stop node if full backup
    if [[ "$backup_type" == "full" ]]; then
        log "Stopping Ethereum node for full backup..."
        docker-compose stop ethereum-rust || true
        sleep 5
    fi
    
    # Create backup
    log "Creating backup archive..."
    if [[ "$backup_type" == "full" ]]; then
        tar czf "$backup_path" \
            -C "$ETHEREUM_DATA_DIR" \
            --exclude='*.log' \
            --exclude='*.tmp' \
            . 2>/dev/null
    else
        # Incremental backup - only critical files
        tar czf "$backup_path" \
            -C "$ETHEREUM_DATA_DIR" \
            --exclude='*.log' \
            --exclude='*.tmp' \
            --exclude='ancient/*' \
            state config 2>/dev/null || true
    fi
    
    # Encrypt backup if key provided
    if [[ -n "$ENCRYPTION_KEY" ]]; then
        log "Encrypting backup..."
        openssl enc -aes-256-cbc -salt -in "$backup_path" \
            -out "${backup_path}.enc" -pass pass:"$ENCRYPTION_KEY"
        rm "$backup_path"
        backup_path="${backup_path}.enc"
    fi
    
    # Calculate checksum
    local checksum=$(sha256sum "$backup_path" | awk '{print $1}')
    echo "$checksum" > "${backup_path}.sha256"
    
    # Upload to S3 if configured
    if [[ -n "$S3_BUCKET" ]]; then
        log "Uploading backup to S3..."
        aws s3 cp "$backup_path" "s3://${S3_BUCKET}/backups/" --storage-class GLACIER_IR
        aws s3 cp "${backup_path}.sha256" "s3://${S3_BUCKET}/backups/"
    fi
    
    # Restart node if it was stopped
    if [[ "$backup_type" == "full" ]]; then
        log "Restarting Ethereum node..."
        docker-compose start ethereum-rust
        
        # Wait for node to be healthy
        sleep 10
        if ! health_check; then
            error_exit "Node failed to restart after backup"
        fi
    fi
    
    # Clean old backups
    clean_old_backups
    
    log "${GREEN}Backup completed successfully: $backup_path${NC}"
    send_notification "SUCCESS" "Backup completed: $backup_name"
    
    # Return backup info
    echo "{\"name\":\"$backup_name\",\"path\":\"$backup_path\",\"checksum\":\"$checksum\",\"timestamp\":\"$timestamp\"}"
}

# Restore from backup
restore_backup() {
    local backup_name=${1:-latest}
    local backup_path=""
    
    log "${GREEN}Starting restore process...${NC}"
    send_notification "INFO" "Starting restore from backup: $backup_name"
    
    # Find backup file
    if [[ "$backup_name" == "latest" ]]; then
        backup_path=$(ls -t "${BACKUP_DIR}"/ethereum-rust-backup-*.tar.gz* 2>/dev/null | head -1)
        if [[ -z "$backup_path" ]]; then
            # Try S3 if no local backup
            if [[ -n "$S3_BUCKET" ]]; then
                log "No local backup found, checking S3..."
                local latest_s3=$(aws s3 ls "s3://${S3_BUCKET}/backups/" | sort | tail -1 | awk '{print $4}')
                if [[ -n "$latest_s3" ]]; then
                    backup_path="${BACKUP_DIR}/${latest_s3}"
                    aws s3 cp "s3://${S3_BUCKET}/backups/${latest_s3}" "$backup_path"
                    aws s3 cp "s3://${S3_BUCKET}/backups/${latest_s3}.sha256" "${backup_path}.sha256"
                fi
            fi
        fi
    else
        backup_path="${BACKUP_DIR}/${backup_name}"
        if [[ ! -f "$backup_path" ]] && [[ -n "$S3_BUCKET" ]]; then
            log "Downloading backup from S3..."
            aws s3 cp "s3://${S3_BUCKET}/backups/${backup_name}" "$backup_path"
            aws s3 cp "s3://${S3_BUCKET}/backups/${backup_name}.sha256" "${backup_path}.sha256"
        fi
    fi
    
    if [[ ! -f "$backup_path" ]]; then
        error_exit "Backup file not found: $backup_name"
    fi
    
    log "Using backup: $backup_path"
    
    # Verify checksum if available
    if [[ -f "${backup_path}.sha256" ]]; then
        log "Verifying backup integrity..."
        local expected_checksum=$(cat "${backup_path}.sha256")
        local actual_checksum=$(sha256sum "$backup_path" | awk '{print $1}')
        if [[ "$expected_checksum" != "$actual_checksum" ]]; then
            error_exit "Backup checksum verification failed"
        fi
        log "Checksum verified successfully"
    fi
    
    # Decrypt if encrypted
    local restore_file="$backup_path"
    if [[ "$backup_path" == *.enc ]]; then
        if [[ -z "$ENCRYPTION_KEY" ]]; then
            error_exit "Backup is encrypted but no encryption key provided"
        fi
        log "Decrypting backup..."
        restore_file="${backup_path%.enc}"
        openssl enc -aes-256-cbc -d -in "$backup_path" \
            -out "$restore_file" -pass pass:"$ENCRYPTION_KEY"
    fi
    
    # Stop node
    log "Stopping Ethereum node..."
    docker-compose stop ethereum-rust
    sleep 5
    
    # Backup current data
    log "Backing up current data..."
    if [[ -d "$ETHEREUM_DATA_DIR" ]]; then
        mv "$ETHEREUM_DATA_DIR" "${ETHEREUM_DATA_DIR}.backup.$(date +%Y%m%d_%H%M%S)"
    fi
    mkdir -p "$ETHEREUM_DATA_DIR"
    
    # Restore data
    log "Restoring data..."
    tar xzf "$restore_file" -C "$ETHEREUM_DATA_DIR"
    
    # Set proper permissions
    chown -R 1000:1000 "$ETHEREUM_DATA_DIR"
    
    # Start node
    log "Starting Ethereum node..."
    docker-compose start ethereum-rust
    
    # Wait for node to be healthy
    log "Waiting for node to become healthy..."
    local max_attempts=30
    local attempt=0
    while [[ $attempt -lt $max_attempts ]]; do
        if health_check; then
            log "${GREEN}Node is healthy${NC}"
            break
        fi
        sleep 10
        ((attempt++))
    done
    
    if [[ $attempt -ge $max_attempts ]]; then
        error_exit "Node failed to become healthy after restore"
    fi
    
    # Verify restore
    verify_restore
    
    log "${GREEN}Restore completed successfully${NC}"
    send_notification "SUCCESS" "Restore completed from: $backup_name"
}

# Verify restore
verify_restore() {
    log "Verifying restore..."
    
    # Check block number
    local block_number=$(curl -s -X POST http://localhost:8545 \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
        | jq -r '.result' | xargs printf "%d\n")
    
    if [[ $block_number -lt 1 ]]; then
        error_exit "Restore verification failed: invalid block number"
    fi
    
    log "Current block number: $block_number"
    
    # Check peer count
    local peer_count=$(curl -s -X POST http://localhost:8545 \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}' \
        | jq -r '.result' | xargs printf "%d\n")
    
    log "Connected peers: $peer_count"
    
    # Check sync status
    local syncing=$(curl -s -X POST http://localhost:8545 \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' \
        | jq -r '.result')
    
    if [[ "$syncing" != "false" ]]; then
        log "Node is syncing: $syncing"
    fi
    
    log "${GREEN}Restore verification completed${NC}"
}

# Clean old backups
clean_old_backups() {
    local retention_days=${BACKUP_RETENTION_DAYS:-30}
    
    log "Cleaning backups older than $retention_days days..."
    
    # Clean local backups
    find "$BACKUP_DIR" -name "ethereum-rust-backup-*.tar.gz*" \
        -mtime +$retention_days -delete 2>/dev/null || true
    
    # Clean S3 backups if configured
    if [[ -n "$S3_BUCKET" ]]; then
        local cutoff_date=$(date -d "$retention_days days ago" +%Y-%m-%d)
        aws s3 ls "s3://${S3_BUCKET}/backups/" | while read -r line; do
            local file_date=$(echo "$line" | awk '{print $1}')
            local file_name=$(echo "$line" | awk '{print $4}')
            if [[ "$file_date" < "$cutoff_date" ]]; then
                aws s3 rm "s3://${S3_BUCKET}/backups/${file_name}"
            fi
        done
    fi
    
    log "Cleanup completed"
}

# Test disaster recovery
test_disaster_recovery() {
    log "${GREEN}Starting disaster recovery test...${NC}"
    send_notification "INFO" "Starting DR test"
    
    # Create test backup
    log "Creating test backup..."
    local backup_info=$(create_backup "test")
    local backup_name=$(echo "$backup_info" | jq -r '.name')
    
    # Simulate disaster
    log "${YELLOW}Simulating disaster - corrupting data...${NC}"
    docker-compose stop ethereum-rust
    
    # Corrupt some data files (safely)
    if [[ -d "${ETHEREUM_DATA_DIR}/state" ]]; then
        echo "CORRUPTED" > "${ETHEREUM_DATA_DIR}/state/test_corruption"
    fi
    
    # Attempt restore
    log "Attempting restore..."
    restore_backup "${backup_name}.tar.gz"
    
    # Verify recovery
    if health_check; then
        log "${GREEN}Disaster recovery test PASSED${NC}"
        send_notification "SUCCESS" "DR test completed successfully"
        return 0
    else
        log "${RED}Disaster recovery test FAILED${NC}"
        send_notification "ERROR" "DR test failed"
        return 1
    fi
}

# Failover procedure
perform_failover() {
    local target_node=${1:-}
    
    log "${GREEN}Initiating failover procedure...${NC}"
    send_notification "WARNING" "Failover initiated to: $target_node"
    
    # Create final backup before failover
    create_backup "failover"
    
    # Stop primary node
    log "Stopping primary node..."
    docker-compose stop ethereum-rust
    
    if [[ -n "$target_node" ]]; then
        # Transfer data to failover node
        log "Transferring data to failover node: $target_node"
        rsync -avz --progress "$ETHEREUM_DATA_DIR/" "${target_node}:${ETHEREUM_DATA_DIR}/"
        
        # Start failover node
        ssh "$target_node" "cd /opt/ethereum-rust && docker-compose start ethereum-rust"
        
        # Update DNS/load balancer
        # This would be specific to your infrastructure
        log "Updating routing to failover node..."
    fi
    
    log "${GREEN}Failover completed${NC}"
    send_notification "SUCCESS" "Failover completed successfully"
}

# Recovery time objective (RTO) test
test_rto() {
    log "${GREEN}Testing Recovery Time Objective (RTO)...${NC}"
    
    local start_time=$(date +%s)
    
    # Simulate failure
    docker-compose stop ethereum-rust
    
    # Perform recovery
    restore_backup "latest"
    
    local end_time=$(date +%s)
    local recovery_time=$((end_time - start_time))
    
    log "Recovery completed in ${recovery_time} seconds"
    
    # Check against RTO target (e.g., 15 minutes)
    local rto_target=900
    if [[ $recovery_time -lt $rto_target ]]; then
        log "${GREEN}RTO test PASSED: ${recovery_time}s < ${rto_target}s${NC}"
        return 0
    else
        log "${RED}RTO test FAILED: ${recovery_time}s > ${rto_target}s${NC}"
        return 1
    fi
}

# Recovery point objective (RPO) test
test_rpo() {
    log "${GREEN}Testing Recovery Point Objective (RPO)...${NC}"
    
    # Get current block number
    local current_block=$(curl -s -X POST http://localhost:8545 \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
        | jq -r '.result' | xargs printf "%d\n")
    
    # Restore from latest backup
    restore_backup "latest"
    
    # Get restored block number
    local restored_block=$(curl -s -X POST http://localhost:8545 \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
        | jq -r '.result' | xargs printf "%d\n")
    
    local blocks_lost=$((current_block - restored_block))
    local time_lost=$((blocks_lost * 12)) # Assuming 12 second blocks
    
    log "Data loss: ${blocks_lost} blocks (~${time_lost} seconds)"
    
    # Check against RPO target (e.g., 1 hour)
    local rpo_target=3600
    if [[ $time_lost -lt $rpo_target ]]; then
        log "${GREEN}RPO test PASSED: ${time_lost}s < ${rpo_target}s${NC}"
        return 0
    else
        log "${RED}RPO test FAILED: ${time_lost}s > ${rpo_target}s${NC}"
        return 1
    fi
}

# Main menu
show_menu() {
    echo -e "${GREEN}════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}    Ethereum Rust - Disaster Recovery Tool     ${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════${NC}"
    echo ""
    echo "1) Create Full Backup"
    echo "2) Create Incremental Backup"
    echo "3) Restore from Backup"
    echo "4) Test Disaster Recovery"
    echo "5) Perform Failover"
    echo "6) Test RTO (Recovery Time Objective)"
    echo "7) Test RPO (Recovery Point Objective)"
    echo "8) Health Check"
    echo "9) Clean Old Backups"
    echo "0) Exit"
    echo ""
    echo -n "Select option: "
}

# Main function
main() {
    check_prerequisites
    
    if [[ $# -eq 0 ]]; then
        # Interactive mode
        while true; do
            show_menu
            read -r option
            
            case $option in
                1) create_backup "full" ;;
                2) create_backup "incremental" ;;
                3) 
                    echo -n "Enter backup name (or 'latest'): "
                    read -r backup_name
                    restore_backup "$backup_name"
                    ;;
                4) test_disaster_recovery ;;
                5) 
                    echo -n "Enter failover target node (optional): "
                    read -r target
                    perform_failover "$target"
                    ;;
                6) test_rto ;;
                7) test_rpo ;;
                8) 
                    if health_check; then
                        echo -e "${GREEN}Node is healthy${NC}"
                    else
                        echo -e "${RED}Node is unhealthy${NC}"
                    fi
                    ;;
                9) clean_old_backups ;;
                0) exit 0 ;;
                *) echo -e "${RED}Invalid option${NC}" ;;
            esac
            
            echo ""
            echo "Press Enter to continue..."
            read -r
        done
    else
        # Command line mode
        case "$1" in
            backup)
                create_backup "${2:-full}"
                ;;
            restore)
                restore_backup "${2:-latest}"
                ;;
            test)
                test_disaster_recovery
                ;;
            failover)
                perform_failover "${2:-}"
                ;;
            test-rto)
                test_rto
                ;;
            test-rpo)
                test_rpo
                ;;
            health)
                if health_check; then
                    echo "OK"
                    exit 0
                else
                    echo "FAIL"
                    exit 1
                fi
                ;;
            clean)
                clean_old_backups
                ;;
            *)
                echo "Usage: $0 [backup|restore|test|failover|test-rto|test-rpo|health|clean]"
                exit 1
                ;;
        esac
    fi
}

# Run main function
main "$@"