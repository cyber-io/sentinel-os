//! Sentinel OS Attestation Service
//!
//! TPM-based remote attestation with IMA/EVM integration.

mod tpm;
mod ima;
mod pcr;
mod quote;
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

use tpm::TpmManager;
use ima::ImaManager;
use pcr::PcrManager;
use quote::QuoteService;
use api::{create_router, AppState};

#[derive(Parser, Debug)]
#[command(name = "sentinel-attestation", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/etc/sentinel/attestation.toml")]
    config: PathBuf,

    /// HTTP API address
    #[arg(long, default_value = "127.0.0.1:8086")]
    http_addr: String,

    /// TPM device path
    #[arg(long, default_value = "/dev/tpmrm0")]
    tpm_device: String,

    /// IMA log path
    #[arg(long, default_value = "/sys/kernel/security/ima/ascii_runtime_measurements")]
    ima_log: PathBuf,

    /// EVM log path
    #[arg(long, default_value = "/sys/kernel/security/evm/ascii_runtime_measurements")]
    evm_log: PathBuf,

    /// PCR policy file
    #[arg(long, default_value = "/etc/sentinel/pcr-policy.toml")]
    pcr_policy: PathBuf,

    /// Quote signing key
    #[arg(long)]
    signing_key: Option<PathBuf>,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Config {
    tpm: TpmConfig,
    ima: ImaConfig,
    pcr: PcrConfig,
    quote: QuoteConfig,
    server: ServerConfig,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct TpmConfig {
    device: String,
    tcti: String,
    owner_auth: Option<String>,
    endorsement_auth: Option<String>,
    lockout_auth: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ImaConfig {
    log_path: PathBuf,
    evm_log_path: PathBuf,
    verify_signatures: bool,
    allowed_keys: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct PcrConfig {
    policy_file: PathBuf,
    monitored_pcrs: Vec<u32>,
    alert_on_change: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct QuoteConfig {
    signing_key: Option<PathBuf>,
    aik_name: String,
    quote_format: String, // tpm2b, json
    include_ima_log: bool,
    include_event_log: bool,
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

    info!("Starting Sentinel Attestation Service v{}", env!("CARGO_PKG_VERSION"));

    // Load config
    let config = load_config(&args).await?;

    // Initialize TPM manager
    let tpm = Arc::new(RwLock::new(TpmManager::new(&config.tpm).await?));

    // Initialize IMA manager
    let ima = Arc::new(ImaManager::new(&config.ima).await?);

    // Initialize PCR manager
    let pcr = Arc::new(PcrManager::new(&config.pcr).await?);

    // Initialize quote service
    let quote = Arc::new(QuoteService::new(
        tpm.clone(),
        ima.clone(),
        pcr.clone(),
        &config.quote,
    ).await?);

    // Start PCR monitoring
    let pcr_clone = pcr.clone();
    tokio::spawn(async move {
        pcr_clone.start_monitoring().await;
    });

    // Create app state
    let state = AppState {
        tpm,
        ima,
        pcr,
        quote,
        config: config.clone(),
        start_time: SystemTime::now(),
    };

    // Build router
    let app = create_router(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.server.addr).await?;
    info!("Attestation Service API listening on {}", config.server.addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn load_config(args: &Args) -> Result<Config> {
    let config_content = tokio::fs::read_to_string(&args.config).await
        .unwrap_or_else(|_| String::new());

    if config_content.is_empty() {
        Ok(Config {
            tpm: TpmConfig {
                device: args.tpm_device.clone(),
                tcti: "device".to_string(),
                owner_auth: None,
                endorsement_auth: None,
                lockout_auth: None,
            },
            ima: ImaConfig {
                log_path: args.ima_log.clone(),
                evm_log_path: args.evm_log.clone(),
                verify_signatures: true,
                allowed_keys: vec![],
            },
            pcr: PcrConfig {
                policy_file: args.pcr_policy.clone(),
                monitored_pcrs: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
                alert_on_change: true,
            },
            quote: QuoteConfig {
                signing_key: args.signing_key.clone(),
                aik_name: "sentinel-aik".to_string(),
                quote_format: "json".to_string(),
                include_ima_log: true,
                include_event_log: true,
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