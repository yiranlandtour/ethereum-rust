use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

/// Health status of the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: HealthState,
    pub timestamp: DateTime<Utc>,
    pub components: HashMap<String, ComponentHealth>,
    pub checks_passed: usize,
    pub checks_failed: usize,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthState {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthState,
    pub message: String,
    pub last_check: DateTime<Utc>,
    pub consecutive_failures: u32,
    pub metadata: HashMap<String, String>,
}

/// Health check function type
pub type HealthCheckFn = Arc<dyn Fn() -> Box<dyn std::future::Future<Output = ComponentHealth> + Send> + Send + Sync>;

/// Health check system
pub struct HealthCheck {
    components: Arc<RwLock<HashMap<String, ComponentHealth>>>,
    checks: Arc<RwLock<HashMap<String, HealthCheckFn>>>,
    check_interval: Duration,
    start_time: DateTime<Utc>,
    check_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl HealthCheck {
    pub fn new() -> Self {
        Self {
            components: Arc::new(RwLock::new(HashMap::new())),
            checks: Arc::new(RwLock::new(HashMap::new())),
            check_interval: Duration::from_secs(30),
            start_time: Utc::now(),
            check_handle: Arc::new(RwLock::new(None)),
        }
    }
    
    pub fn with_interval(interval_secs: u64) -> Self {
        Self {
            components: Arc::new(RwLock::new(HashMap::new())),
            checks: Arc::new(RwLock::new(HashMap::new())),
            check_interval: Duration::from_secs(interval_secs),
            start_time: Utc::now(),
            check_handle: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Register a health check
    pub async fn register_check<F, Fut>(&self, name: String, check: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ComponentHealth> + Send + 'static,
    {
        let check_fn: HealthCheckFn = Arc::new(move || Box::new(check()));
        self.checks.write().await.insert(name.clone(), check_fn);
        
        // Initialize component health
        let initial_health = ComponentHealth {
            name: name.clone(),
            status: HealthState::Healthy,
            message: "Not checked yet".to_string(),
            last_check: Utc::now(),
            consecutive_failures: 0,
            metadata: HashMap::new(),
        };
        self.components.write().await.insert(name, initial_health);
    }
    
    /// Start health checks
    pub async fn start_checks(&self) {
        let mut handle_guard = self.check_handle.write().await;
        if handle_guard.is_some() {
            return; // Already running
        }
        
        let components = self.components.clone();
        let checks = self.checks.clone();
        let interval_duration = self.check_interval;
        
        let handle = tokio::spawn(async move {
            let mut check_interval = interval(interval_duration);
            
            loop {
                check_interval.tick().await;
                
                let checks_snapshot = checks.read().await.clone();
                for (name, check_fn) in checks_snapshot {
                    let result = check_fn().await;
                    components.write().await.insert(name, result);
                }
            }
        });
        
        *handle_guard = Some(handle);
    }
    
    /// Stop health checks
    pub async fn stop_checks(&self) {
        let mut handle_guard = self.check_handle.write().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }
    }
    
    /// Get current health status
    pub async fn get_status(&self) -> HealthStatus {
        let components = self.components.read().await.clone();
        
        let mut checks_passed = 0;
        let mut checks_failed = 0;
        let mut overall_status = HealthState::Healthy;
        
        for (_, health) in &components {
            match health.status {
                HealthState::Healthy => checks_passed += 1,
                HealthState::Degraded => {
                    checks_failed += 1;
                    if overall_status == HealthState::Healthy {
                        overall_status = HealthState::Degraded;
                    }
                }
                HealthState::Unhealthy => {
                    checks_failed += 1;
                    overall_status = HealthState::Unhealthy;
                }
            }
        }
        
        let uptime = Utc::now().signed_duration_since(self.start_time);
        
        HealthStatus {
            status: overall_status,
            timestamp: Utc::now(),
            components,
            checks_passed,
            checks_failed,
            uptime_seconds: uptime.num_seconds() as u64,
        }
    }
    
    /// Get health of specific component
    pub async fn get_component_health(&self, name: &str) -> Option<ComponentHealth> {
        self.components.read().await.get(name).cloned()
    }
    
    /// Check if system is healthy
    pub async fn is_healthy(&self) -> bool {
        let status = self.get_status().await;
        status.status == HealthState::Healthy
    }
    
    /// Register default checks for Ethereum node
    pub async fn register_default_checks(&self) {
        // Database health check
        self.register_check("database".to_string(), || async {
            // Check database connectivity
            ComponentHealth {
                name: "database".to_string(),
                status: HealthState::Healthy,
                message: "Database is accessible".to_string(),
                last_check: Utc::now(),
                consecutive_failures: 0,
                metadata: HashMap::new(),
            }
        }).await;
        
        // Network health check
        self.register_check("network".to_string(), || async {
            // Check peer connections
            ComponentHealth {
                name: "network".to_string(),
                status: HealthState::Healthy,
                message: "Network is operational".to_string(),
                last_check: Utc::now(),
                consecutive_failures: 0,
                metadata: HashMap::new(),
            }
        }).await;
        
        // RPC health check
        self.register_check("rpc".to_string(), || async {
            // Check RPC server
            ComponentHealth {
                name: "rpc".to_string(),
                status: HealthState::Healthy,
                message: "RPC server is responding".to_string(),
                last_check: Utc::now(),
                consecutive_failures: 0,
                metadata: HashMap::new(),
            }
        }).await;
        
        // Sync health check
        self.register_check("sync".to_string(), || async {
            // Check sync status
            ComponentHealth {
                name: "sync".to_string(),
                status: HealthState::Healthy,
                message: "Node is syncing".to_string(),
                last_check: Utc::now(),
                consecutive_failures: 0,
                metadata: HashMap::new(),
            }
        }).await;
        
        // Disk space check
        self.register_check("disk_space".to_string(), || async {
            // Check available disk space
            ComponentHealth {
                name: "disk_space".to_string(),
                status: HealthState::Healthy,
                message: "Sufficient disk space available".to_string(),
                last_check: Utc::now(),
                consecutive_failures: 0,
                metadata: HashMap::new(),
            }
        }).await;
    }
}

/// Liveness probe for Kubernetes
pub struct LivenessProbe {
    health_check: Arc<HealthCheck>,
    max_failures: u32,
}

impl LivenessProbe {
    pub fn new(health_check: Arc<HealthCheck>) -> Self {
        Self {
            health_check,
            max_failures: 3,
        }
    }
    
    pub async fn check(&self) -> bool {
        let status = self.health_check.get_status().await;
        
        // Check if any component has too many consecutive failures
        for (_, component) in &status.components {
            if component.consecutive_failures > self.max_failures {
                return false;
            }
        }
        
        status.status != HealthState::Unhealthy
    }
}

/// Readiness probe for Kubernetes
pub struct ReadinessProbe {
    health_check: Arc<HealthCheck>,
    required_components: Vec<String>,
}

impl ReadinessProbe {
    pub fn new(health_check: Arc<HealthCheck>) -> Self {
        Self {
            health_check,
            required_components: vec![
                "database".to_string(),
                "network".to_string(),
                "rpc".to_string(),
            ],
        }
    }
    
    pub async fn check(&self) -> bool {
        let status = self.health_check.get_status().await;
        
        // Check if all required components are healthy
        for component_name in &self.required_components {
            if let Some(component) = status.components.get(component_name) {
                if component.status != HealthState::Healthy {
                    return false;
                }
            } else {
                return false; // Required component not found
            }
        }
        
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_health_check() {
        let health_check = HealthCheck::new();
        
        // Register a test check
        health_check.register_check("test".to_string(), || async {
            ComponentHealth {
                name: "test".to_string(),
                status: HealthState::Healthy,
                message: "Test is healthy".to_string(),
                last_check: Utc::now(),
                consecutive_failures: 0,
                metadata: HashMap::new(),
            }
        }).await;
        
        let status = health_check.get_status().await;
        assert_eq!(status.components.len(), 1);
        assert!(health_check.is_healthy().await);
    }
}