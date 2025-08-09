use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time;
use sysinfo::{System, SystemExt, ProcessExt, DiskExt, CpuExt};
use chrono::{DateTime, Utc};

use crate::{Metrics, Result, MonitorError};

/// System metrics snapshot
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    pub timestamp: DateTime<Utc>,
    pub cpu_usage: f64,
    pub memory_used: u64,
    pub memory_total: u64,
    pub disk_usage: Vec<DiskMetrics>,
    pub network_io: NetworkIO,
    pub process_metrics: ProcessMetrics,
}

#[derive(Debug, Clone)]
pub struct DiskMetrics {
    pub mount_point: String,
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub usage_percent: f64,
}

#[derive(Debug, Clone)]
pub struct NetworkIO {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
}

#[derive(Debug, Clone)]
pub struct ProcessMetrics {
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    pub virtual_memory_bytes: u64,
    pub thread_count: usize,
    pub open_files: usize,
}

/// Metrics collector that periodically collects system and application metrics
pub struct MetricsCollector {
    metrics: Arc<Metrics>,
    system: System,
    collection_interval: Duration,
    collector_handle: Option<JoinHandle<()>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl MetricsCollector {
    pub fn new(metrics: Arc<Metrics>) -> Self {
        Self {
            metrics,
            system: System::new_all(),
            collection_interval: Duration::from_secs(10),
            collector_handle: None,
            shutdown_tx: None,
        }
    }
    
    pub fn with_interval(metrics: Arc<Metrics>, interval_secs: u64) -> Self {
        Self {
            metrics,
            system: System::new_all(),
            collection_interval: Duration::from_secs(interval_secs),
            collector_handle: None,
            shutdown_tx: None,
        }
    }
    
    /// Start the metrics collection loop
    pub async fn start(&mut self) -> Result<()> {
        if self.collector_handle.is_some() {
            return Err(MonitorError::MetricsError("Collector already running".to_string()));
        }
        
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);
        
        let metrics = self.metrics.clone();
        let interval = self.collection_interval;
        
        let handle = tokio::spawn(async move {
            let mut interval_timer = time::interval(interval);
            let mut system = System::new_all();
            
            loop {
                tokio::select! {
                    _ = interval_timer.tick() => {
                        Self::collect_metrics(&metrics, &mut system).await;
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }
        });
        
        self.collector_handle = Some(handle);
        Ok(())
    }
    
    /// Stop the metrics collection
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        
        if let Some(handle) = self.collector_handle.take() {
            let _ = handle.await;
        }
    }
    
    /// Collect all metrics once
    async fn collect_metrics(metrics: &Arc<Metrics>, system: &mut System) {
        // Refresh system information
        system.refresh_all();
        
        // Collect system metrics
        Self::collect_system_metrics(metrics, system);
        
        // Collect process metrics
        Self::collect_process_metrics(metrics, system);
    }
    
    /// Collect system-wide metrics
    fn collect_system_metrics(metrics: &Arc<Metrics>, system: &System) {
        // CPU usage
        let cpu_usage = system.global_cpu_info().cpu_usage();
        metrics.system_cpu_usage.set(cpu_usage as f64);
        
        // Memory usage
        let used_memory = system.used_memory();
        metrics.system_memory_usage.set(used_memory as i64);
        
        // Disk usage
        for disk in system.disks() {
            let mount_point = disk.mount_point().to_string_lossy();
            let total_space = disk.total_space();
            let available_space = disk.available_space();
            let used_space = total_space - available_space;
            
            metrics.system_disk_usage
                .with_label_values(&[&mount_point])
                .set(used_space as f64);
        }
    }
    
    /// Collect process-specific metrics
    fn collect_process_metrics(metrics: &Arc<Metrics>, system: &System) {
        let pid = sysinfo::get_current_pid().ok();
        
        if let Some(pid) = pid {
            if let Some(process) = system.process(pid) {
                // CPU usage
                metrics.process_cpu_usage.set(process.cpu_usage() as f64);
                
                // Memory usage
                metrics.process_memory_usage.set(process.memory() as i64 * 1024);
                
                // Thread count
                // Note: sysinfo doesn't directly provide thread count
                // This would need platform-specific implementation
                
                // Open file descriptors (Linux-specific)
                #[cfg(target_os = "linux")]
                {
                    if let Ok(fds) = std::fs::read_dir(format!("/proc/{}/fd", pid)) {
                        let count = fds.count();
                        metrics.process_open_fds.set(count as i64);
                    }
                }
            }
        }
    }
    
    /// Get current system metrics snapshot
    pub fn get_system_metrics(&mut self) -> SystemMetrics {
        self.system.refresh_all();
        
        let cpu_usage = self.system.global_cpu_info().cpu_usage() as f64;
        let memory_used = self.system.used_memory();
        let memory_total = self.system.total_memory();
        
        let disk_usage: Vec<DiskMetrics> = self.system.disks()
            .iter()
            .map(|disk| {
                let total = disk.total_space();
                let available = disk.available_space();
                let used = total - available;
                DiskMetrics {
                    mount_point: disk.mount_point().to_string_lossy().to_string(),
                    used_bytes: used,
                    total_bytes: total,
                    usage_percent: (used as f64 / total as f64) * 100.0,
                }
            })
            .collect();
        
        let process_metrics = self.get_process_metrics();
        
        SystemMetrics {
            timestamp: Utc::now(),
            cpu_usage,
            memory_used,
            memory_total,
            disk_usage,
            network_io: NetworkIO {
                bytes_sent: 0,
                bytes_received: 0,
                packets_sent: 0,
                packets_received: 0,
            },
            process_metrics,
        }
    }
    
    /// Get process-specific metrics
    fn get_process_metrics(&self) -> ProcessMetrics {
        let pid = sysinfo::get_current_pid().ok();
        
        if let Some(pid) = pid {
            if let Some(process) = self.system.process(pid) {
                let open_files = Self::count_open_files(pid);
                
                return ProcessMetrics {
                    cpu_usage: process.cpu_usage(),
                    memory_bytes: process.memory() * 1024,
                    virtual_memory_bytes: process.virtual_memory() * 1024,
                    thread_count: 0, // Would need platform-specific implementation
                    open_files,
                };
            }
        }
        
        ProcessMetrics {
            cpu_usage: 0.0,
            memory_bytes: 0,
            virtual_memory_bytes: 0,
            thread_count: 0,
            open_files: 0,
        }
    }
    
    /// Count open file descriptors (Linux-specific)
    fn count_open_files(pid: sysinfo::Pid) -> usize {
        #[cfg(target_os = "linux")]
        {
            if let Ok(fds) = std::fs::read_dir(format!("/proc/{}/fd", pid)) {
                return fds.count();
            }
        }
        0
    }
    
    /// Update blockchain metrics
    pub fn update_blockchain_metrics(&self, height: u64, gas_used: u64, tx_count: u32, processing_time: f64) {
        self.metrics.block_height.set(height as i64);
        self.metrics.block_gas_used.observe(gas_used as f64);
        self.metrics.block_transactions.observe(tx_count as f64);
        self.metrics.block_processing_time.observe(processing_time);
    }
    
    /// Update network metrics
    pub fn update_network_metrics(&self, peer_count: usize, bytes_sent: u64, bytes_received: u64) {
        self.metrics.peers_connected.set(peer_count as i64);
        self.metrics.bytes_sent.inc_by(bytes_sent as f64);
        self.metrics.bytes_received.inc_by(bytes_received as f64);
    }
    
    /// Update transaction pool metrics
    pub fn update_txpool_metrics(&self, pending: usize, queued: usize) {
        self.metrics.txpool_pending.set(pending as i64);
        self.metrics.txpool_queued.set(queued as i64);
        self.metrics.txpool_total.set((pending + queued) as i64);
    }
    
    /// Update sync metrics
    pub fn update_sync_metrics(&self, current: u64, highest: u64) {
        self.metrics.sync_current_block.set(current as i64);
        self.metrics.sync_highest_block.set(highest as i64);
        
        if highest > 0 {
            let progress = (current as f64 / highest as f64) * 100.0;
            self.metrics.sync_progress.set(progress);
        }
    }
    
    /// Record RPC request
    pub fn record_rpc_request(&self, method: &str, duration: f64, success: bool) {
        self.metrics.rpc_requests.with_label_values(&[method]).inc();
        self.metrics.rpc_request_duration.with_label_values(&[method]).observe(duration);
        
        if !success {
            self.metrics.rpc_errors.with_label_values(&[method, "request_failed"]).inc();
        }
    }
    
    /// Record EVM execution
    pub fn record_evm_execution(&self, gas_used: u64, duration: f64, reverted: bool) {
        self.metrics.evm_executions.inc();
        self.metrics.evm_gas_used.inc_by(gas_used as f64);
        self.metrics.evm_execution_time.observe(duration);
        
        if reverted {
            self.metrics.evm_reverts.inc();
        }
    }
}