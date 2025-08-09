use ethereum_types::{H256, H512};
use secp256k1::{PublicKey, SecretKey, Secp256k1};
use std::net::{SocketAddr, IpAddr};
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tokio::time;
// use bytes::{Bytes, BytesMut, BufMut}; // Unused imports
use rand::Rng;
// use sha2::{Sha256, Digest}; // Unused imports

use crate::{Result, NetworkError};

const PROTOCOL_VERSION: u32 = 4;
const BUCKET_SIZE: usize = 16;
const ALPHA: usize = 3; // Concurrency parameter
const MAX_NODES: usize = 10000;
const PING_INTERVAL: Duration = Duration::from_secs(60);
const EXPIRATION_TIME: Duration = Duration::from_secs(20);

#[derive(Debug, Clone)]
pub struct NodeId {
    pub id: H512,
    pub address: SocketAddr,
    pub public_key: PublicKey,
}

impl NodeId {
    pub fn new(public_key: PublicKey, address: SocketAddr) -> Self {
        let id = public_key_to_node_id(&public_key);
        Self {
            id,
            address,
            public_key,
        }
    }
    
    pub fn distance(&self, other: &H512) -> H512 {
        let mut result = [0u8; 64];
        for i in 0..64 {
            result[i] = self.id.as_bytes()[i] ^ other.as_bytes()[i];
        }
        H512::from(result)
    }
    
    pub fn log_distance(&self, other: &H512) -> Option<usize> {
        let distance = self.distance(other);
        for i in 0..512 {
            let byte_idx = i / 8;
            let bit_idx = 7 - (i % 8);
            if (distance.as_bytes()[byte_idx] >> bit_idx) & 1 == 1 {
                return Some(511 - i);
            }
        }
        None
    }
}

#[derive(Debug)]
pub struct KBucket {
    nodes: VecDeque<NodeId>,
    capacity: usize,
}

impl KBucket {
    pub fn new(capacity: usize) -> Self {
        Self {
            nodes: VecDeque::new(),
            capacity,
        }
    }
    
    pub fn add(&mut self, node: NodeId) -> bool {
        // Check if node already exists
        if let Some(pos) = self.nodes.iter().position(|n| n.id == node.id) {
            // Move to front (most recently seen)
            self.nodes.remove(pos);
            self.nodes.push_front(node);
            return true;
        }
        
        // Add new node if space available
        if self.nodes.len() < self.capacity {
            self.nodes.push_front(node);
            return true;
        }
        
        false
    }
    
    pub fn remove(&mut self, id: &H512) {
        self.nodes.retain(|n| n.id != *id);
    }
    
    pub fn get_nodes(&self) -> Vec<NodeId> {
        self.nodes.iter().cloned().collect()
    }
}

pub struct RoutingTable {
    buckets: Vec<RwLock<KBucket>>,
    local_id: H512,
}

impl RoutingTable {
    pub fn new(local_id: H512) -> Self {
        let mut buckets = Vec::new();
        for _ in 0..256 {
            buckets.push(RwLock::new(KBucket::new(BUCKET_SIZE)));
        }
        
        Self {
            buckets,
            local_id,
        }
    }
    
    pub async fn add_node(&self, node: NodeId) {
        if node.id == self.local_id {
            return;
        }
        
        if let Some(bucket_idx) = self.bucket_index(&node.id) {
            let mut bucket = self.buckets[bucket_idx].write().await;
            bucket.add(node);
        }
    }
    
    pub async fn remove_node(&self, id: &H512) {
        if let Some(bucket_idx) = self.bucket_index(id) {
            let mut bucket = self.buckets[bucket_idx].write().await;
            bucket.remove(id);
        }
    }
    
    pub async fn find_nearest(&self, target: &H512, count: usize) -> Vec<NodeId> {
        let mut nodes = Vec::new();
        
        // Collect nodes from all buckets
        for bucket in &self.buckets {
            let bucket = bucket.read().await;
            nodes.extend(bucket.get_nodes());
        }
        
        // Sort by distance to target
        nodes.sort_by_key(|n| n.distance(target));
        nodes.truncate(count);
        nodes
    }
    
    fn bucket_index(&self, id: &H512) -> Option<usize> {
        let node = NodeId {
            id: self.local_id,
            address: "0.0.0.0:0".parse().unwrap(),
            public_key: PublicKey::from_slice(&[0x04; 65]).unwrap(),
        };
        node.log_distance(id)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Message {
    Ping {
        from: NodeEndpoint,
        to: NodeEndpoint,
        expiration: u64,
    },
    Pong {
        to: NodeEndpoint,
        ping_hash: H256,
        expiration: u64,
    },
    FindNode {
        target: H512,
        expiration: u64,
    },
    Neighbors {
        nodes: Vec<NodeEndpoint>,
        expiration: u64,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeEndpoint {
    pub address: IpAddr,
    pub udp_port: u16,
    pub tcp_port: u16,
}

impl NodeEndpoint {
    pub fn to_socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.address, self.udp_port)
    }
}

pub struct Discovery {
    secret_key: SecretKey,
    public_key: PublicKey,
    node_id: H512,
    socket: Arc<UdpSocket>,
    routing_table: Arc<RoutingTable>,
    pending_pings: Arc<RwLock<HashMap<H256, NodeId>>>,
    pending_finds: Arc<RwLock<HashMap<H256, (H512, Vec<NodeId>)>>>,
}

impl Discovery {
    pub async fn new(
        secret_key: SecretKey,
        listen_addr: SocketAddr,
    ) -> Result<Self> {
        let secp = Secp256k1::new();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        let node_id = public_key_to_node_id(&public_key);
        
        let socket = Arc::new(UdpSocket::bind(listen_addr).await?);
        let routing_table = Arc::new(RoutingTable::new(node_id));
        
        Ok(Self {
            secret_key,
            public_key,
            node_id,
            socket,
            routing_table,
            pending_pings: Arc::new(RwLock::new(HashMap::new())),
            pending_finds: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    pub async fn bootstrap(&self, nodes: Vec<NodeId>) -> Result<()> {
        for node in nodes {
            self.ping_node(node).await?;
        }
        Ok(())
    }
    
    pub async fn run(self: Arc<Self>) {
        // Start message handler
        let handler = self.clone();
        tokio::spawn(async move {
            handler.handle_messages().await;
        });
        
        // Start maintenance tasks
        let maintenance = self.clone();
        tokio::spawn(async move {
            maintenance.run_maintenance().await;
        });
        
        // Start node discovery
        let discovery = self.clone();
        tokio::spawn(async move {
            discovery.run_discovery().await;
        });
    }
    
    async fn handle_messages(&self) {
        let mut buf = vec![0u8; 1280];
        
        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    if let Err(e) = self.handle_packet(&buf[..len], addr).await {
                        tracing::debug!("Failed to handle packet from {}: {}", addr, e);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to receive packet: {}", e);
                }
            }
        }
    }
    
    async fn handle_packet(&self, data: &[u8], from: SocketAddr) -> Result<()> {
        let (msg_type, msg_data, node_id) = self.decode_packet(data)?;
        
        match msg_type {
            0x01 => self.handle_ping(msg_data, from, node_id).await?,
            0x02 => self.handle_pong(msg_data, from, node_id).await?,
            0x03 => self.handle_find_node(msg_data, from, node_id).await?,
            0x04 => self.handle_neighbors(msg_data, from, node_id).await?,
            _ => return Err(NetworkError::InvalidMessage("Unknown message type".to_string())),
        }
        
        Ok(())
    }
    
    async fn handle_ping(&self, data: &[u8], from: SocketAddr, node_id: H512) -> Result<()> {
        let msg: Message = decode_message(data)?;
        
        if let Message::Ping { from: from_endpoint, to: _, expiration } = msg {
            // Check expiration
            if is_expired(expiration) {
                return Ok(());
            }
            
            // Send pong
            let pong = Message::Pong {
                to: from_endpoint.clone(),
                ping_hash: ethereum_crypto::keccak256(data),
                expiration: future_expiration(),
            };
            
            self.send_message(&pong, from).await?;
            
            // Add to routing table
            if let Ok(public_key) = node_id_to_public_key(&node_id) {
                let node = NodeId::new(public_key, from);
                self.routing_table.add_node(node).await;
            }
        }
        
        Ok(())
    }
    
    async fn handle_pong(&self, data: &[u8], _from: SocketAddr, _node_id: H512) -> Result<()> {
        let msg: Message = decode_message(data)?;
        
        if let Message::Pong { ping_hash, expiration, .. } = msg {
            // Check expiration
            if is_expired(expiration) {
                return Ok(());
            }
            
            // Check if we have a pending ping
            let mut pending = self.pending_pings.write().await;
            if let Some(node) = pending.remove(&ping_hash) {
                // Add to routing table
                self.routing_table.add_node(node).await;
            }
        }
        
        Ok(())
    }
    
    async fn handle_find_node(&self, data: &[u8], from: SocketAddr, _node_id: H512) -> Result<()> {
        let msg: Message = decode_message(data)?;
        
        if let Message::FindNode { target, expiration } = msg {
            // Check expiration
            if is_expired(expiration) {
                return Ok(());
            }
            
            // Find nearest nodes
            let nodes = self.routing_table.find_nearest(&target, BUCKET_SIZE).await;
            
            // Convert to endpoints
            let endpoints: Vec<NodeEndpoint> = nodes
                .iter()
                .map(|n| NodeEndpoint {
                    address: n.address.ip(),
                    udp_port: n.address.port(),
                    tcp_port: n.address.port(), // Same as UDP for now
                })
                .collect();
            
            // Send neighbors
            let neighbors = Message::Neighbors {
                nodes: endpoints,
                expiration: future_expiration(),
            };
            
            self.send_message(&neighbors, from).await?;
        }
        
        Ok(())
    }
    
    async fn handle_neighbors(&self, data: &[u8], _from: SocketAddr, _node_id: H512) -> Result<()> {
        let msg: Message = decode_message(data)?;
        
        if let Message::Neighbors { nodes, expiration } = msg {
            // Check expiration
            if is_expired(expiration) {
                return Ok(());
            }
            
            // Process received nodes
            for endpoint in nodes {
                // Ping each new node
                if let Ok(public_key) = endpoint_to_public_key(&endpoint) {
                    let node = NodeId::new(public_key, endpoint.to_socket_addr());
                    self.ping_node(node).await?;
                }
            }
        }
        
        Ok(())
    }
    
    async fn ping_node(&self, node: NodeId) -> Result<()> {
        let from = NodeEndpoint {
            address: "0.0.0.0".parse().unwrap(), // Will be filled by receiver
            udp_port: 30303,
            tcp_port: 30303,
        };
        
        let to = NodeEndpoint {
            address: node.address.ip(),
            udp_port: node.address.port(),
            tcp_port: node.address.port(),
        };
        
        let ping = Message::Ping {
            from,
            to,
            expiration: future_expiration(),
        };
        
        let msg_bytes = encode_message(&ping)?;
        let ping_hash = ethereum_crypto::keccak256(&msg_bytes);
        
        // Store pending ping
        let mut pending = self.pending_pings.write().await;
        pending.insert(ping_hash, node.clone());
        
        self.send_message(&ping, node.address).await?;
        
        Ok(())
    }
    
    async fn find_node(&self, target: H512) -> Result<Vec<NodeId>> {
        let nearest = self.routing_table.find_nearest(&target, ALPHA).await;
        
        for node in &nearest {
            let find = Message::FindNode {
                target,
                expiration: future_expiration(),
            };
            
            self.send_message(&find, node.address).await?;
        }
        
        // Wait for responses (simplified)
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        Ok(nearest)
    }
    
    async fn send_message(&self, msg: &Message, to: SocketAddr) -> Result<()> {
        let packet = self.encode_packet(msg)?;
        self.socket.send_to(&packet, to).await?;
        Ok(())
    }
    
    fn encode_packet(&self, msg: &Message) -> Result<Vec<u8>> {
        let msg_bytes = encode_message(msg)?;
        let msg_hash = ethereum_crypto::keccak256(&msg_bytes);
        
        // Sign the message hash
        let secp = Secp256k1::new();
        let message = secp256k1::Message::from_slice(&msg_hash[..])
            .map_err(|e| NetworkError::CryptoError(e.to_string()))?;
        let sig = secp.sign_ecdsa_recoverable(&message, &self.secret_key);
        let (recovery_id, sig_bytes) = sig.serialize_compact();
        
        // Build packet
        let mut packet = Vec::new();
        packet.extend_from_slice(&msg_hash[..]);
        packet.extend_from_slice(&sig_bytes);
        packet.push(recovery_id.to_i32() as u8);
        packet.push(msg_type_id(msg));
        packet.extend_from_slice(&msg_bytes);
        
        Ok(packet)
    }
    
    fn decode_packet<'a>(&self, data: &'a [u8]) -> Result<(u8, &'a [u8], H512)> {
        if data.len() < 98 {
            return Err(NetworkError::InvalidMessage("Packet too short".to_string()));
        }
        
        let msg_hash = &data[0..32];
        let signature = &data[32..96];
        let recovery_id = data[96];
        let msg_type = data[97];
        let msg_data = &data[98..];
        
        // Verify hash
        let computed_hash = ethereum_crypto::keccak256(msg_data);
        if &computed_hash[..] != msg_hash {
            return Err(NetworkError::InvalidMessage("Invalid message hash".to_string()));
        }
        
        // Recover public key from signature
        let secp = Secp256k1::new();
        let message = secp256k1::Message::from_slice(msg_hash)
            .map_err(|e| NetworkError::CryptoError(e.to_string()))?;
        let recovery_id = secp256k1::ecdsa::RecoveryId::from_i32(recovery_id as i32)
            .map_err(|e| NetworkError::CryptoError(e.to_string()))?;
        let sig = secp256k1::ecdsa::RecoverableSignature::from_compact(&signature[0..64], recovery_id)
            .map_err(|e| NetworkError::CryptoError(e.to_string()))?;
        let public_key = secp.recover_ecdsa(&message, &sig)
            .map_err(|e| NetworkError::CryptoError(e.to_string()))?;
        
        let node_id = public_key_to_node_id(&public_key);
        
        Ok((msg_type, msg_data, node_id))
    }
    
    async fn run_maintenance(&self) {
        let mut interval = time::interval(PING_INTERVAL);
        
        loop {
            interval.tick().await;
            
            // Ping random nodes to keep routing table fresh
            let nodes = self.routing_table.find_nearest(&random_node_id(), ALPHA).await;
            for node in nodes {
                if let Err(e) = self.ping_node(node).await {
                    tracing::debug!("Failed to ping node: {}", e);
                }
            }
        }
    }
    
    async fn run_discovery(&self) {
        let mut interval = time::interval(Duration::from_secs(30));
        
        loop {
            interval.tick().await;
            
            // Discover new nodes by searching for random IDs
            let target = random_node_id();
            if let Err(e) = self.find_node(target).await {
                tracing::debug!("Failed to find node: {}", e);
            }
        }
    }
}

fn public_key_to_node_id(public_key: &PublicKey) -> H512 {
    let serialized = public_key.serialize_uncompressed();
    H512::from_slice(&ethereum_crypto::keccak256(&serialized[1..])[..])
}

fn node_id_to_public_key(_node_id: &H512) -> Result<PublicKey> {
    // This is a simplified version - in reality, we'd need to store the mapping
    Err(NetworkError::InvalidMessage("Cannot recover public key from node ID".to_string()))
}

fn endpoint_to_public_key(_endpoint: &NodeEndpoint) -> Result<PublicKey> {
    // This is a simplified version - in reality, we'd need to get this from the node
    Err(NetworkError::InvalidMessage("Cannot get public key from endpoint".to_string()))
}

fn encode_message(msg: &Message) -> Result<Vec<u8>> {
    // Simplified encoding - real implementation would use RLP
    Ok(bincode::serialize(msg).map_err(|e| NetworkError::InvalidMessage(e.to_string()))?)
}

fn decode_message(data: &[u8]) -> Result<Message> {
    // Simplified decoding - real implementation would use RLP
    Ok(bincode::deserialize(data).map_err(|e| NetworkError::InvalidMessage(e.to_string()))?)
}

fn msg_type_id(msg: &Message) -> u8 {
    match msg {
        Message::Ping { .. } => 0x01,
        Message::Pong { .. } => 0x02,
        Message::FindNode { .. } => 0x03,
        Message::Neighbors { .. } => 0x04,
    }
}

fn future_expiration() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() + EXPIRATION_TIME.as_secs()
}

fn is_expired(expiration: u64) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    now > expiration
}

fn random_node_id() -> H512 {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 64];
    rng.fill(&mut bytes);
    H512::from(bytes)
}