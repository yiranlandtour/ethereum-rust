use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{RwLock, mpsc};
use ethereum_types::H512;
use secp256k1::PublicKey;

use crate::{Result, NetworkError};
use crate::rlpx::{RLPxHandshake, RLPxSession};
use crate::protocol::Protocol;

#[derive(Debug, Clone)]
pub struct PeerId {
    pub node_id: H512,
    pub address: SocketAddr,
    pub client_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerState {
    Connecting,
    Handshaking,
    Connected,
    Disconnecting,
    Disconnected,
}

pub struct Peer {
    pub id: PeerId,
    pub state: RwLock<PeerState>,
    pub session: Option<Arc<RwLock<RLPxSession>>>,
    pub protocols: Vec<Protocol>,
    pub inbound: bool,
    tx: mpsc::Sender<PeerMessage>,
    rx: Arc<RwLock<mpsc::Receiver<PeerMessage>>>,
}

#[derive(Debug)]
pub enum PeerMessage {
    Data(Vec<u8>),
    Disconnect(DisconnectReason),
}

#[derive(Debug, Clone, Copy)]
pub enum DisconnectReason {
    DisconnectRequested,
    TcpError,
    ProtocolError,
    UselessPeer,
    TooManyPeers,
    AlreadyConnected,
    IncompatibleVersion,
    NullNodeId,
    ClientQuit,
    UnexpectedIdentity,
    LocalIdentity,
    PingTimeout,
    Other(u8),
}

impl Peer {
    pub fn new(
        id: PeerId,
        inbound: bool,
    ) -> Self {
        let (tx, rx) = mpsc::channel(100);
        
        Self {
            id,
            state: RwLock::new(PeerState::Connecting),
            session: None,
            protocols: Vec::new(),
            inbound,
            tx,
            rx: Arc::new(RwLock::new(rx)),
        }
    }
    
    pub async fn connect(&mut self, stream: TcpStream, secret_key: secp256k1::SecretKey) -> Result<()> {
        *self.state.write().await = PeerState::Handshaking;
        
        // Perform RLPx handshake
        // Note: This is a simplified conversion - in practice, you'd need proper node ID to public key mapping
        let remote_id = None; // Simplified for compilation
        
        let mut handshake = RLPxHandshake::new(secret_key, remote_id);
        
        if !self.inbound {
            // Initiate handshake
            let auth = handshake.create_auth_message()?;
            // Send auth over stream
            // Receive ack
            // handshake.handle_ack_message(&ack)?;
        } else {
            // Respond to handshake
            // Receive auth
            // handshake.handle_auth_message(&auth)?;
            let ack = handshake.create_ack_message()?;
            // Send ack over stream
        }
        
        // Derive session secrets
        let secrets = handshake.derive_secrets()?;
        let session = Arc::new(RwLock::new(RLPxSession::new(secrets)));
        self.session = Some(session);
        
        *self.state.write().await = PeerState::Connected;
        
        Ok(())
    }
    
    pub async fn send(&self, data: Vec<u8>) -> Result<()> {
        self.tx.send(PeerMessage::Data(data)).await
            .map_err(|_| NetworkError::PeerDisconnected("Channel closed".to_string()))?;
        Ok(())
    }
    
    pub async fn recv(&self) -> Option<PeerMessage> {
        let mut rx = self.rx.write().await;
        rx.recv().await
    }
    
    pub async fn disconnect(&self, reason: DisconnectReason) -> Result<()> {
        *self.state.write().await = PeerState::Disconnecting;
        
        self.tx.send(PeerMessage::Disconnect(reason)).await
            .map_err(|_| NetworkError::PeerDisconnected("Channel closed".to_string()))?;
        
        *self.state.write().await = PeerState::Disconnected;
        
        Ok(())
    }
    
    pub async fn is_connected(&self) -> bool {
        *self.state.read().await == PeerState::Connected
    }
}

pub struct PeerManager {
    peers: Arc<RwLock<Vec<Arc<Peer>>>>,
    max_peers: usize,
}

impl PeerManager {
    pub fn new(max_peers: usize) -> Self {
        Self {
            peers: Arc::new(RwLock::new(Vec::new())),
            max_peers,
        }
    }
    
    pub async fn add_peer(&self, peer: Arc<Peer>) -> Result<()> {
        let mut peers = self.peers.write().await;
        
        if peers.len() >= self.max_peers {
            return Err(NetworkError::PeerDisconnected("Too many peers".to_string()));
        }
        
        // Check if already connected
        for existing in peers.iter() {
            if existing.id.node_id == peer.id.node_id {
                return Err(NetworkError::PeerDisconnected("Already connected".to_string()));
            }
        }
        
        peers.push(peer);
        Ok(())
    }
    
    pub async fn remove_peer(&self, node_id: &H512) {
        let mut peers = self.peers.write().await;
        peers.retain(|p| p.id.node_id != *node_id);
    }
    
    pub async fn get_peer(&self, node_id: &H512) -> Option<Arc<Peer>> {
        let peers = self.peers.read().await;
        peers.iter()
            .find(|p| p.id.node_id == *node_id)
            .cloned()
    }
    
    pub async fn get_all_peers(&self) -> Vec<Arc<Peer>> {
        self.peers.read().await.clone()
    }
    
    pub async fn connected_count(&self) -> usize {
        let peers = self.peers.read().await;
        let mut count = 0;
        for peer in peers.iter() {
            if peer.is_connected().await {
                count += 1;
            }
        }
        count
    }
}