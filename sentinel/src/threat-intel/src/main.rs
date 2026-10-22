//! Sentinel OS Threat Intelligence Engine
//!
//! ML-based threat detection (ONNX) with LLM-powered explanations.

mod detector;
mod explainer;
mod feeds;
mod api;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use tokio::sync::RwLock;
use tracing::{info, error};

use detector::ThreatDetector;
use explainer::ThreatExplainer;
use feeds::FeedManager;
use api::{create_router, AppState};

#[derive(Parser, Debug)]
#[command(name = "sentinel-threat-intel", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/etc/sentinel/threat-intel.toml")]
    config: PathBuf,

    /// HTTP API address
    #[arg(long, default_value = "127.0.0.1:8085")]
    http_addr: String,

    /// Model directory
    #[arg(long, default_value = "/var/lib/sentinel/models/threat")]
    model_dir: PathBuf,

    /// Feed configurations directory
    #[arg(long, default_value = "/etc/sentinel/feeds")]
    feeds_dir: PathBuf,

    /// Inference gateway URL
    #[arg(long, default_value = "http://127.0.0.1:11434")]
    inference_gateway: String,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Config {
    detector: DetectorConfig,
    explainer: ExplainerConfig,
    feeds: FeedsConfig,
    server: ServerConfig,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct DetectorConfig {
    models: Vec<ModelConfig>,
    threshold: f32,
    batch_size: usize,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ModelConfig {
    name: String,
    path: PathBuf,
    input_shape: Vec<usize>,
    labels: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ExplainerConfig {
    model: String,
    temperature: f32,
    max_tokens: usize,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct FeedsConfig {
    enabled: bool,
    update_interval_secs: u64,
    sources: Vec<FeedSource>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct FeedSource {
    name: String,
    url: String,
    format: String, // stix, misp, json, csv
    api_key: Option<String>,
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

    info!("Starting Sentinel Threat Intelligence Engine v{}", env!("CARGO_PKG_VERSION"));

    // Load config
    let config = load_config(&args).await?;

    // Initialize detector
    let detector = Arc::new(RwLock::new(ThreatDetector::new(&config.detector).await?));

    // Initialize explainer
    let explainer = Arc::new(ThreatExplainer::new(&config.explainer, args.inference_gateway).await?);

    // Initialize feed manager
    let feeds = Arc::new(FeedManager::new(&config.feeds).await?);

    // Start feed updates
    let feeds_clone = feeds.clone();
    tokio::spawn(async move {
        feeds_clone.start_updates().await;
    });

    // Create app state
    let state = AppState {
        detector,
        explainer,
        feeds,
        config: config.clone(),
        start_time: SystemTime::now(),
    };

    // Build router
    let app = create_router(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.server.addr).await?;
    info!("Threat Intelligence API listening on {}", config.server.addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn load_config(args: &Args) -> Result<Config> {
    let config_content = tokio::fs::read_to_string(&args.config).await
        .unwrap_or_else(|_| String::new());

    if config_content.is_empty() {
        Ok(Config {
            detector: DetectorConfig {
                models: vec![
                    ModelConfig {
                        name: "network-ids".to_string(),
                        path: args.model_dir.join("network-ids.onnx"),
                        input_shape: vec![1, 78],
                        labels: vec!["benign".to_string(), "malicious".to_string()],
                    },
                    ModelConfig {
                        name: "malware-classifier".to_string(),
                        path: args.model_dir.join("malware-classifier.onnx"),
                        input_shape: vec![1, 2381],
                        labels: vec!["benign".to_string(), "trojan".to_string(), "ransomware".to_string(), "miner".to_string()],
                    },
                ],
                threshold: 0.7,
                batch_size: 32,
            },
            explainer: ExplainerConfig {
                model: "sentinel-analyst".to_string(),
                temperature: 0.3,
                max_tokens: 1024,
            },
            feeds: FeedsConfig {
                enabled: true,
                update_interval_secs: 3600,
                sources: vec![
                    FeedSource {
                        name: "alienvault-otx".to_string(),
                        url: "https://otx.alienvault.com/api/v1/pulses/subscribed".to_string(),
                        format: "json".to_string(),
                        api_key: None,
                    },
                    FeedSource {
                        name: "misp".to_string(),
                        url: "https://misp.example.com/events/restSearch".to_string(),
                        format: "misp".to_string(),
                        api_key: None,
                    },
                ],
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