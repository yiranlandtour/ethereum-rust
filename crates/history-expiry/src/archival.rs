use ethereum_types::{H256, U256};
use ethereum_core::Block;
use std::sync::{Arc, RwLock};
use std::path::PathBuf;
use tokio::fs;
use async_trait::async_trait;
use tracing::{info, debug, warn};
use parquet::file::writer::{FileWriter, SerializedFileWriter};
use arrow::array::{ArrayRef, BinaryArray, UInt64Array};
use arrow::record_batch::RecordBatch;
use zstd::stream::encode_all;
use ipfs_api::IpfsClient;

use crate::{Result, HistoryExpiryError};

/// Archival backend trait for different storage solutions
#[async_trait]
pub trait ArchivalBackend: Send + Sync {
    /// Archive a batch of blocks
    async fn archive_blocks(&self, blocks: Vec<Block>) -> Result<String>;
    
    /// Retrieve archived blocks
    async fn retrieve_blocks(&self, archive_id: &str, block_range: std::ops::Range<u64>) -> Result<Vec<Block>>;
    
    /// Get archival statistics
    async fn get_stats(&self) -> Result<ArchivalStats>;
    
    /// Verify archive integrity
    async fn verify_archive(&self, archive_id: &str) -> Result<bool>;
}

#[derive(Debug, Clone)]
pub enum ArchivalStrategy {
    /// Local file system archival
    FileSystem { 
        base_path: PathBuf,
        compression: CompressionType,
    },
    /// IPFS distributed storage
    IPFS { 
        gateway: String,
        pinning_service: Option<String>,
    },
    /// Arweave permanent storage
    Arweave { 
        gateway: String,
        wallet_path: PathBuf,
    },
    /// Cloud storage (S3-compatible)
    Cloud { 
        endpoint: String,
        bucket: String,
        region: String,
    },
    /// BitTorrent for P2P sharing
    Torrent {
        tracker_url: String,
        seed_after_upload: bool,
    },
    /// Multiple backends for redundancy
    Multi {
        primary: Box<ArchivalStrategy>,
        secondary: Vec<Box<ArchivalStrategy>>,
    },
}

#[derive(Debug, Clone)]
pub enum CompressionType {
    None,
    Zstd { level: i32 },
    Lz4,
    Snappy,
}

/// File system archival backend
pub struct FileSystemBackend {
    base_path: PathBuf,
    compression: CompressionType,
    metadata: Arc<RwLock<ArchivalMetadata>>,
}

struct ArchivalMetadata {
    archives: Vec<ArchiveEntry>,
    total_size: u64,
    total_blocks: u64,
}

#[derive(Debug, Clone)]
struct ArchiveEntry {
    id: String,
    path: PathBuf,
    block_range: std::ops::Range<u64>,
    size: u64,
    created_at: std::time::SystemTime,
    checksum: H256,
}

impl FileSystemBackend {
    pub fn new(base_path: PathBuf, compression: CompressionType) -> Result<Self> {
        Ok(Self {
            base_path,
            compression,
            metadata: Arc::new(RwLock::new(ArchivalMetadata {
                archives: Vec::new(),
                total_size: 0,
                total_blocks: 0,
            })),
        })
    }

    async fn compress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        match &self.compression {
            CompressionType::None => Ok(data.to_vec()),
            CompressionType::Zstd { level } => {
                encode_all(data, *level)
                    .map_err(|e| HistoryExpiryError::ArchivalError(format!("Compression failed: {}", e)))
            }
            CompressionType::Lz4 => {
                let compressed = lz4::block::compress(data, None, true)
                    .map_err(|e| HistoryExpiryError::ArchivalError(format!("LZ4 compression failed: {}", e)))?;
                Ok(compressed)
            }
            CompressionType::Snappy => {
                let mut encoder = snap::raw::Encoder::new();
                let compressed = encoder.compress_vec(data)
                    .map_err(|e| HistoryExpiryError::ArchivalError(format!("Snappy compression failed: {}", e)))?;
                Ok(compressed)
            }
        }
    }

    async fn decompress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        match &self.compression {
            CompressionType::None => Ok(data.to_vec()),
            CompressionType::Zstd { .. } => {
                zstd::stream::decode_all(data)
                    .map_err(|e| HistoryExpiryError::ArchivalError(format!("Decompression failed: {}", e)))
            }
            CompressionType::Lz4 => {
                lz4::block::decompress(data, None)
                    .map_err(|e| HistoryExpiryError::ArchivalError(format!("LZ4 decompression failed: {}", e)))
            }
            CompressionType::Snappy => {
                let mut decoder = snap::raw::Decoder::new();
                decoder.decompress_vec(data)
                    .map_err(|e| HistoryExpiryError::ArchivalError(format!("Snappy decompression failed: {}", e)))
            }
        }
    }
}

#[async_trait]
impl ArchivalBackend for FileSystemBackend {
    async fn archive_blocks(&self, blocks: Vec<Block>) -> Result<String> {
        if blocks.is_empty() {
            return Err(HistoryExpiryError::ArchivalError("No blocks to archive".into()));
        }

        let first_block = blocks.first().unwrap().header.number;
        let last_block = blocks.last().unwrap().header.number;
        let archive_id = format!("blocks_{}-{}", first_block, last_block);
        
        // Create archive directory if needed
        let archive_dir = self.base_path.join("archives");
        fs::create_dir_all(&archive_dir).await
            .map_err(|e| HistoryExpiryError::ArchivalError(format!("Failed to create directory: {}", e)))?;

        // Serialize blocks
        let serialized = bincode::serialize(&blocks)
            .map_err(|e| HistoryExpiryError::ArchivalError(format!("Serialization failed: {}", e)))?;

        // Compress data
        let compressed = self.compress_data(&serialized).await?;

        // Calculate checksum
        let checksum = H256::from_slice(&ethereum_crypto::keccak256(&compressed));

        // Write to file
        let file_path = archive_dir.join(format!("{}.dat", archive_id));
        fs::write(&file_path, &compressed).await
            .map_err(|e| HistoryExpiryError::ArchivalError(format!("Failed to write archive: {}", e)))?;

        // Update metadata
        {
            let mut metadata = self.metadata.write().unwrap();
            metadata.archives.push(ArchiveEntry {
                id: archive_id.clone(),
                path: file_path,
                block_range: first_block..last_block + 1,
                size: compressed.len() as u64,
                created_at: std::time::SystemTime::now(),
                checksum,
            });
            metadata.total_size += compressed.len() as u64;
            metadata.total_blocks += blocks.len() as u64;
        }

        info!("Archived {} blocks ({} bytes compressed) to {}", 
              blocks.len(), compressed.len(), archive_id);

        Ok(archive_id)
    }

    async fn retrieve_blocks(&self, archive_id: &str, block_range: std::ops::Range<u64>) -> Result<Vec<Block>> {
        let entry = {
            let metadata = self.metadata.read().unwrap();
            metadata.archives.iter()
                .find(|e| e.id == archive_id)
                .cloned()
                .ok_or_else(|| HistoryExpiryError::RetrievalError("Archive not found".into()))?
        };

        // Read compressed data
        let compressed = fs::read(&entry.path).await
            .map_err(|e| HistoryExpiryError::RetrievalError(format!("Failed to read archive: {}", e)))?;

        // Verify checksum
        let checksum = H256::from_slice(&ethereum_crypto::keccak256(&compressed));
        if checksum != entry.checksum {
            return Err(HistoryExpiryError::RetrievalError("Checksum mismatch".into()));
        }

        // Decompress
        let decompressed = self.decompress_data(&compressed).await?;

        // Deserialize
        let blocks: Vec<Block> = bincode::deserialize(&decompressed)
            .map_err(|e| HistoryExpiryError::RetrievalError(format!("Deserialization failed: {}", e)))?;

        // Filter to requested range
        let filtered: Vec<Block> = blocks.into_iter()
            .filter(|b| block_range.contains(&b.header.number))
            .collect();

        Ok(filtered)
    }

    async fn get_stats(&self) -> Result<ArchivalStats> {
        let metadata = self.metadata.read().unwrap();
        
        Ok(ArchivalStats {
            total_archives: metadata.archives.len(),
            total_blocks: metadata.total_blocks,
            total_size: metadata.total_size,
            compression_ratio: if metadata.total_blocks > 0 {
                // Estimate uncompressed size (rough)
                let estimated_uncompressed = metadata.total_blocks * 2_000_000; // ~2MB per block
                metadata.total_size as f64 / estimated_uncompressed as f64
            } else {
                1.0
            },
            oldest_archive: metadata.archives.first().map(|e| e.created_at),
            newest_archive: metadata.archives.last().map(|e| e.created_at),
        })
    }

    async fn verify_archive(&self, archive_id: &str) -> Result<bool> {
        let entry = {
            let metadata = self.metadata.read().unwrap();
            metadata.archives.iter()
                .find(|e| e.id == archive_id)
                .cloned()
                .ok_or_else(|| HistoryExpiryError::ArchivalError("Archive not found".into()))?
        };

        // Read and verify checksum
        let data = fs::read(&entry.path).await
            .map_err(|e| HistoryExpiryError::ArchivalError(format!("Failed to read archive: {}", e)))?;

        let checksum = H256::from_slice(&ethereum_crypto::keccak256(&data));
        Ok(checksum == entry.checksum)
    }
}

/// IPFS archival backend
pub struct IPFSBackend {
    client: IpfsClient,
    pinning_service: Option<String>,
    metadata: Arc<RwLock<HashMap<String, IPFSArchiveEntry>>>,
}

use std::collections::HashMap;

#[derive(Debug, Clone)]
struct IPFSArchiveEntry {
    cid: String,
    block_range: std::ops::Range<u64>,
    size: u64,
    created_at: std::time::SystemTime,
}

impl IPFSBackend {
    pub fn new(gateway: String, pinning_service: Option<String>) -> Result<Self> {
        let client = IpfsClient::from_str(&gateway)
            .map_err(|e| HistoryExpiryError::ArchivalError(format!("Failed to create IPFS client: {}", e)))?;

        Ok(Self {
            client,
            pinning_service,
            metadata: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

#[async_trait]
impl ArchivalBackend for IPFSBackend {
    async fn archive_blocks(&self, blocks: Vec<Block>) -> Result<String> {
        if blocks.is_empty() {
            return Err(HistoryExpiryError::ArchivalError("No blocks to archive".into()));
        }

        let first_block = blocks.first().unwrap().header.number;
        let last_block = blocks.last().unwrap().header.number;

        // Serialize blocks
        let serialized = bincode::serialize(&blocks)
            .map_err(|e| HistoryExpiryError::ArchivalError(format!("Serialization failed: {}", e)))?;

        // Add to IPFS
        let res = self.client.add(std::io::Cursor::new(serialized)).await
            .map_err(|e| HistoryExpiryError::ArchivalError(format!("Failed to add to IPFS: {}", e)))?;

        let cid = res.hash;

        // Pin if configured
        if let Some(ref _service) = self.pinning_service {
            self.client.pin_add(&cid, false).await
                .map_err(|e| HistoryExpiryError::ArchivalError(format!("Failed to pin: {}", e)))?;
        }

        // Store metadata
        {
            let mut metadata = self.metadata.write().unwrap();
            metadata.insert(cid.clone(), IPFSArchiveEntry {
                cid: cid.clone(),
                block_range: first_block..last_block + 1,
                size: res.size.parse().unwrap_or(0),
                created_at: std::time::SystemTime::now(),
            });
        }

        info!("Archived {} blocks to IPFS: {}", blocks.len(), cid);
        Ok(cid)
    }

    async fn retrieve_blocks(&self, archive_id: &str, block_range: std::ops::Range<u64>) -> Result<Vec<Block>> {
        // Get from IPFS
        let data = self.client.cat(archive_id).await
            .map_err(|e| HistoryExpiryError::RetrievalError(format!("Failed to retrieve from IPFS: {}", e)))?;

        // Collect data
        let bytes: Vec<u8> = data
            .map_ok(|chunk| chunk.to_vec())
            .try_concat()
            .await
            .map_err(|e| HistoryExpiryError::RetrievalError(format!("Failed to read IPFS data: {}", e)))?;

        // Deserialize
        let blocks: Vec<Block> = bincode::deserialize(&bytes)
            .map_err(|e| HistoryExpiryError::RetrievalError(format!("Deserialization failed: {}", e)))?;

        // Filter to requested range
        let filtered: Vec<Block> = blocks.into_iter()
            .filter(|b| block_range.contains(&b.header.number))
            .collect();

        Ok(filtered)
    }

    async fn get_stats(&self) -> Result<ArchivalStats> {
        let metadata = self.metadata.read().unwrap();
        
        let total_blocks: u64 = metadata.values()
            .map(|e| e.block_range.end - e.block_range.start)
            .sum();
        
        let total_size: u64 = metadata.values()
            .map(|e| e.size)
            .sum();

        Ok(ArchivalStats {
            total_archives: metadata.len(),
            total_blocks,
            total_size,
            compression_ratio: 1.0, // IPFS handles its own compression
            oldest_archive: metadata.values().map(|e| e.created_at).min(),
            newest_archive: metadata.values().map(|e| e.created_at).max(),
        })
    }

    async fn verify_archive(&self, archive_id: &str) -> Result<bool> {
        // Check if we can retrieve the archive
        match self.client.cat(archive_id).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

use futures::TryStreamExt;

#[derive(Debug, Clone)]
pub struct ArchivalStats {
    pub total_archives: usize,
    pub total_blocks: u64,
    pub total_size: u64,
    pub compression_ratio: f64,
    pub oldest_archive: Option<std::time::SystemTime>,
    pub newest_archive: Option<std::time::SystemTime>,
}