use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    pub enabled: bool,
    pub check_interval_secs: u64,
    pub thresholds: AlertThresholds,
    pub webhooks: Vec<String>,
    pub email_recipients: Vec<String>,
    pub cooldown_minutes: u64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_secs: 60,
            thresholds: AlertThresholds::default(),
            webhooks: Vec::new(),
            email_recipients: Vec::new(),
            cooldown_minutes: 15,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    pub high_cpu_percent: f64,
    pub high_memory_percent: f64,
    pub low_disk_space_gb: u64,
    pub min_peer_count: usize,
    pub max_sync_lag_blocks: u64,
    pub max_txpool_size: usize,
    pub high_error_rate_per_minute: u64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            high_cpu_percent: 90.0,
            high_memory_percent: 85.0,
            low_disk_space_gb: 10,
            min_peer_count: 3,
            max_sync_lag_blocks: 100,
            max_txpool_size: 10000,
            high_error_rate_per_minute: 100,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub level: AlertLevel,
    pub category: String,
    pub message: String,
    pub details: HashMap<String, String>,
    pub timestamp: DateTime<Utc>,
    pub resolved: bool,
    pub resolved_at: Option<DateTime<Utc>>,
}

impl Alert {
    pub fn new(level: AlertLevel, category: String, message: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            level,
            category,
            message,
            details: HashMap::new(),
            timestamp: Utc::now(),
            resolved: false,
            resolved_at: None,
        }
    }
    
    pub fn with_detail(mut self, key: String, value: String) -> Self {
        self.details.insert(key, value);
        self
    }
    
    pub fn resolve(&mut self) {
        self.resolved = true;
        self.resolved_at = Some(Utc::now());
    }
}

/// Alert manager for monitoring and sending alerts
pub struct AlertManager {
    config: AlertConfig,
    active_alerts: Arc<RwLock<HashMap<String, Alert>>>,
    alert_history: Arc<RwLock<Vec<Alert>>>,
    last_alert_times: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    check_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl AlertManager {
    pub fn new(config: AlertConfig) -> Self {
        Self {
            config,
            active_alerts: Arc::new(RwLock::new(HashMap::new())),
            alert_history: Arc::new(RwLock::new(Vec::new())),
            last_alert_times: Arc::new(RwLock::new(HashMap::new())),
            check_handle: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Start the alert manager
    pub async fn start(&self) -> crate::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        
        let mut handle_guard = self.check_handle.write().await;
        if handle_guard.is_some() {
            return Ok(()); // Already running
        }
        
        let active_alerts = self.active_alerts.clone();
        let interval_duration = Duration::from_secs(self.config.check_interval_secs);
        
        let handle = tokio::spawn(async move {
            let mut check_interval = interval(interval_duration);
            
            loop {
                check_interval.tick().await;
                
                // Check for auto-resolved alerts
                let mut alerts = active_alerts.write().await;
                let mut resolved_keys = Vec::new();
                
                for (key, alert) in alerts.iter_mut() {
                    // Auto-resolve alerts older than 1 hour if not updated
                    let age = Utc::now().signed_duration_since(alert.timestamp);
                    if age.num_hours() > 1 && !alert.resolved {
                        alert.resolve();
                        resolved_keys.push(key.clone());
                    }
                }
                
                // Remove resolved alerts
                for key in resolved_keys {
                    alerts.remove(&key);
                }
            }
        });
        
        *handle_guard = Some(handle);
        Ok(())
    }
    
    /// Stop the alert manager
    pub async fn stop(&self) {
        let mut handle_guard = self.check_handle.write().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }
    }
    
    /// Trigger an alert
    pub async fn trigger_alert(&self, alert: Alert) -> crate::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        
        // Check cooldown
        if !self.check_cooldown(&alert.category).await {
            return Ok(()); // Still in cooldown
        }
        
        // Add to active alerts
        let alert_key = format!("{}:{}", alert.category, alert.level as u8);
        self.active_alerts.write().await.insert(alert_key.clone(), alert.clone());
        
        // Add to history
        self.alert_history.write().await.push(alert.clone());
        
        // Update last alert time
        self.last_alert_times.write().await.insert(
            alert.category.clone(),
            Utc::now(),
        );
        
        // Send notifications
        self.send_notifications(&alert).await?;
        
        Ok(())
    }
    
    /// Check if alert is in cooldown
    async fn check_cooldown(&self, category: &str) -> bool {
        let last_times = self.last_alert_times.read().await;
        
        if let Some(last_time) = last_times.get(category) {
            let elapsed = Utc::now().signed_duration_since(*last_time);
            elapsed.num_minutes() >= self.config.cooldown_minutes as i64
        } else {
            true // No previous alert
        }
    }
    
    /// Send alert notifications
    async fn send_notifications(&self, alert: &Alert) -> crate::Result<()> {
        // Send webhook notifications
        for webhook_url in &self.config.webhooks {
            self.send_webhook(webhook_url, alert).await?;
        }
        
        // Send email notifications (would need email service integration)
        if !self.config.email_recipients.is_empty() {
            // Email sending would be implemented here
        }
        
        Ok(())
    }
    
    /// Send webhook notification
    async fn send_webhook(&self, url: &str, alert: &Alert) -> crate::Result<()> {
        // Webhook payload
        let payload = serde_json::json!({
            "id": alert.id,
            "level": alert.level,
            "category": alert.category,
            "message": alert.message,
            "details": alert.details,
            "timestamp": alert.timestamp.to_rfc3339(),
        });
        
        // In production, would use reqwest or similar to send HTTP POST
        tracing::info!("Would send webhook to {}: {}", url, payload);
        
        Ok(())
    }
    
    /// Resolve an alert
    pub async fn resolve_alert(&self, alert_id: &str) -> crate::Result<()> {
        let mut alerts = self.active_alerts.write().await;
        
        for alert in alerts.values_mut() {
            if alert.id == alert_id {
                alert.resolve();
                break;
            }
        }
        
        Ok(())
    }
    
    /// Get active alerts
    pub async fn get_active_alerts(&self) -> Vec<Alert> {
        self.active_alerts.read().await.values().cloned().collect()
    }
    
    /// Get alert history
    pub async fn get_alert_history(&self, limit: usize) -> Vec<Alert> {
        let history = self.alert_history.read().await;
        let start = if history.len() > limit {
            history.len() - limit
        } else {
            0
        };
        history[start..].to_vec()
    }
    
    /// Check system metrics and trigger alerts if needed
    pub async fn check_system_alerts(&self, metrics: &crate::collector::SystemMetrics) {
        // Check CPU usage
        if metrics.cpu_usage > self.config.thresholds.high_cpu_percent {
            let alert = Alert::new(
                AlertLevel::Warning,
                "system.cpu".to_string(),
                format!("High CPU usage: {:.1}%", metrics.cpu_usage),
            ).with_detail("cpu_usage".to_string(), format!("{:.1}", metrics.cpu_usage));
            
            let _ = self.trigger_alert(alert).await;
        }
        
        // Check memory usage
        let memory_percent = (metrics.memory_used as f64 / metrics.memory_total as f64) * 100.0;
        if memory_percent > self.config.thresholds.high_memory_percent {
            let alert = Alert::new(
                AlertLevel::Warning,
                "system.memory".to_string(),
                format!("High memory usage: {:.1}%", memory_percent),
            ).with_detail("memory_percent".to_string(), format!("{:.1}", memory_percent));
            
            let _ = self.trigger_alert(alert).await;
        }
        
        // Check disk space
        for disk in &metrics.disk_usage {
            let free_gb = (disk.total_bytes - disk.used_bytes) / (1024 * 1024 * 1024);
            if free_gb < self.config.thresholds.low_disk_space_gb {
                let alert = Alert::new(
                    AlertLevel::Critical,
                    "system.disk".to_string(),
                    format!("Low disk space on {}: {} GB free", disk.mount_point, free_gb),
                )
                .with_detail("mount_point".to_string(), disk.mount_point.clone())
                .with_detail("free_gb".to_string(), free_gb.to_string());
                
                let _ = self.trigger_alert(alert).await;
            }
        }
    }
    
    /// Check blockchain alerts
    pub async fn check_blockchain_alerts(&self, peer_count: usize, sync_lag: u64, txpool_size: usize) {
        // Check peer count
        if peer_count < self.config.thresholds.min_peer_count {
            let alert = Alert::new(
                AlertLevel::Warning,
                "network.peers".to_string(),
                format!("Low peer count: {}", peer_count),
            ).with_detail("peer_count".to_string(), peer_count.to_string());
            
            let _ = self.trigger_alert(alert).await;
        }
        
        // Check sync lag
        if sync_lag > self.config.thresholds.max_sync_lag_blocks {
            let alert = Alert::new(
                AlertLevel::Warning,
                "sync.lag".to_string(),
                format!("Node is {} blocks behind", sync_lag),
            ).with_detail("lag_blocks".to_string(), sync_lag.to_string());
            
            let _ = self.trigger_alert(alert).await;
        }
        
        // Check transaction pool size
        if txpool_size > self.config.thresholds.max_txpool_size {
            let alert = Alert::new(
                AlertLevel::Warning,
                "txpool.size".to_string(),
                format!("Transaction pool is large: {} transactions", txpool_size),
            ).with_detail("txpool_size".to_string(), txpool_size.to_string());
            
            let _ = self.trigger_alert(alert).await;
        }
    }
}

/// Alert rules for automated alerting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub name: String,
    pub condition: String,
    pub level: AlertLevel,
    pub message_template: String,
    pub enabled: bool,
}

impl AlertRule {
    pub fn evaluate(&self, value: f64, threshold: f64) -> bool {
        match self.condition.as_str() {
            ">" => value > threshold,
            ">=" => value >= threshold,
            "<" => value < threshold,
            "<=" => value <= threshold,
            "==" => (value - threshold).abs() < f64::EPSILON,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_alert_creation() {
        let alert = Alert::new(
            AlertLevel::Warning,
            "test".to_string(),
            "Test alert".to_string(),
        );
        
        assert_eq!(alert.level, AlertLevel::Warning);
        assert_eq!(alert.category, "test");
        assert!(!alert.resolved);
    }
    
    #[tokio::test]
    async fn test_alert_manager() {
        let config = AlertConfig::default();
        let manager = AlertManager::new(config);
        
        let alert = Alert::new(
            AlertLevel::Info,
            "test".to_string(),
            "Test alert".to_string(),
        );
        
        manager.trigger_alert(alert).await.unwrap();
        
        let active = manager.get_active_alerts().await;
        assert_eq!(active.len(), 1);
    }
}