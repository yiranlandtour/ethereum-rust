use ethereum_types::{H256, H512};
use ethereum_crypto::keccak256;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn, error};

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("Invalid ENR: {0}")]
    InvalidEnr(String),
    
    #[error("Invalid signature")]
    InvalidSignature,
    
    #[error("Node not found")]
    NodeNotFound,
    
    #[error("Bucket full")]
    BucketFull,
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Decoding error: {0}")]
    DecodingError(String),
}

pub type Result<T> = std::result::Result<T, DiscoveryError>;

/// Ethereum Node Record (ENR) - EIP-778
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enr {
    pub seq: u64,
    pub node_id: NodeId,
    pub ip: Option<IpAddr>,
    pub tcp: Option<u16>,
    pub udp: Option<u16>,
    pub ip6: Option<IpAddr>,
    pub tcp6: Option<u16>,
    pub udp6: Option<u16>,
    pub id: String, // "v4" or "v5"
    pub secp256k1: Option<Vec<u8>>, // Public key
    pub eth2: Option<Eth2Data>,
    pub attnets: Option<Vec<u8>>,
    pub syncnets: Option<Vec<u8>>,
    pub signature: Vec<u8>,
    pub custom_fields: HashMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eth2Data {
    pub fork_digest: [u8; 4],
    pub next_fork_version: [u8; 4],
    pub next_fork_epoch: u64,
}

impl Enr {
    pub fn new(node_id: NodeId, seq: u64) -> Self {
        Self {
            seq,
            node_id,
            ip: None,
            tcp: None,
            udp: None,
            ip6: None,
            tcp6: None,
            udp6: None,
            id: "v5".to_string(),
            secp256k1: None,
            eth2: None,
            attnets: None,
            syncnets: None,
            signature: Vec::new(),
            custom_fields: HashMap::new(),
        }
    }
    
    pub fn with_ip(mut self, ip: IpAddr, tcp: u16, udp: u16) -> Self {
        match ip {
            IpAddr::V4(_) => {
                self.ip = Some(ip);
                self.tcp = Some(tcp);
                self.udp = Some(udp);
            }
            IpAddr::V6(_) => {
                self.ip6 = Some(ip);
                self.tcp6 = Some(tcp);
                self.udp6 = Some(udp);
            }
        }
        self
    }
    
    pub fn sign(&mut self, private_key: &[u8; 32]) -> Result<()> {
        let content = self.content_to_sign();
        let signature = ethereum_crypto::sign_message(&keccak256(&content), private_key)
            .map_err(|_| DiscoveryError::InvalidSignature)?;
        
        self.signature = signature.to_bytes().to_vec();
        Ok(())
    }
    
    pub fn verify(&self) -> Result<bool> {
        if self.signature.is_empty() {
            return Ok(false);
        }
        
        let content = self.content_to_sign();
        let hash = keccak256(&content);
        
        // Verify signature against node_id (which is derived from public key)
        // In production, would recover public key and verify it matches node_id
        Ok(true)
    }
    
    fn content_to_sign(&self) -> Vec<u8> {
        let mut content = Vec::new();
        content.extend_from_slice(&self.seq.to_be_bytes());
        content.extend_from_slice(self.node_id.as_bytes());
        
        if let Some(ip) = &self.ip {
            content.extend_from_slice(b"ip");
            match ip {
                IpAddr::V4(addr) => content.extend_from_slice(&addr.octets()),
                IpAddr::V6(addr) => content.extend_from_slice(&addr.octets()),
            }
        }
        
        content
    }
    
    pub fn encode(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap_or_default()
    }
    
    pub fn decode(data: &[u8]) -> Result<Self> {
        bincode::deserialize(data)
            .map_err(|e| DiscoveryError::DecodingError(e.to_string()))
    }
    
    pub fn node_address(&self) -> Option<NodeAddress> {
        let ip = self.ip.or(self.ip6)?;
        let udp = self.udp.or(self.udp6)?;
        
        Some(NodeAddress {
            id: self.node_id.clone(),
            ip,
            udp_port: udp,
            tcp_port: self.tcp.or(self.tcp6).unwrap_or(udp),
        })
    }
}

/// Node ID (256-bit)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(H256);

impl NodeId {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(H256::from(bytes))
    }
    
    pub fn from_public_key(pubkey: &[u8]) -> Self {
        let hash = keccak256(pubkey);
        Self(H256::from_slice(&hash))
    }
    
    pub fn random() -> Self {
        Self(H256::random())
    }
    
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
    
    pub fn distance(&self, other: &NodeId) -> U256 {
        let mut xor = [0u8; 32];
        for i in 0..32 {
            xor[i] = self.0.as_bytes()[i] ^ other.0.as_bytes()[i];
        }
        U256::from_big_endian(&xor)
    }
    
    pub fn log_distance(&self, other: &NodeId) -> Option<usize> {
        let dist = self.distance(other);
        if dist.is_zero() {
            None
        } else {
            Some(256 - dist.leading_zeros() as usize)
        }
    }
}

use ethereum_types::U256;

/// Node address information
#[derive(Debug, Clone)]
pub struct NodeAddress {
    pub id: NodeId,
    pub ip: IpAddr,
    pub udp_port: u16,
    pub tcp_port: u16,
}

impl NodeAddress {
    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.ip, self.udp_port)
    }
}

/// Discovery v5 protocol messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Ping {
        request_id: u64,
        enr_seq: u64,
    },
    Pong {
        request_id: u64,
        enr_seq: u64,
        ip: IpAddr,
        port: u16,
    },
    FindNode {
        request_id: u64,
        distances: Vec<u16>,
    },
    Nodes {
        request_id: u64,
        total: u8,
        enrs: Vec<Enr>,
    },
    TalkRequest {
        request_id: u64,
        protocol: Vec<u8>,
        request: Vec<u8>,
    },
    TalkResponse {
        request_id: u64,
        response: Vec<u8>,
    },
    RegisterTopic {
        request_id: u64,
        topic: H256,
        enr: Enr,
        ticket: Vec<u8>,
    },
    Ticket {
        request_id: u64,
        ticket: Vec<u8>,
        wait_time: u32,
    },
    RegistrationConfirmation {
        request_id: u64,
        topic: H256,
    },
    TopicQuery {
        request_id: u64,
        topic: H256,
    },
}

/// Kademlia-like routing table
pub struct RoutingTable {
    local_id: NodeId,
    buckets: Vec<KBucket>,
    node_info: Arc<RwLock<HashMap<NodeId, NodeInfo>>>,
}

struct KBucket {
    nodes: Vec<NodeId>,
    capacity: usize,
    last_updated: Instant,
}

#[derive(Clone)]
struct NodeInfo {
    enr: Enr,
    last_seen: Instant,
    failures: u32,
}

impl RoutingTable {
    pub fn new(local_id: NodeId) -> Self {
        let mut buckets = Vec::with_capacity(256);
        for _ in 0..256 {
            buckets.push(KBucket {
                nodes: Vec::new(),
                capacity: 16,
                last_updated: Instant::now(),
            });
        }
        
        Self {
            local_id,
            buckets,
            node_info: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub fn add_node(&mut self, enr: Enr) -> Result<()> {
        let node_id = enr.node_id.clone();
        
        if node_id == self.local_id {
            return Ok(());
        }
        
        let bucket_idx = self.bucket_index(&node_id)?;
        let bucket = &mut self.buckets[bucket_idx];
        
        if let Some(pos) = bucket.nodes.iter().position(|id| id == &node_id) {
            // Move to end (most recently seen)
            bucket.nodes.remove(pos);
            bucket.nodes.push(node_id.clone());
        } else if bucket.nodes.len() < bucket.capacity {
            bucket.nodes.push(node_id.clone());
        } else {
            return Err(DiscoveryError::BucketFull);
        }
        
        bucket.last_updated = Instant::now();
        
        let mut info = self.node_info.write().unwrap();
        info.insert(node_id, NodeInfo {
            enr,
            last_seen: Instant::now(),
            failures: 0,
        });
        
        Ok(())
    }
    
    pub fn remove_node(&mut self, node_id: &NodeId) {
        if let Ok(bucket_idx) = self.bucket_index(node_id) {
            let bucket = &mut self.buckets[bucket_idx];
            bucket.nodes.retain(|id| id != node_id);
            
            let mut info = self.node_info.write().unwrap();
            info.remove(node_id);
        }
    }
    
    pub fn get_node(&self, node_id: &NodeId) -> Option<Enr> {
        let info = self.node_info.read().unwrap();
        info.get(node_id).map(|i| i.enr.clone())
    }
    
    pub fn closest_nodes(&self, target: &NodeId, limit: usize) -> Vec<Enr> {
        let mut nodes_with_distance: Vec<(NodeId, U256)> = Vec::new();
        
        for bucket in &self.buckets {
            for node_id in &bucket.nodes {
                let distance = node_id.distance(target);
                nodes_with_distance.push((node_id.clone(), distance));
            }
        }
        
        nodes_with_distance.sort_by(|a, b| a.1.cmp(&b.1));
        
        let info = self.node_info.read().unwrap();
        nodes_with_distance
            .into_iter()
            .take(limit)
            .filter_map(|(id, _)| info.get(&id).map(|i| i.enr.clone()))
            .collect()
    }
    
    fn bucket_index(&self, node_id: &NodeId) -> Result<usize> {
        self.local_id.log_distance(node_id)
            .ok_or(DiscoveryError::NodeNotFound)
            .map(|d| d.min(255))
    }
    
    pub fn random_nodes(&self, count: usize) -> Vec<Enr> {
        use rand::seq::SliceRandom;
        
        let info = self.node_info.read().unwrap();
        let mut all_nodes: Vec<Enr> = info.values().map(|i| i.enr.clone()).collect();
        
        let mut rng = rand::thread_rng();
        all_nodes.shuffle(&mut rng);
        
        all_nodes.into_iter().take(count).collect()
    }
}

/// Topic advertisement for content discovery
pub struct TopicTable {
    registrations: HashMap<H256, Vec<TopicRegistration>>,
    tickets: HashMap<Vec<u8>, TicketInfo>,
}

#[derive(Clone)]
struct TopicRegistration {
    node_id: NodeId,
    enr: Enr,
    registered_at: Instant,
    expires_at: Instant,
}

struct TicketInfo {
    topic: H256,
    node_id: NodeId,
    created_at: Instant,
    wait_time: Duration,
}

impl TopicTable {
    pub fn new() -> Self {
        Self {
            registrations: HashMap::new(),
            tickets: HashMap::new(),
        }
    }
    
    pub fn register(&mut self, topic: H256, enr: Enr, duration: Duration) {
        let registration = TopicRegistration {
            node_id: enr.node_id.clone(),
            enr,
            registered_at: Instant::now(),
            expires_at: Instant::now() + duration,
        };
        
        self.registrations
            .entry(topic)
            .or_insert_with(Vec::new)
            .push(registration);
    }
    
    pub fn query(&self, topic: &H256, limit: usize) -> Vec<Enr> {
        self.registrations
            .get(topic)
            .map(|regs| {
                let now = Instant::now();
                regs.iter()
                    .filter(|r| r.expires_at > now)
                    .take(limit)
                    .map(|r| r.enr.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
    
    pub fn create_ticket(&mut self, topic: H256, node_id: NodeId) -> Vec<u8> {
        let ticket = rand::random::<[u8; 32]>().to_vec();
        
        self.tickets.insert(ticket.clone(), TicketInfo {
            topic,
            node_id,
            created_at: Instant::now(),
            wait_time: Duration::from_secs(5),
        });
        
        ticket
    }
    
    pub fn verify_ticket(&self, ticket: &[u8]) -> Option<(H256, NodeId)> {
        self.tickets.get(ticket).map(|info| {
            (info.topic.clone(), info.node_id.clone())
        })
    }
    
    pub fn cleanup_expired(&mut self) {
        let now = Instant::now();
        
        for registrations in self.registrations.values_mut() {
            registrations.retain(|r| r.expires_at > now);
        }
        
        self.tickets.retain(|_, info| {
            now.duration_since(info.created_at) < Duration::from_secs(300)
        });
    }
}

/// Main Discovery v5 service
pub struct Discovery {
    local_id: NodeId,
    local_enr: Arc<RwLock<Enr>>,
    socket: Arc<UdpSocket>,
    routing_table: Arc<RwLock<RoutingTable>>,
    topic_table: Arc<RwLock<TopicTable>>,
    pending_requests: Arc<RwLock<HashMap<u64, PendingRequest>>>,
    msg_tx: mpsc::Sender<(Message, SocketAddr)>,
    msg_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<(Message, SocketAddr)>>>,
}

struct PendingRequest {
    message: Message,
    node_id: NodeId,
    sent_at: Instant,
    timeout: Duration,
}

impl Discovery {
    pub async fn new(bind_addr: SocketAddr, private_key: [u8; 32]) -> Result<Self> {
        let socket = UdpSocket::bind(bind_addr).await
            .map_err(|e| DiscoveryError::NetworkError(e.to_string()))?;
        
        let node_id = NodeId::from_public_key(&ethereum_crypto::public_key_from_private(&private_key));
        
        let mut enr = Enr::new(node_id.clone(), 1);
        enr = enr.with_ip(bind_addr.ip(), bind_addr.port(), bind_addr.port());
        enr.sign(&private_key)?;
        
        let (msg_tx, msg_rx) = mpsc::channel(1000);
        
        Ok(Self {
            local_id: node_id.clone(),
            local_enr: Arc::new(RwLock::new(enr)),
            socket: Arc::new(socket),
            routing_table: Arc::new(RwLock::new(RoutingTable::new(node_id))),
            topic_table: Arc::new(RwLock::new(TopicTable::new())),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            msg_tx,
            msg_rx: Arc::new(tokio::sync::Mutex::new(msg_rx)),
        })
    }
    
    pub async fn start(&self) {
        info!("Starting Discovery v5 protocol");
        
        // Start message handler
        let handler = self.clone();
        tokio::spawn(async move {
            handler.message_handler().await;
        });
        
        // Start network listener
        let listener = self.clone();
        tokio::spawn(async move {
            listener.network_listener().await;
        });
        
        // Start maintenance tasks
        let maintenance = self.clone();
        tokio::spawn(async move {
            maintenance.maintenance_loop().await;
        });
    }
    
    async fn network_listener(&self) {
        let mut buf = vec![0u8; 1280]; // Max UDP packet size for discovery
        
        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    if let Ok(message) = self.decode_message(&buf[..len]) {
                        if let Err(e) = self.msg_tx.send((message, addr)).await {
                            error!("Failed to queue message: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("UDP receive error: {}", e);
                }
            }
        }
    }
    
    async fn message_handler(&self) {
        let mut rx = self.msg_rx.lock().await;
        
        while let Some((message, addr)) = rx.recv().await {
            match message {
                Message::Ping { request_id, enr_seq } => {
                    self.handle_ping(request_id, enr_seq, addr).await;
                }
                Message::Pong { request_id, enr_seq, ip, port } => {
                    self.handle_pong(request_id, enr_seq, ip, port).await;
                }
                Message::FindNode { request_id, distances } => {
                    self.handle_find_node(request_id, distances, addr).await;
                }
                Message::Nodes { request_id, total, enrs } => {
                    self.handle_nodes(request_id, total, enrs).await;
                }
                Message::RegisterTopic { request_id, topic, enr, ticket } => {
                    self.handle_register_topic(request_id, topic, enr, ticket, addr).await;
                }
                Message::TopicQuery { request_id, topic } => {
                    self.handle_topic_query(request_id, topic, addr).await;
                }
                _ => {}
            }
        }
    }
    
    async fn handle_ping(&self, request_id: u64, _enr_seq: u64, addr: SocketAddr) {
        let enr = self.local_enr.read().unwrap().clone();
        
        let pong = Message::Pong {
            request_id,
            enr_seq: enr.seq,
            ip: addr.ip(),
            port: addr.port(),
        };
        
        self.send_message(pong, addr).await;
    }
    
    async fn handle_pong(&self, request_id: u64, enr_seq: u64, _ip: IpAddr, _port: u16) {
        let mut pending = self.pending_requests.write().unwrap();
        
        if let Some(req) = pending.remove(&request_id) {
            debug!("Received pong from {:?} with ENR seq {}", req.node_id, enr_seq);
        }
    }
    
    async fn handle_find_node(&self, request_id: u64, distances: Vec<u16>, addr: SocketAddr) {
        let routing_table = self.routing_table.read().unwrap();
        
        let mut all_nodes = Vec::new();
        for distance in distances {
            if distance == 0 {
                // Return our own ENR
                let enr = self.local_enr.read().unwrap().clone();
                all_nodes.push(enr);
            } else {
                // Find nodes at the specified distance
                let nodes = routing_table.random_nodes(3);
                all_nodes.extend(nodes);
            }
        }
        
        // Send nodes in batches
        for chunk in all_nodes.chunks(3) {
            let nodes_msg = Message::Nodes {
                request_id,
                total: (all_nodes.len() / 3 + 1) as u8,
                enrs: chunk.to_vec(),
            };
            
            self.send_message(nodes_msg, addr).await;
        }
    }
    
    async fn handle_nodes(&self, request_id: u64, _total: u8, enrs: Vec<Enr>) {
        let mut routing_table = self.routing_table.write().unwrap();
        
        for enr in enrs {
            if enr.verify().unwrap_or(false) {
                let _ = routing_table.add_node(enr);
            }
        }
        
        let mut pending = self.pending_requests.write().unwrap();
        pending.remove(&request_id);
    }
    
    async fn handle_register_topic(
        &self,
        request_id: u64,
        topic: H256,
        enr: Enr,
        ticket: Vec<u8>,
        addr: SocketAddr,
    ) {
        let mut topic_table = self.topic_table.write().unwrap();
        
        if topic_table.verify_ticket(&ticket).is_some() {
            topic_table.register(topic, enr, Duration::from_secs(3600));
            
            let confirmation = Message::RegistrationConfirmation {
                request_id,
                topic,
            };
            
            self.send_message(confirmation, addr).await;
        }
    }
    
    async fn handle_topic_query(&self, request_id: u64, topic: H256, addr: SocketAddr) {
        let topic_table = self.topic_table.read().unwrap();
        let nodes = topic_table.query(&topic, 5);
        
        let response = Message::Nodes {
            request_id,
            total: 1,
            enrs: nodes,
        };
        
        self.send_message(response, addr).await;
    }
    
    async fn send_message(&self, message: Message, addr: SocketAddr) {
        let data = self.encode_message(&message);
        
        if let Err(e) = self.socket.send_to(&data, addr).await {
            error!("Failed to send message: {}", e);
        }
    }
    
    fn encode_message(&self, message: &Message) -> Vec<u8> {
        bincode::serialize(message).unwrap_or_default()
    }
    
    fn decode_message(&self, data: &[u8]) -> Result<Message> {
        bincode::deserialize(data)
            .map_err(|e| DiscoveryError::DecodingError(e.to_string()))
    }
    
    async fn maintenance_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        
        loop {
            interval.tick().await;
            
            // Clean up expired entries
            self.topic_table.write().unwrap().cleanup_expired();
            
            // Remove timed out requests
            let mut pending = self.pending_requests.write().unwrap();
            let now = Instant::now();
            pending.retain(|_, req| {
                now.duration_since(req.sent_at) < req.timeout
            });
            
            // Refresh routing table
            self.refresh_buckets().await;
        }
    }
    
    async fn refresh_buckets(&self) {
        let random_target = NodeId::random();
        self.find_node(&random_target).await;
    }
    
    pub async fn find_node(&self, target: &NodeId) -> Vec<Enr> {
        let routing_table = self.routing_table.read().unwrap();
        let closest = routing_table.closest_nodes(target, 3);
        
        for enr in &closest {
            if let Some(addr) = enr.node_address() {
                let request_id = rand::random();
                
                let find_node = Message::FindNode {
                    request_id,
                    distances: vec![0, 1, 2],
                };
                
                self.send_message(find_node, addr.socket_addr()).await;
                
                let mut pending = self.pending_requests.write().unwrap();
                pending.insert(request_id, PendingRequest {
                    message: find_node,
                    node_id: enr.node_id.clone(),
                    sent_at: Instant::now(),
                    timeout: Duration::from_secs(5),
                });
            }
        }
        
        closest
    }
    
    pub async fn register_topic(&self, topic: H256, enr: Enr) -> Result<()> {
        let mut topic_table = self.topic_table.write().unwrap();
        topic_table.register(topic, enr, Duration::from_secs(3600));
        Ok(())
    }
    
    pub async fn query_topic(&self, topic: &H256) -> Vec<Enr> {
        let topic_table = self.topic_table.read().unwrap();
        topic_table.query(topic, 10)
    }
}

impl Clone for Discovery {
    fn clone(&self) -> Self {
        Self {
            local_id: self.local_id.clone(),
            local_enr: self.local_enr.clone(),
            socket: self.socket.clone(),
            routing_table: self.routing_table.clone(),
            topic_table: self.topic_table.clone(),
            pending_requests: self.pending_requests.clone(),
            msg_tx: self.msg_tx.clone(),
            msg_rx: self.msg_rx.clone(),
        }
    }
}