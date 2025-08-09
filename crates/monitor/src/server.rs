use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use hyper::{Body, Request, Response, Server, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use prometheus::{TextEncoder, Encoder};
use serde::{Serialize, Deserialize};
use tokio::sync::RwLock;

use crate::{Monitor, Result, MonitorError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsServerConfig {
    pub host: String,
    pub port: u16,
    pub path: String,
    pub enable_health_endpoint: bool,
    pub enable_metrics_endpoint: bool,
}

impl Default for MetricsServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 9090,
            path: "/metrics".to_string(),
            enable_health_endpoint: true,
            enable_metrics_endpoint: true,
        }
    }
}

/// HTTP server for metrics and health endpoints
pub struct MetricsServer {
    config: MetricsServerConfig,
    monitor: Arc<Monitor>,
    server_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl MetricsServer {
    pub fn new(config: MetricsServerConfig, monitor: Arc<Monitor>) -> Self {
        Self {
            config,
            monitor,
            server_handle: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Start the metrics server
    pub async fn start(&self) -> Result<()> {
        let mut handle_guard = self.server_handle.write().await;
        if handle_guard.is_some() {
            return Err(MonitorError::ServerError("Server already running".to_string()));
        }
        
        let addr: SocketAddr = format!("{}:{}", self.config.host, self.config.port)
            .parse()
            .map_err(|e| MonitorError::ServerError(format!("Invalid address: {}", e)))?;
        
        let monitor = self.monitor.clone();
        let config = self.config.clone();
        
        let make_svc = make_service_fn(move |_conn| {
            let monitor = monitor.clone();
            let config = config.clone();
            
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    handle_request(req, monitor.clone(), config.clone())
                }))
            }
        });
        
        let server = Server::bind(&addr).serve(make_svc);
        
        let handle = tokio::spawn(async move {
            tracing::info!("Metrics server listening on http://{}", addr);
            if let Err(e) = server.await {
                tracing::error!("Metrics server error: {}", e);
            }
        });
        
        *handle_guard = Some(handle);
        Ok(())
    }
    
    /// Stop the metrics server
    pub async fn stop(&self) {
        let mut handle_guard = self.server_handle.write().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }
    }
    
    /// Get server address
    pub fn address(&self) -> String {
        format!("http://{}:{}", self.config.host, self.config.port)
    }
}

/// Handle HTTP requests
async fn handle_request(
    req: Request<Body>,
    monitor: Arc<Monitor>,
    config: MetricsServerConfig,
) -> Result<Response<Body>> {
    let path = req.uri().path();
    
    match path {
        "/metrics" if config.enable_metrics_endpoint => {
            handle_metrics(monitor).await
        }
        "/health" if config.enable_health_endpoint => {
            handle_health(monitor).await
        }
        "/health/live" if config.enable_health_endpoint => {
            handle_liveness(monitor).await
        }
        "/health/ready" if config.enable_health_endpoint => {
            handle_readiness(monitor).await
        }
        "/alerts" => {
            handle_alerts(monitor).await
        }
        "/alerts/active" => {
            handle_active_alerts(monitor).await
        }
        "/" => {
            handle_index(config).await
        }
        _ => {
            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Not Found"))
                .unwrap())
        }
    }
}

/// Handle metrics endpoint
async fn handle_metrics(monitor: Arc<Monitor>) -> Result<Response<Body>> {
    match monitor.get_metrics() {
        Ok(metrics) => {
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/plain; version=0.0.4")
                .body(Body::from(metrics))
                .unwrap())
        }
        Err(e) => {
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(format!("Error collecting metrics: {}", e)))
                .unwrap())
        }
    }
}

/// Handle health endpoint
async fn handle_health(monitor: Arc<Monitor>) -> Result<Response<Body>> {
    let health = monitor.get_health().await;
    let status_code = match health.status {
        crate::health::HealthState::Healthy => StatusCode::OK,
        crate::health::HealthState::Degraded => StatusCode::OK,
        crate::health::HealthState::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };
    
    let body = serde_json::to_string_pretty(&health)
        .unwrap_or_else(|_| "Error serializing health status".to_string());
    
    Ok(Response::builder()
        .status(status_code)
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap())
}

/// Handle liveness probe endpoint
async fn handle_liveness(monitor: Arc<Monitor>) -> Result<Response<Body>> {
    let health_check = monitor.health_check();
    let probe = crate::health::LivenessProbe::new(health_check);
    
    if probe.check().await {
        Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from("OK"))
            .unwrap())
    } else {
        Ok(Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(Body::from("Unhealthy"))
            .unwrap())
    }
}

/// Handle readiness probe endpoint
async fn handle_readiness(monitor: Arc<Monitor>) -> Result<Response<Body>> {
    let health_check = monitor.health_check();
    let probe = crate::health::ReadinessProbe::new(health_check);
    
    if probe.check().await {
        Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from("Ready"))
            .unwrap())
    } else {
        Ok(Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(Body::from("Not Ready"))
            .unwrap())
    }
}

/// Handle alerts endpoint
async fn handle_alerts(monitor: Arc<Monitor>) -> Result<Response<Body>> {
    let alert_manager = monitor.alert_manager();
    let history = alert_manager.get_alert_history(100).await;
    
    let body = serde_json::to_string_pretty(&history)
        .unwrap_or_else(|_| "Error serializing alerts".to_string());
    
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap())
}

/// Handle active alerts endpoint
async fn handle_active_alerts(monitor: Arc<Monitor>) -> Result<Response<Body>> {
    let alert_manager = monitor.alert_manager();
    let active = alert_manager.get_active_alerts().await;
    
    let body = serde_json::to_string_pretty(&active)
        .unwrap_or_else(|_| "Error serializing active alerts".to_string());
    
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap())
}

/// Handle index page
async fn handle_index(config: MetricsServerConfig) -> Result<Response<Body>> {
    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Ethereum Rust Monitoring</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
            max-width: 1200px;
            margin: 0 auto;
            padding: 20px;
            background: #f5f5f5;
        }}
        h1 {{
            color: #333;
            border-bottom: 2px solid #627eea;
            padding-bottom: 10px;
        }}
        .endpoints {{
            background: white;
            border-radius: 8px;
            padding: 20px;
            margin-top: 20px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        .endpoint {{
            margin: 15px 0;
            padding: 10px;
            background: #f8f9fa;
            border-radius: 4px;
        }}
        .endpoint a {{
            color: #627eea;
            text-decoration: none;
            font-weight: 500;
        }}
        .endpoint a:hover {{
            text-decoration: underline;
        }}
        .description {{
            color: #666;
            margin-left: 20px;
            font-size: 14px;
        }}
    </style>
</head>
<body>
    <h1>⚡ Ethereum Rust Monitoring</h1>
    
    <div class="endpoints">
        <h2>Available Endpoints</h2>
        
        <div class="endpoint">
            <a href="/metrics">📊 /metrics</a>
            <span class="description">Prometheus metrics endpoint</span>
        </div>
        
        <div class="endpoint">
            <a href="/health">🏥 /health</a>
            <span class="description">Detailed health status</span>
        </div>
        
        <div class="endpoint">
            <a href="/health/live">💚 /health/live</a>
            <span class="description">Kubernetes liveness probe</span>
        </div>
        
        <div class="endpoint">
            <a href="/health/ready">✅ /health/ready</a>
            <span class="description">Kubernetes readiness probe</span>
        </div>
        
        <div class="endpoint">
            <a href="/alerts">🚨 /alerts</a>
            <span class="description">Alert history</span>
        </div>
        
        <div class="endpoint">
            <a href="/alerts/active">⚠️ /alerts/active</a>
            <span class="description">Currently active alerts</span>
        </div>
    </div>
    
    <div class="endpoints">
        <h2>Integration</h2>
        <p>Configure your Prometheus server to scrape metrics from:</p>
        <code>http://{}:{}/metrics</code>
    </div>
</body>
</html>"#,
        config.host, config.port
    );
    
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Body::from(html))
        .unwrap())
}

/// Grafana dashboard configuration
pub fn generate_grafana_dashboard() -> serde_json::Value {
    serde_json::json!({
        "dashboard": {
            "title": "Ethereum Rust Node Monitoring",
            "panels": [
                {
                    "title": "Block Height",
                    "targets": [{
                        "expr": "ethereum_block_height"
                    }]
                },
                {
                    "title": "Connected Peers",
                    "targets": [{
                        "expr": "ethereum_peers_connected"
                    }]
                },
                {
                    "title": "Transaction Pool Size",
                    "targets": [{
                        "expr": "ethereum_txpool_total"
                    }]
                },
                {
                    "title": "Sync Progress",
                    "targets": [{
                        "expr": "ethereum_sync_progress"
                    }]
                },
                {
                    "title": "CPU Usage",
                    "targets": [{
                        "expr": "ethereum_process_cpu_usage_percent"
                    }]
                },
                {
                    "title": "Memory Usage",
                    "targets": [{
                        "expr": "ethereum_process_memory_bytes"
                    }]
                }
            ]
        }
    })
}