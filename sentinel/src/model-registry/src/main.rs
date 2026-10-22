//! Sentinel OS Model Registry
//!
//! ORAS-based signed model distribution with cosign verification.

mod registry;
mod storage;
mod verification;
mod api;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::Result;
use axum::{
    routing::{get, post, put, delete},
    Router,
};
use clap::Parser;
use tokio::sync::RwLock;
use tracing::{info, error};

use registry::ModelRegistry;
use storage::StorageBackend;
use verification::VerificationService;
use api::{create_router, AppState};

#[derive(Parser, Debug)]
#[command(name = "sentinel-model-registry", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/etc/sentinel/model-registry.toml")]
    config: PathBuf,

    /// HTTP API address
    #[arg(long, default_value = "127.0.0.1:8084")]
    http_addr: String,

    /// Storage backend (fs, s3, oras)
    #[arg(long, default_value = "fs")]
    storage: String,

    /// Storage path
    #[arg(long, default_value = "/var/lib/sentinel/models")]
    storage_path: PathBuf,

    /// ORAS registry URL
    #[arg(long, default_value = "localhost:5000")]
    oras_registry: String,

    /// Cosign public key path
    #[arg(long)]
    cosign_pubkey: Option<PathBuf>,

    /// Cosign private key path
    #[arg(long)]
    cosign_privkey: Option<PathBuf>,

    /// Fulcio/Root CA for keyless signing
    #[arg(long)]
    fulcio_ca: Option<PathBuf>,

    /// Rekor transparency log URL
    #[arg(long, default_value = "https://rekor.sigstore.dev")]
    rekor_url: String,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Config {
    storage: StorageConfig,
    oras: OrasConfig,
    cosign: CosignConfig,
    server: ServerConfig,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct StorageConfig {
    backend: String,
    path: PathBuf,
    s3_bucket: Option<String>,
    s3_region: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OrasConfig {
    registry: String,
    username: Option<String>,
    password: Option<String>,
    insecure: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct CosignConfig {
    pubkey_path: Option<PathBuf>,
    privkey_path: Option<PathBuf>,
    fulcio_ca: Option<PathBuf>,
    rekor_url: String,
    keyless: bool,
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

    info!("Starting Sentinel Model Registry v{}", env!("CARGO_PKG_VERSION"));

    // Load config
    let config = load_config(&args).await?;

    // Initialize storage backend
    let storage = StorageBackend::new(&config.storage).await?;

    // Initialize verification service
    let verification = VerificationService::new(&config.cosign).await?;

    // Initialize model registry
    let registry = Arc::new(RwLock::new(ModelRegistry::new(
        storage,
        verification,
        config.oras.clone(),
    ).await?));

    // Create app state
    let state = AppState {
        registry: registry.clone(),
        config: config.clone(),
        start_time: SystemTime::now(),
    };

    // Build router
    let app = create_router(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.server.addr).await?;
    info!("Model Registry API listening on {}", config.server.addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn load_config(args: &Args) -> Result<Config> {
    let config_content = tokio::fs::read_to_string(&args.config).await
        .unwrap_or_else(|_| String::new());

    if config_content.is_empty() {
        // Use defaults from args
        Ok(Config {
            storage: StorageConfig {
                backend: args.storage.clone(),
                path: args.storage_path.clone(),
                s3_bucket: None,
                s3_region: None,
            },
            oras: OrasConfig {
                registry: args.oras_registry.clone(),
                username: None,
                password: None,
                insecure: true,
            },
            cosign: CosignConfig {
                pubkey_path: args.cosign_pubkey.clone(),
                privkey_path: args.cosign_privkey.clone(),
                fulcio_ca: args.fulcio_ca.clone(),
                rekor_url: args.rekor_url.clone(),
                keyless: args.cosign_pubkey.is_none() && args.cosign_privkey.is_none(),
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