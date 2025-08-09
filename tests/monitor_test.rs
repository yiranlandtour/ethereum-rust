use ethereum_monitor::{
    Monitor, MetricsConfig, MetricsServer, MetricsServerConfig,
    Alert, AlertLevel, HealthState,
};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_monitoring_system_startup() {
    // Create monitoring configuration
    let config = MetricsConfig::default();
    
    // Create monitor
    let monitor = Monitor::new(config).unwrap();
    
    // Start monitoring
    monitor.start().await.unwrap();
    
    // Let it run for a bit
    sleep(Duration::from_millis(100)).await;
    
    // Get metrics
    let metrics = monitor.get_metrics().unwrap();
    assert!(!metrics.is_empty());
    assert!(metrics.contains("ethereum_block_height"));
    
    // Stop monitoring
    monitor.stop().await.unwrap();
}

#[tokio::test]
async fn test_health_checks() {
    let config = MetricsConfig::default();
    let monitor = Monitor::new(config).unwrap();
    
    // Register default health checks
    monitor.health_check().register_default_checks().await;
    
    // Get health status
    let health = monitor.get_health().await;
    assert_eq!(health.status, HealthState::Healthy);
    assert!(health.components.contains_key("database"));
    assert!(health.components.contains_key("network"));
    assert!(health.components.contains_key("rpc"));
}

#[tokio::test]
async fn test_alert_system() {
    let config = MetricsConfig::default();
    let monitor = Monitor::new(config).unwrap();
    
    // Create test alert
    let alert = Alert::new(
        AlertLevel::Warning,
        "test.category".to_string(),
        "Test warning message".to_string(),
    );
    
    // Trigger alert
    monitor.alert_manager().trigger_alert(alert).await.unwrap();
    
    // Check active alerts
    let active_alerts = monitor.alert_manager().get_active_alerts().await;
    assert_eq!(active_alerts.len(), 1);
    assert_eq!(active_alerts[0].category, "test.category");
    
    // Resolve alert
    let alert_id = active_alerts[0].id.clone();
    monitor.alert_manager().resolve_alert(&alert_id).await.unwrap();
}

#[tokio::test]
async fn test_metrics_server() {
    let config = MetricsConfig::default();
    let monitor = Arc::new(Monitor::new(config).unwrap());
    
    // Create server configuration
    let server_config = MetricsServerConfig {
        host: "127.0.0.1".to_string(),
        port: 19090, // Use different port to avoid conflicts
        ..Default::default()
    };
    
    // Create and start server
    let server = MetricsServer::new(server_config, monitor.clone());
    server.start().await.unwrap();
    
    // Give server time to start
    sleep(Duration::from_millis(500)).await;
    
    // Server should be running
    let address = server.address();
    assert!(address.contains("127.0.0.1:19090"));
    
    // Stop server
    server.stop().await;
}

#[tokio::test]
async fn test_custom_metrics() {
    let config = MetricsConfig::default();
    let monitor = Monitor::new(config).unwrap();
    
    // Register custom metric
    monitor.register_custom_metric(
        "custom_test_metric",
        "A custom test metric"
    ).unwrap();
    
    // Record values
    monitor.record_custom("custom_test_metric", 42.0);
    monitor.record_custom("custom_test_metric", 100.0);
    
    // Metrics should include our custom metric
    let metrics = monitor.get_metrics().unwrap();
    assert!(metrics.contains("custom_test_metric"));
}

#[tokio::test]
async fn test_collector_metrics_update() {
    use ethereum_monitor::MetricsCollector;
    
    let config = MetricsConfig::default();
    let monitor = Monitor::new(config).unwrap();
    
    let collector = monitor.collector();
    let mut collector_guard = collector.write().await;
    
    // Update blockchain metrics
    collector_guard.update_blockchain_metrics(1000, 8_000_000, 150, 2.5);
    
    // Update network metrics
    collector_guard.update_network_metrics(25, 1_000_000, 2_000_000);
    
    // Update transaction pool metrics
    collector_guard.update_txpool_metrics(100, 50);
    
    // Update sync metrics
    collector_guard.update_sync_metrics(900, 1000);
    
    // Get system metrics snapshot
    let system_metrics = collector_guard.get_system_metrics();
    assert!(system_metrics.memory_total > 0);
    assert!(system_metrics.disk_usage.len() > 0);
}

#[tokio::test]
async fn test_alert_thresholds() {
    use ethereum_monitor::alerts::AlertConfig;
    use ethereum_monitor::collector::SystemMetrics;
    
    let mut alert_config = AlertConfig::default();
    alert_config.thresholds.high_cpu_percent = 50.0; // Lower threshold for testing
    
    let mut config = MetricsConfig::default();
    config.alert_config = alert_config;
    
    let monitor = Monitor::new(config).unwrap();
    
    // Create mock system metrics with high CPU
    let high_cpu_metrics = SystemMetrics {
        timestamp: chrono::Utc::now(),
        cpu_usage: 75.0, // Above threshold
        memory_used: 4_000_000_000,
        memory_total: 8_000_000_000,
        disk_usage: vec![],
        network_io: ethereum_monitor::collector::NetworkIO {
            bytes_sent: 0,
            bytes_received: 0,
            packets_sent: 0,
            packets_received: 0,
        },
        process_metrics: ethereum_monitor::collector::ProcessMetrics {
            cpu_usage: 25.0,
            memory_bytes: 1_000_000_000,
            virtual_memory_bytes: 2_000_000_000,
            thread_count: 10,
            open_files: 100,
        },
    };
    
    // Check system alerts
    monitor.alert_manager().check_system_alerts(&high_cpu_metrics).await;
    
    // Should have triggered CPU alert
    let active = monitor.alert_manager().get_active_alerts().await;
    assert!(active.iter().any(|a| a.category == "system.cpu"));
}

#[tokio::test]
async fn test_health_probes() {
    use ethereum_monitor::health::{LivenessProbe, ReadinessProbe};
    
    let config = MetricsConfig::default();
    let monitor = Monitor::new(config).unwrap();
    
    // Register health checks
    monitor.health_check().register_default_checks().await;
    
    // Create probes
    let liveness = LivenessProbe::new(monitor.health_check());
    let readiness = ReadinessProbe::new(monitor.health_check());
    
    // Both should pass initially
    assert!(liveness.check().await);
    assert!(readiness.check().await);
}

#[tokio::test]
async fn test_metrics_persistence() {
    let config = MetricsConfig {
        retention_hours: 1,
        ..Default::default()
    };
    
    let monitor = Monitor::new(config).unwrap();
    let metrics_arc = monitor.metrics();
    
    // Record some network activity
    metrics_arc.peers_connected.set(10);
    metrics_arc.peers_discovered.inc();
    metrics_arc.bytes_sent.inc_by(1000.0);
    metrics_arc.bytes_received.inc_by(2000.0);
    
    // Record blockchain activity
    metrics_arc.block_height.set(12345);
    metrics_arc.block_transactions.observe(150.0);
    metrics_arc.block_gas_used.observe(8_000_000.0);
    metrics_arc.block_processing_time.observe(2.5);
    
    // Get metrics string
    let metrics_output = monitor.get_metrics().unwrap();
    
    // Verify metrics are present
    assert!(metrics_output.contains("ethereum_peers_connected 10"));
    assert!(metrics_output.contains("ethereum_peers_discovered_total 1"));
    assert!(metrics_output.contains("ethereum_block_height 12345"));
}