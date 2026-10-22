//! Sentinel OS Health Check Service
//!
//! Comprehensive system health monitoring with hardware, software, and security checks.

mod checks;
mod collector;
mod api;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{info, error};

use checks::HealthChecker;
use collector::MetricsCollector;
use api::{create_router, AppState};

#[derive(Parser, Debug)]
#[command(name = "sentinel-health-check", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/etc/sentinel/health-check.toml")]
    config: PathBuf,

    /// HTTP API address
    #[arg(long, default_value = "127.0.0.1:8087")]
    http_addr: String,

    /// Check interval seconds
    #[arg(long, default_value = "30")]
    check_interval: u64,

    /// Metrics retention hours
    #[arg(long, default_value = "24")]
    retention_hours: u64,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Config {
    checks: ChecksConfig,
    collector: CollectorConfig,
    alerts: AlertsConfig,
    server: ServerConfig,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ChecksConfig {
    enabled: Vec<String>,
    disabled: Vec<String>,
    thresholds: ThresholdsConfig,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ThresholdsConfig {
    cpu_warning: f32,
    cpu_critical: f32,
    memory_warning: f32,
    memory_critical: f32,
    disk_warning: f32,
    disk_critical: f32,
    temperature_warning: f32,
    temperature_critical: f32,
    gpu_memory_warning: f32,
    gpu_memory_critical: f32,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct CollectorConfig {
    interval_secs: u64,
    retention_hours: u64,
    export_prometheus: bool,
    prometheus_port: u16,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct AlertsConfig {
    enabled: bool,
    webhook_url: Option<String>,
    email_recipients: Vec<String>,
    slack_webhook: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ServerConfig {
    addr: String,
    tls_cert: Option<PathBuf>,
    tls_key: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| args.log_level.into());

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(true)
        .init();

    info!("Starting Sentinel Health Check Service v{}", env!("CARGO_PKG_VERSION"));

    // Load config
    let config = load_config(&args).await?;

    // Initialize health checker
    let checker = Arc::new(RwLock::new(HealthChecker::new(&config.checks).await?));

    // Initialize metrics collector
    let collector = Arc::new(MetricsCollector::new(&config.collector).await?);

    // Start periodic health checks
    let checker_clone = checker.clone();
    let collector_clone = collector.clone();
    let check_interval = Duration::from_secs(args.check_interval);

    tokio::spawn(async move {
        let mut interval = interval(check_interval);
        loop {
            interval.tick().await;
            if let Err(e) = run_checks(&checker_clone, &collector_clone).await {
                error!("Health check failed: {}", e);
            }
        }
    });

    // Start metrics cleanup
    let collector_clone = collector.clone();
    let retention = Duration::from_secs(config.collector.retention_hours * 3600);
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(3600)); // Every hour
        loop {
            interval.tick().await;
            collector_clone.cleanup_old_metrics(retention).await;
        }
    });

    // Create app state
    let state = AppState {
        checker,
        collector,
        config: config.clone(),
        start_time: SystemTime::now(),
    };

    // Build router
    let app = create_router(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.server.addr).await?;
    info!("Health Check Service API listening on {}", config.server.addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn run_checks(
    checker: &Arc<RwLock<HealthChecker>>,
    collector: &Arc<MetricsCollector>,
) -> Result<()> {
    let results = checker.write().await.run_all_checks().await?;
    collector.record_results(&results).await?;
    Ok(())
}

async fn load_config(args: &Args) -> Result<Config> {
    let config_content = tokio::fs::read_to_string(&args.config).await
        .unwrap_or_else(|_| String::new());

    if config_content.is_empty() {
        Ok(Config {
            checks: ChecksConfig {
                enabled: vec![
                    "cpu".to_string(),
                    "memory".to_string(),
                    "disk".to_string(),
                    "network".to_string(),
                    "gpu".to_string(),
                    "temperature".to_string(),
                    "smart".to_string(),
                    "services".to_string(),
                    "security".to_string(),
                    "containers".to_string(),
                ],
                disabled: vec![],
                thresholds: ThresholdsConfig {
                    cpu_warning: 70.0,
                    cpu_critical: 90.0,
                    memory_warning: 80.0,
                    memory_critical: 95.0,
                    disk_warning: 80.0,
                    disk_critical: 95.0,
                    temperature_warning: 70.0,
                    temperature_critical: 85.0,
                    gpu_memory_warning: 80.0,
                    gpu_memory_critical: 95.0,
                },
            },
            collector: CollectorConfig {
                interval_secs: args.check_interval,
                retention_hours: args.retention_hours,
                export_prometheus: true,
                prometheus_port: 9090,
            },
            alerts: AlertsConfig {
                enabled: true,
                webhook_url: None,
                email_recipients: vec![],
                slack_webhook: None,
            },
            server: ServerConfig {
                addr: args.http_addr.clone(),
                tls_cert: None,
                tls_key: None,
            },
        })
    } else {
        Ok(toml::from_str(&config_content)?)
    }
}