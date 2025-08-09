# Ethereum Rust Deployment Guide

## System Requirements

### Minimum Requirements
- CPU: 4 cores @ 2.0 GHz
- RAM: 8 GB
- Storage: 500 GB SSD
- Network: 25 Mbps bandwidth
- OS: Linux (Ubuntu 20.04+, Debian 11+, RHEL 8+)

### Recommended Requirements
- CPU: 8 cores @ 3.0 GHz
- RAM: 16 GB
- Storage: 2 TB NVMe SSD
- Network: 100 Mbps bandwidth
- OS: Linux (Ubuntu 22.04 LTS)

## Installation Methods

### Binary Installation

Download pre-built binaries from releases:

```bash
# Download latest release
wget https://github.com/ethereum-rust/ethereum-rust/releases/latest/download/ethereum-rust-linux-amd64.tar.gz

# Extract
tar -xzf ethereum-rust-linux-amd64.tar.gz

# Install
sudo mv ethereum-rust /usr/local/bin/
sudo chmod +x /usr/local/bin/ethereum-rust

# Verify installation
ethereum-rust --version
```

### Build from Source

```bash
# Install dependencies
sudo apt update
sudo apt install -y build-essential cmake libssl-dev pkg-config libclang-dev

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Clone repository
git clone https://github.com/ethereum-rust/ethereum-rust.git
cd ethereum-rust

# Build release binary
cargo build --release

# Install
sudo cp target/release/ethereum-rust /usr/local/bin/
```

### Docker Installation

```bash
# Pull Docker image
docker pull ethereumrust/node:latest

# Run container
docker run -d \
  --name ethereum-rust \
  -p 8545:8545 \
  -p 8546:8546 \
  -p 30303:30303 \
  -p 30303:30303/udp \
  -p 9090:9090 \
  -v /data/ethereum:/data \
  ethereumrust/node:latest
```

## Configuration

### Basic Configuration

Create `/etc/ethereum-rust/config.toml`:

```toml
[node]
data_dir = "/var/lib/ethereum-rust"
network_id = 1  # 1=mainnet, 5=goerli, 11155111=sepolia
chain = "mainnet"
sync_mode = "fast"
cache_size = 2048  # MB

[network]
listen_addr = "0.0.0.0:30303"
external_addr = "YOUR_PUBLIC_IP:30303"
max_peers = 50
discovery = true
bootnodes = [
    "enode://d860a01f9722d78051619d1e2351aba3f43f943f6f00718d1b9baa4101932a1f5011f16bb2b1bb35db20d6fe28fa0bf09636d26a87d31de9ec6203eeedb1f666@18.138.108.67:30303",
    "enode://22a8232c3abc76a16ae9d6c3b164f98775fe226f0917b0ca871128a74a8e9630b458460865bab457221f1d448dd9791d24c4e5d88786180ac185df813a68d4de@3.209.45.79:30303"
]

[rpc]
enabled = true
host = "127.0.0.1"
port = 8545
ws_enabled = true
ws_port = 8546
apis = ["eth", "net", "web3", "debug", "trace"]
cors_origins = ["http://localhost:3000"]
max_connections = 100

[txpool]
max_pending = 4096
max_queued = 1024
min_gas_price = 1000000000  # 1 gwei
gas_price_bump = 10  # percent

[metrics]
enabled = true
host = "0.0.0.0"
port = 9090
```

### Advanced Configuration

#### Performance Tuning

```toml
[performance]
db_cache_size = 4096  # MB
state_cache_size = 2048  # MB
trie_cache_gens = 120
parallel_evm_threads = 4
batch_size = 5000

[database]
backend = "rocksdb"
path = "/var/lib/ethereum-rust/chaindata"
compression = "lz4"
write_buffer_size = 64  # MB
max_open_files = 10000
```

#### Security Configuration

```toml
[security]
enable_tls = true
tls_cert = "/etc/ethereum-rust/cert.pem"
tls_key = "/etc/ethereum-rust/key.pem"
jwt_secret = "/etc/ethereum-rust/jwt.secret"
allowed_hosts = ["localhost", "127.0.0.1"]
rate_limit = 100  # requests per second
```

## Systemd Service

Create `/etc/systemd/system/ethereum-rust.service`:

```ini
[Unit]
Description=Ethereum Rust Node
After=network.target

[Service]
Type=simple
User=ethereum
Group=ethereum
ExecStart=/usr/local/bin/ethereum-rust --config /etc/ethereum-rust/config.toml
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal
SyslogIdentifier=ethereum-rust

# Security
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/ethereum-rust

# Resource limits
LimitNOFILE=65536
LimitNPROC=4096

[Install]
WantedBy=multi-user.target
```

Enable and start service:

```bash
# Create user
sudo useradd -r -s /bin/false ethereum

# Create directories
sudo mkdir -p /var/lib/ethereum-rust
sudo mkdir -p /etc/ethereum-rust
sudo chown -R ethereum:ethereum /var/lib/ethereum-rust

# Enable service
sudo systemctl daemon-reload
sudo systemctl enable ethereum-rust
sudo systemctl start ethereum-rust

# Check status
sudo systemctl status ethereum-rust
sudo journalctl -u ethereum-rust -f
```

## Docker Compose

Create `docker-compose.yml`:

```yaml
version: '3.8'

services:
  ethereum-rust:
    image: ethereumrust/node:latest
    container_name: ethereum-rust
    restart: unless-stopped
    ports:
      - "8545:8545"  # HTTP RPC
      - "8546:8546"  # WebSocket RPC
      - "30303:30303"  # P2P TCP
      - "30303:30303/udp"  # P2P UDP
      - "9090:9090"  # Metrics
    volumes:
      - ./data:/data
      - ./config.toml:/config.toml:ro
    command: ["--config", "/config.toml"]
    logging:
      driver: "json-file"
      options:
        max-size: "10m"
        max-file: "10"
    deploy:
      resources:
        limits:
          cpus: '4'
          memory: 8G
        reservations:
          cpus: '2'
          memory: 4G

  prometheus:
    image: prom/prometheus:latest
    container_name: prometheus
    restart: unless-stopped
    ports:
      - "9091:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - prometheus_data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'

  grafana:
    image: grafana/grafana:latest
    container_name: grafana
    restart: unless-stopped
    ports:
      - "3000:3000"
    volumes:
      - grafana_data:/var/lib/grafana
      - ./grafana/dashboards:/etc/grafana/provisioning/dashboards:ro
      - ./grafana/datasources:/etc/grafana/provisioning/datasources:ro
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
      - GF_USERS_ALLOW_SIGN_UP=false

volumes:
  prometheus_data:
  grafana_data:
```

## Kubernetes Deployment

### Deployment Manifest

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: ethereum-rust
  namespace: blockchain
spec:
  replicas: 1
  selector:
    matchLabels:
      app: ethereum-rust
  template:
    metadata:
      labels:
        app: ethereum-rust
    spec:
      containers:
      - name: ethereum-rust
        image: ethereumrust/node:latest
        ports:
        - containerPort: 8545
          name: rpc
        - containerPort: 8546
          name: ws
        - containerPort: 30303
          name: p2p-tcp
        - containerPort: 30303
          protocol: UDP
          name: p2p-udp
        - containerPort: 9090
          name: metrics
        volumeMounts:
        - name: data
          mountPath: /data
        - name: config
          mountPath: /config.toml
          subPath: config.toml
        resources:
          requests:
            memory: "8Gi"
            cpu: "2"
          limits:
            memory: "16Gi"
            cpu: "4"
        livenessProbe:
          httpGet:
            path: /health/live
            port: 9090
          initialDelaySeconds: 60
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /health/ready
            port: 9090
          initialDelaySeconds: 30
          periodSeconds: 10
      volumes:
      - name: data
        persistentVolumeClaim:
          claimName: ethereum-rust-pvc
      - name: config
        configMap:
          name: ethereum-rust-config
```

### Service Definition

```yaml
apiVersion: v1
kind: Service
metadata:
  name: ethereum-rust
  namespace: blockchain
spec:
  selector:
    app: ethereum-rust
  ports:
  - port: 8545
    targetPort: 8545
    name: rpc
  - port: 8546
    targetPort: 8546
    name: ws
  - port: 30303
    targetPort: 30303
    protocol: TCP
    name: p2p-tcp
  - port: 30303
    targetPort: 30303
    protocol: UDP
    name: p2p-udp
  - port: 9090
    targetPort: 9090
    name: metrics
  type: LoadBalancer
```

## Monitoring Setup

### Prometheus Configuration

Create `prometheus.yml`:

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'ethereum-rust'
    static_configs:
      - targets: ['ethereum-rust:9090']
    metrics_path: '/metrics'
```

### Alerting Rules

Create `alerts.yml`:

```yaml
groups:
- name: ethereum-rust
  interval: 30s
  rules:
  - alert: NodeDown
    expr: up{job="ethereum-rust"} == 0
    for: 5m
    annotations:
      summary: "Ethereum node is down"
      description: "Node {{ $labels.instance }} has been down for more than 5 minutes"

  - alert: LowPeerCount
    expr: ethereum_peers_connected < 3
    for: 10m
    annotations:
      summary: "Low peer count"
      description: "Node has less than 3 peers for more than 10 minutes"

  - alert: HighCPUUsage
    expr: ethereum_process_cpu_usage_percent > 90
    for: 5m
    annotations:
      summary: "High CPU usage"
      description: "CPU usage is above 90% for more than 5 minutes"

  - alert: SyncLag
    expr: ethereum_sync_highest_block - ethereum_sync_current_block > 100
    for: 30m
    annotations:
      summary: "Node is lagging behind"
      description: "Node is more than 100 blocks behind for more than 30 minutes"
```

## Backup and Recovery

### Backup Strategy

```bash
#!/bin/bash
# backup.sh

DATA_DIR="/var/lib/ethereum-rust"
BACKUP_DIR="/backup/ethereum-rust"
DATE=$(date +%Y%m%d_%H%M%S)

# Stop node
systemctl stop ethereum-rust

# Create backup
tar -czf "$BACKUP_DIR/backup_$DATE.tar.gz" -C "$DATA_DIR" .

# Start node
systemctl start ethereum-rust

# Keep only last 7 days
find "$BACKUP_DIR" -name "backup_*.tar.gz" -mtime +7 -delete
```

### Recovery Process

```bash
# Stop node
systemctl stop ethereum-rust

# Restore from backup
tar -xzf /backup/ethereum-rust/backup_20240101_120000.tar.gz -C /var/lib/ethereum-rust/

# Start node
systemctl start ethereum-rust
```

## Security Best Practices

1. **Firewall Configuration**
   ```bash
   # Allow P2P
   sudo ufw allow 30303/tcp
   sudo ufw allow 30303/udp
   
   # Allow RPC only from specific IPs
   sudo ufw allow from 192.168.1.0/24 to any port 8545
   
   # Allow metrics only locally
   sudo ufw allow from 127.0.0.1 to any port 9090
   ```

2. **SSL/TLS for RPC**
   ```nginx
   server {
       listen 443 ssl;
       server_name rpc.example.com;
       
       ssl_certificate /etc/ssl/certs/rpc.crt;
       ssl_certificate_key /etc/ssl/private/rpc.key;
       
       location / {
           proxy_pass http://127.0.0.1:8545;
           proxy_set_header Host $host;
           proxy_set_header X-Real-IP $remote_addr;
       }
   }
   ```

3. **JWT Authentication**
   ```bash
   # Generate JWT secret
   openssl rand -hex 32 > /etc/ethereum-rust/jwt.secret
   chmod 600 /etc/ethereum-rust/jwt.secret
   chown ethereum:ethereum /etc/ethereum-rust/jwt.secret
   ```

## Troubleshooting

### Common Issues

1. **Node not syncing**
   - Check peer count: `curl http://localhost:8545 -X POST -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}'`
   - Check firewall rules
   - Verify bootnodes are correct

2. **High disk usage**
   - Enable pruning in config
   - Check log rotation
   - Consider using archive node only if necessary

3. **Memory issues**
   - Reduce cache sizes
   - Limit peer count
   - Enable swap (not recommended for production)

### Logs

```bash
# View logs
journalctl -u ethereum-rust -f

# Export logs
journalctl -u ethereum-rust --since "1 hour ago" > debug.log

# Check disk usage
du -sh /var/lib/ethereum-rust/*

# Monitor resources
htop -p $(pgrep ethereum-rust)
```

## Performance Optimization

1. **Database Optimization**
   ```toml
   [database]
   cache_size = 8192  # Increase for better performance
   write_buffer_size = 128
   compaction_style = "level"
   ```

2. **Network Optimization**
   ```bash
   # Increase file descriptors
   echo "ethereum soft nofile 65536" >> /etc/security/limits.conf
   echo "ethereum hard nofile 65536" >> /etc/security/limits.conf
   
   # TCP optimization
   sysctl -w net.core.somaxconn=1024
   sysctl -w net.ipv4.tcp_max_syn_backlog=2048
   ```

3. **Storage Optimization**
   - Use NVMe SSD
   - Enable TRIM for SSDs
   - Use separate disk for database

## Maintenance

### Regular Tasks

- **Daily**: Check logs for errors
- **Weekly**: Verify backup completion
- **Monthly**: Update to latest version
- **Quarterly**: Review security settings

### Upgrade Process

```bash
# Backup current version
cp /usr/local/bin/ethereum-rust /usr/local/bin/ethereum-rust.backup

# Download new version
wget https://github.com/ethereum-rust/ethereum-rust/releases/latest/download/ethereum-rust-linux-amd64.tar.gz
tar -xzf ethereum-rust-linux-amd64.tar.gz

# Stop service
systemctl stop ethereum-rust

# Install new version
mv ethereum-rust /usr/local/bin/
chmod +x /usr/local/bin/ethereum-rust

# Start service
systemctl start ethereum-rust

# Verify
ethereum-rust --version
systemctl status ethereum-rust
```