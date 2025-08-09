use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramVec,
    HistogramOpts, Opts, Registry, IntCounter, IntGauge,
};
use std::collections::HashMap;
use std::sync::RwLock;
use serde::{Serialize, Deserialize};

use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enable_process_metrics: bool,
    pub enable_system_metrics: bool,
    pub collection_interval_secs: u64,
    pub retention_hours: u64,
    pub alert_config: crate::alerts::AlertConfig,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enable_process_metrics: true,
            enable_system_metrics: true,
            collection_interval_secs: 10,
            retention_hours: 24,
            alert_config: Default::default(),
        }
    }
}

/// Core metrics for Ethereum node
pub struct Metrics {
    // Network metrics
    pub peers_connected: IntGauge,
    pub peers_discovered: IntCounter,
    pub messages_sent: CounterVec,
    pub messages_received: CounterVec,
    pub bytes_sent: Counter,
    pub bytes_received: Counter,
    pub network_latency: HistogramVec,
    
    // Blockchain metrics
    pub block_height: IntGauge,
    pub block_transactions: Histogram,
    pub block_gas_used: Histogram,
    pub block_processing_time: Histogram,
    pub chain_head_header: IntGauge,
    pub chain_head_block: IntGauge,
    pub chain_head_receipts: IntGauge,
    
    // Transaction pool metrics
    pub txpool_pending: IntGauge,
    pub txpool_queued: IntGauge,
    pub txpool_total: IntGauge,
    pub txpool_evicted: IntCounter,
    pub txpool_added: IntCounter,
    pub txpool_replaced: IntCounter,
    
    // State metrics
    pub state_db_reads: Counter,
    pub state_db_writes: Counter,
    pub state_db_size: IntGauge,
    pub state_cache_hits: Counter,
    pub state_cache_misses: Counter,
    
    // EVM metrics
    pub evm_executions: Counter,
    pub evm_execution_time: Histogram,
    pub evm_gas_used: Counter,
    pub evm_reverts: Counter,
    
    // RPC metrics
    pub rpc_requests: CounterVec,
    pub rpc_request_duration: HistogramVec,
    pub rpc_errors: CounterVec,
    pub rpc_active_connections: IntGauge,
    
    // Sync metrics
    pub sync_progress: Gauge,
    pub sync_highest_block: IntGauge,
    pub sync_starting_block: IntGauge,
    pub sync_current_block: IntGauge,
    pub sync_known_states: IntGauge,
    pub sync_pulled_states: IntGauge,
    
    // System metrics
    pub process_cpu_usage: Gauge,
    pub process_memory_usage: IntGauge,
    pub process_threads: IntGauge,
    pub process_open_fds: IntGauge,
    pub system_cpu_usage: Gauge,
    pub system_memory_usage: IntGauge,
    pub system_disk_usage: GaugeVec,
    
    // Custom metrics
    custom_metrics: RwLock<HashMap<String, Gauge>>,
}

impl Metrics {
    pub fn new(registry: &Registry) -> Result<Self> {
        // Network metrics
        let peers_connected = IntGauge::new("ethereum_peers_connected", "Number of connected peers")?;
        let peers_discovered = IntCounter::new("ethereum_peers_discovered_total", "Total number of discovered peers")?;
        let messages_sent = CounterVec::new(
            Opts::new("ethereum_p2p_messages_sent_total", "Total P2P messages sent"),
            &["protocol", "message_type"]
        )?;
        let messages_received = CounterVec::new(
            Opts::new("ethereum_p2p_messages_received_total", "Total P2P messages received"),
            &["protocol", "message_type"]
        )?;
        let bytes_sent = Counter::new("ethereum_p2p_bytes_sent_total", "Total bytes sent over P2P")?;
        let bytes_received = Counter::new("ethereum_p2p_bytes_received_total", "Total bytes received over P2P")?;
        let network_latency = HistogramVec::new(
            HistogramOpts::new("ethereum_network_latency_seconds", "Network latency to peers"),
            &["peer_id"]
        )?;
        
        // Blockchain metrics
        let block_height = IntGauge::new("ethereum_block_height", "Current blockchain height")?;
        let block_transactions = Histogram::with_opts(
            HistogramOpts::new("ethereum_block_transactions", "Number of transactions per block")
        )?;
        let block_gas_used = Histogram::with_opts(
            HistogramOpts::new("ethereum_block_gas_used", "Gas used per block")
        )?;
        let block_processing_time = Histogram::with_opts(
            HistogramOpts::new("ethereum_block_processing_seconds", "Time to process a block")
        )?;
        let chain_head_header = IntGauge::new("ethereum_chain_head_header", "Current chain head header number")?;
        let chain_head_block = IntGauge::new("ethereum_chain_head_block", "Current chain head block number")?;
        let chain_head_receipts = IntGauge::new("ethereum_chain_head_receipts", "Current chain head receipts number")?;
        
        // Transaction pool metrics
        let txpool_pending = IntGauge::new("ethereum_txpool_pending", "Number of pending transactions")?;
        let txpool_queued = IntGauge::new("ethereum_txpool_queued", "Number of queued transactions")?;
        let txpool_total = IntGauge::new("ethereum_txpool_total", "Total transactions in pool")?;
        let txpool_evicted = IntCounter::new("ethereum_txpool_evicted_total", "Total evicted transactions")?;
        let txpool_added = IntCounter::new("ethereum_txpool_added_total", "Total added transactions")?;
        let txpool_replaced = IntCounter::new("ethereum_txpool_replaced_total", "Total replaced transactions")?;
        
        // State metrics
        let state_db_reads = Counter::new("ethereum_state_db_reads_total", "Total state database reads")?;
        let state_db_writes = Counter::new("ethereum_state_db_writes_total", "Total state database writes")?;
        let state_db_size = IntGauge::new("ethereum_state_db_size_bytes", "State database size in bytes")?;
        let state_cache_hits = Counter::new("ethereum_state_cache_hits_total", "State cache hits")?;
        let state_cache_misses = Counter::new("ethereum_state_cache_misses_total", "State cache misses")?;
        
        // EVM metrics
        let evm_executions = Counter::new("ethereum_evm_executions_total", "Total EVM executions")?;
        let evm_execution_time = Histogram::with_opts(
            HistogramOpts::new("ethereum_evm_execution_seconds", "EVM execution time")
        )?;
        let evm_gas_used = Counter::new("ethereum_evm_gas_used_total", "Total gas used by EVM")?;
        let evm_reverts = Counter::new("ethereum_evm_reverts_total", "Total EVM reverts")?;
        
        // RPC metrics
        let rpc_requests = CounterVec::new(
            Opts::new("ethereum_rpc_requests_total", "Total RPC requests"),
            &["method"]
        )?;
        let rpc_request_duration = HistogramVec::new(
            HistogramOpts::new("ethereum_rpc_request_duration_seconds", "RPC request duration"),
            &["method"]
        )?;
        let rpc_errors = CounterVec::new(
            Opts::new("ethereum_rpc_errors_total", "Total RPC errors"),
            &["method", "error_type"]
        )?;
        let rpc_active_connections = IntGauge::new("ethereum_rpc_active_connections", "Active RPC connections")?;
        
        // Sync metrics
        let sync_progress = Gauge::new("ethereum_sync_progress", "Synchronization progress percentage")?;
        let sync_highest_block = IntGauge::new("ethereum_sync_highest_block", "Highest known block")?;
        let sync_starting_block = IntGauge::new("ethereum_sync_starting_block", "Block sync started from")?;
        let sync_current_block = IntGauge::new("ethereum_sync_current_block", "Current sync block")?;
        let sync_known_states = IntGauge::new("ethereum_sync_known_states", "Known state entries")?;
        let sync_pulled_states = IntGauge::new("ethereum_sync_pulled_states", "Pulled state entries")?;
        
        // System metrics
        let process_cpu_usage = Gauge::new("ethereum_process_cpu_usage_percent", "Process CPU usage")?;
        let process_memory_usage = IntGauge::new("ethereum_process_memory_bytes", "Process memory usage")?;
        let process_threads = IntGauge::new("ethereum_process_threads", "Number of process threads")?;
        let process_open_fds = IntGauge::new("ethereum_process_open_fds", "Number of open file descriptors")?;
        let system_cpu_usage = Gauge::new("ethereum_system_cpu_usage_percent", "System CPU usage")?;
        let system_memory_usage = IntGauge::new("ethereum_system_memory_bytes", "System memory usage")?;
        let system_disk_usage = GaugeVec::new(
            Opts::new("ethereum_system_disk_usage_bytes", "Disk usage by mount point"),
            &["mount_point"]
        )?;
        
        // Register all metrics
        registry.register(Box::new(peers_connected.clone()))?;
        registry.register(Box::new(peers_discovered.clone()))?;
        registry.register(Box::new(messages_sent.clone()))?;
        registry.register(Box::new(messages_received.clone()))?;
        registry.register(Box::new(bytes_sent.clone()))?;
        registry.register(Box::new(bytes_received.clone()))?;
        registry.register(Box::new(network_latency.clone()))?;
        
        registry.register(Box::new(block_height.clone()))?;
        registry.register(Box::new(block_transactions.clone()))?;
        registry.register(Box::new(block_gas_used.clone()))?;
        registry.register(Box::new(block_processing_time.clone()))?;
        registry.register(Box::new(chain_head_header.clone()))?;
        registry.register(Box::new(chain_head_block.clone()))?;
        registry.register(Box::new(chain_head_receipts.clone()))?;
        
        registry.register(Box::new(txpool_pending.clone()))?;
        registry.register(Box::new(txpool_queued.clone()))?;
        registry.register(Box::new(txpool_total.clone()))?;
        registry.register(Box::new(txpool_evicted.clone()))?;
        registry.register(Box::new(txpool_added.clone()))?;
        registry.register(Box::new(txpool_replaced.clone()))?;
        
        registry.register(Box::new(state_db_reads.clone()))?;
        registry.register(Box::new(state_db_writes.clone()))?;
        registry.register(Box::new(state_db_size.clone()))?;
        registry.register(Box::new(state_cache_hits.clone()))?;
        registry.register(Box::new(state_cache_misses.clone()))?;
        
        registry.register(Box::new(evm_executions.clone()))?;
        registry.register(Box::new(evm_execution_time.clone()))?;
        registry.register(Box::new(evm_gas_used.clone()))?;
        registry.register(Box::new(evm_reverts.clone()))?;
        
        registry.register(Box::new(rpc_requests.clone()))?;
        registry.register(Box::new(rpc_request_duration.clone()))?;
        registry.register(Box::new(rpc_errors.clone()))?;
        registry.register(Box::new(rpc_active_connections.clone()))?;
        
        registry.register(Box::new(sync_progress.clone()))?;
        registry.register(Box::new(sync_highest_block.clone()))?;
        registry.register(Box::new(sync_starting_block.clone()))?;
        registry.register(Box::new(sync_current_block.clone()))?;
        registry.register(Box::new(sync_known_states.clone()))?;
        registry.register(Box::new(sync_pulled_states.clone()))?;
        
        registry.register(Box::new(process_cpu_usage.clone()))?;
        registry.register(Box::new(process_memory_usage.clone()))?;
        registry.register(Box::new(process_threads.clone()))?;
        registry.register(Box::new(process_open_fds.clone()))?;
        registry.register(Box::new(system_cpu_usage.clone()))?;
        registry.register(Box::new(system_memory_usage.clone()))?;
        registry.register(Box::new(system_disk_usage.clone()))?;
        
        Ok(Self {
            peers_connected,
            peers_discovered,
            messages_sent,
            messages_received,
            bytes_sent,
            bytes_received,
            network_latency,
            block_height,
            block_transactions,
            block_gas_used,
            block_processing_time,
            chain_head_header,
            chain_head_block,
            chain_head_receipts,
            txpool_pending,
            txpool_queued,
            txpool_total,
            txpool_evicted,
            txpool_added,
            txpool_replaced,
            state_db_reads,
            state_db_writes,
            state_db_size,
            state_cache_hits,
            state_cache_misses,
            evm_executions,
            evm_execution_time,
            evm_gas_used,
            evm_reverts,
            rpc_requests,
            rpc_request_duration,
            rpc_errors,
            rpc_active_connections,
            sync_progress,
            sync_highest_block,
            sync_starting_block,
            sync_current_block,
            sync_known_states,
            sync_pulled_states,
            process_cpu_usage,
            process_memory_usage,
            process_threads,
            process_open_fds,
            system_cpu_usage,
            system_memory_usage,
            system_disk_usage,
            custom_metrics: RwLock::new(HashMap::new()),
        })
    }
    
    /// Register a custom metric
    pub fn register_custom(&self, name: &str, help: &str, registry: &Registry) -> Result<()> {
        let gauge = Gauge::new(name, help)?;
        registry.register(Box::new(gauge.clone()))?;
        self.custom_metrics.write().unwrap().insert(name.to_string(), gauge);
        Ok(())
    }
    
    /// Record a custom metric value
    pub fn record_custom(&self, name: &str, value: f64) {
        if let Some(gauge) = self.custom_metrics.read().unwrap().get(name) {
            gauge.set(value);
        }
    }
}