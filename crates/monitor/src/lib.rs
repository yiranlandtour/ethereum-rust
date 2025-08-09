pub mod metrics;
pub mod collector;
pub mod server;
pub mod health;
pub mod alerts;

use std::sync::Arc;
use tokio::sync::RwLock;
use prometheus::{Registry, Encoder, TextEncoder};
use thiserror::Error;

pub use metrics::{Metrics, MetricsConfig};
pub use collector::{MetricsCollector, SystemMetrics};
pub use server::{MetricsServer, MetricsServerConfig};
pub use health::{HealthCheck, HealthStatus, ComponentHealth};
pub use alerts::{AlertManager, Alert, AlertLevel};

#[derive(Error, Debug)]
pub enum MonitorError {
    #[error("Metrics error: {0}")]
    MetricsError(String),
    
    #[error("Server error: {0}")]
    ServerError(String),
    
    #[error("Registry error: {0}")]
    RegistryError(#[from] prometheus::Error),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),
}

pub type Result<T> = std::result::Result<T, MonitorError>;

/// Main monitoring system
pub struct Monitor {
    metrics: Arc<Metrics>,
    collector: Arc<RwLock<MetricsCollector>>,
    health_check: Arc<HealthCheck>,
    alert_manager: Arc<AlertManager>,
    registry: Registry,
}

impl Monitor {
    pub fn new(config: MetricsConfig) -> Result<Self> {
        let registry = Registry::new();
        let metrics = Arc::new(Metrics::new(&registry)?);
        let collector = Arc::new(RwLock::new(MetricsCollector::new(metrics.clone())));
        let health_check = Arc::new(HealthCheck::new());
        let alert_manager = Arc::new(AlertManager::new(config.alert_config));
        
        Ok(Self {
            metrics,
            collector,
            health_check,
            alert_manager,
            registry,
        })
    }
    
    /// Start monitoring
    pub async fn start(&self) -> Result<()> {
        // Start metrics collection
        self.collector.write().await.start().await?;
        
        // Start health checks
        self.health_check.start_checks().await;
        
        // Start alert manager
        self.alert_manager.start().await?;
        
        Ok(())
    }
    
    /// Stop monitoring
    pub async fn stop(&self) -> Result<()> {
        self.collector.write().await.stop().await;
        self.health_check.stop_checks().await;
        self.alert_manager.stop().await;
        Ok(())
    }
    
    /// Get metrics snapshot
    pub fn get_metrics(&self) -> Result<String> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        String::from_utf8(buffer).map_err(|e| MonitorError::MetricsError(e.to_string()))
    }
    
    /// Get health status
    pub async fn get_health(&self) -> HealthStatus {
        self.health_check.get_status().await
    }
    
    /// Add custom metric
    pub fn register_custom_metric(&self, name: &str, help: &str) -> Result<()> {
        self.metrics.register_custom(name, help, &self.registry)
    }
    
    /// Record custom metric value
    pub fn record_custom(&self, name: &str, value: f64) {
        self.metrics.record_custom(name, value);
    }
    
    /// Get metrics instance
    pub fn metrics(&self) -> Arc<Metrics> {
        self.metrics.clone()
    }
    
    /// Get collector instance
    pub fn collector(&self) -> Arc<RwLock<MetricsCollector>> {
        self.collector.clone()
    }
    
    /// Get health check instance
    pub fn health_check(&self) -> Arc<HealthCheck> {
        self.health_check.clone()
    }
    
    /// Get alert manager
    pub fn alert_manager(&self) -> Arc<AlertManager> {
        self.alert_manager.clone()
    }
}