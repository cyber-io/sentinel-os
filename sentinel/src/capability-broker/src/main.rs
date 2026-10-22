//! Sentinel OS Capability Broker
//!
//! Validates compartment requests against policy and issues capability tokens.
//! Runs as a privileged daemon with access to TPM for attestation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use axum::{
    extract::{State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Utc};
use clap::Parser;
use futures::{SinkExt, StreamExt};
use hex;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use ring::signature::{self, Ed25519KeyPair, KeyPair};
use ring::rand::{SystemRandom, SecureRandom};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::net::UnixListener;
use tokio_util::codec::{Framed, LinesCodec};
use tracing::{debug, error, info, warn, instrument};
use uuid::Uuid;

// ============================================================================
// Data Structures
// ============================================================================

/// Capability that can be granted to a compartment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Raw network access on specific interface/CIDR
    NetworkRaw { interface: String, cidr: String },
    /// Regular network access (TCP/UDP)
    Network { protocol: String, port_range: String },
    /// Filesystem read access
    FilesystemRead { path: String },
    /// Filesystem write access with optional quota
    FilesystemWrite { path: String, quota: Option<String> },
    /// GPU compute access
    GpuCompute { device: Option<String>, partition: Option<String> },
    /// CPU allocation
    Cpu { cores: u32 },
    /// Memory allocation
    Memory { size: String },
    /// Hugepages allocation
    Hugepages { size: String },
    /// Display access (Wayland/X11)
    Display { protocol: String },
    /// Input device access
    Input { device_type: String },
    /// TPM access
    Tpm,
    /// Capability to spawn sub-compartments
    SpawnCompartment,
    /// Access to model registry
    ModelRegistry { read: bool, write: bool },
    /// Access to threat intel database
    ThreatIntel { read: bool, write: bool },
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Capability::NetworkRaw { interface, cidr } => write!(f, "network.raw({},{})", interface, cidr),
            Capability::Network { protocol, port_range } => write!(f, "network.{}({})", protocol, port_range),
            Capability::FilesystemRead { path } => write!(f, "filesystem.read({})", path),
            Capability::FilesystemWrite { path, quota } => {
                if let Some(q) = quota { write!(f, "filesystem.write({},{})", path, q) }
                else { write!(f, "filesystem.write({})", path) }
            }
            Capability::GpuCompute { device, partition } => {
                let dev = device.as_deref().unwrap_or("auto");
                let part = partition.as_deref().unwrap_or("default");
                write!(f, "gpu.compute({}, {})", dev, part)
            }
            Capability::Cpu { cores } => write!(f, "cpu({})", cores),
            Capability::Memory { size } => write!(f, "memory({})", size),
            Capability::Hugepages { size } => write!(f, "hugepages({})", size),
            Capability::Display { protocol } => write!(f, "display({})", protocol),
            Capability::Input { device_type } => write!(f, "input({})", device_type),
            Capability::Tpm => write!(f, "tpm"),
            Capability::SpawnCompartment => write!(f, "spawn_compartment"),
            Capability::ModelRegistry { read, write } => write!(f, "model_registry(r:{},w:{})", read, write),
            Capability::ThreatIntel { read, write } => write!(f, "threat_intel(r:{},w:{})", read, write),
        }
    }
}

impl std::str::FromStr for Capability {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        // Parse capability string like "network.raw(eth0,10.0.0.0/8)"
        if let Some((name, args)) = s.split_once('(') {
            let args = args.strip_suffix(')').unwrap_or(args);
            let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();

            match name {
                "network.raw" if parts.len() == 2 => Ok(Capability::NetworkRaw {
                    interface: parts[0].to_string(),
                    cidr: parts[1].to_string()
                }),
                "network.tcp" | "network.udp" if parts.len() == 1 => Ok(Capability::Network {
                    protocol: name.split('.').nth(1).unwrap().to_string(),
                    port_range: parts[0].to_string()
                }),
                "filesystem.read" if parts.len() == 1 => Ok(Capability::FilesystemRead {
                    path: parts[0].to_string()
                }),
                "filesystem.write" => Ok(Capability::FilesystemWrite {
                    path: parts[0].to_string(),
                    quota: parts.get(1).map(|s| s.to_string()),
                }),
                "gpu.compute" => Ok(Capability::GpuCompute {
                    device: if parts.len() > 0 && !parts[0].is_empty() { Some(parts[0].to_string()) } else { None },
                    partition: if parts.len() > 1 && !parts[1].is_empty() { Some(parts[1].to_string()) } else { None },
                }),
                "cpu" if parts.len() == 1 => Ok(Capability::Cpu {
                    cores: parts[0].parse()?
                }),
                "memory" if parts.len() == 1 => Ok(Capability::Memory {
                    size: parts[0].to_string()
                }),
                "hugepages" if parts.len() == 1 => Ok(Capability::Hugepages {
                    size: parts[0].to_string()
                }),
                "display" if parts.len() == 1 => Ok(Capability::Display {
                    protocol: parts[0].to_string()
                }),
                "input" if parts.len() == 1 => Ok(Capability::Input {
                    device_type: parts[0].to_string()
                }),
                "tpm" => Ok(Capability::Tpm),
                "spawn_compartment" => Ok(Capability::SpawnCompartment),
                "model_registry" => Ok(Capability::ModelRegistry {
                    read: parts.iter().any(|p| p.starts_with("r:true") || p == &"read"),
                    write: parts.iter().any(|p| p.starts_with("w:true") || p == &"write"),
                }),
                "threat_intel" => Ok(Capability::ThreatIntel {
                    read: parts.iter().any(|p| p.starts_with("r:true") || p == &"read"),
                    write: parts.iter().any(|p| p.starts_with("w:true") || p == &"write"),
                }),
                _ => Err(anyhow::anyhow!("Unknown capability: {}", s)),
            }
        } else {
            Err(anyhow::anyhow!("Invalid capability format: {}", s))
        }
    }
}

/// Compartment type definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompartmentType {
    pub name: String,
    pub description: String,
    pub capabilities: Vec<Capability>,
    pub network: Option<String>, // VLAN name
    pub seccomp_profile: Option<String>,
    pub resource_limits: ResourceLimits,
    pub apparmor_profile: Option<String>,
}

/// Resource limits for a compartment
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    pub cpu_cores: Option<u32>,
    pub memory_bytes: Option<u64>,
    pub disk_bytes: Option<u64>,
    pub gpu_devices: Option<u32>,
    pub pids: Option<u32>,
}

/// Policy for a compartment type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompartmentPolicy {
    pub compartment_type: CompartmentType,
    pub allowed_contexts: Vec<String>, // Mission contexts where this type is allowed
    pub requires_approval: bool,
    pub max_instances: Option<u32>,
    pub ttl_seconds: Option<u64>,
}

/// Request to validate capabilities
#[derive(Debug, Serialize, Deserialize)]
pub struct ValidateRequest {
    pub compartment_type: String,
    pub requested_capabilities: Vec<Capability>,
    pub context: Option<String>,
}

/// Response from validation
#[derive(Debug, Serialize, Deserialize)]
pub struct ValidateResponse {
    pub allowed: bool,
    pub granted_capabilities: Vec<Capability>,
    pub denied_capabilities: Vec<Capability>,
    pub message: String,
    pub token: Option<CapabilityToken>,
}

/// Capability token issued by broker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityToken {
    pub token_id: Uuid,
    pub compartment_type: String,
    pub compartment_name: String,
    pub capabilities: Vec<Capability>,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub nonce: String,
    pub signature: String, // Base64 encoded Ed25519 signature
}

/// Request to issue a token
#[derive(Debug, Serialize, Deserialize)]
pub struct IssueTokenRequest {
    pub compartment_type: String,
    pub compartment_name: String,
    pub requested_capabilities: Vec<Capability>,
    pub ttl_seconds: Option<u64>,
    pub context: Option<String>,
}

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    uptime_seconds: u64,
    policies_loaded: usize,
    active_tokens: usize,
}

/// WebSocket message types
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    Validate { request: ValidateRequest },
    IssueToken { request: IssueTokenRequest },
    RevokeToken { token_id: Uuid },
    ListTokens,
    ListPolicies,
    GetPolicy { compartment_type: String },
    ReloadPolicies,
    SubscribeEvents,
    Ping,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsResponse {
    ValidateResponse { response: ValidateResponse },
    TokenIssued { token: CapabilityToken },
    TokenRevoked { token_id: Uuid, success: bool },
    TokenList { tokens: Vec<CapabilityToken> },
    PolicyList { policies: Vec<CompartmentPolicy> },
    Policy { policy: Option<CompartmentPolicy> },
    PoliciesReloaded { count: usize },
    Event { event: BrokerEvent },
    Pong,
    Error { message: String },
}

/// Broker events for subscribers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum BrokerEvent {
    TokenIssued { token: CapabilityToken },
    TokenRevoked { token_id: Uuid, compartment_name: String },
    TokenExpired { token_id: Uuid },
    PolicyChanged { compartment_type: String },
    CompartmentSpawned { compartment_name: String, compartment_type: String },
    CompartmentTerminated { compartment_name: String },
    ValidationRequested { compartment_type: String, allowed: bool },
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Parser, Debug)]
#[command(name = "sentinel-capability-broker", version, about)]
struct Args {
    /// Policy directory
    #[arg(short, long, default_value = "/etc/sentinel/compartments/policies")]
    policy_dir: PathBuf,

    /// Unix socket path
    #[arg(short, long, default_value = "/run/sentinel/capability-broker.sock")]
    socket: PathBuf,

    /// HTTP API address
    #[arg(long, default_value = "127.0.0.1:8081")]
    http_addr: String,

    /// Signing key path
    #[arg(long, default_value = "/etc/sentinel/keys/broker-signing.key")]
    signing_key: PathBuf,

    /// Token TTL default (seconds)
    #[arg(long, default_value = "3600")]
    default_ttl: u64,

    /// Max token TTL (seconds)
    #[arg(long, default_value = "86400")]
    max_ttl: u64,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Enable JSON logging
    #[arg(long)]
    json_logs: bool,
}

#[derive(Clone)]
struct BrokerState {
    policies: Arc<RwLock<HashMap<String, CompartmentPolicy>>>,
    active_tokens: Arc<RwLock<HashMap<Uuid, CapabilityToken>>>,
    signing_key: Arc<Ed25519KeyPair>,
    event_tx: broadcast::Sender<BrokerEvent>,
    policy_dir: PathBuf,
    default_ttl: Duration,
    max_ttl: Duration,
    start_time: SystemTime,
}

// ============================================================================
// Policy Loading
// ============================================================================

async fn load_policies(policy_dir: &Path) -> Result<HashMap<String, CompartmentPolicy>> {
    let mut policies = HashMap::new();

    if !policy_dir.exists() {
        warn!("Policy directory does not exist: {:?}", policy_dir);
        return Ok(policies);
    }

    let mut entries = tokio::fs::read_dir(policy_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            match load_policy_file(&path).await {
                Ok(policy) => {
                    policies.insert(policy.compartment_type.name.clone(), policy);
                    info!("Loaded policy: {}", path.display());
                }
                Err(e) => {
                    error!("Failed to load policy {:?}: {}", path, e);
                }
            }
        }
    }

    info!("Loaded {} policies", policies.len());
    Ok(policies)
}

async fn load_policy_file(path: &Path) -> Result<CompartmentPolicy> {
    let content = tokio::fs::read_to_string(path).await?;
    let policy: CompartmentPolicy = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse policy: {:?}", path))?;
    Ok(policy)
}

async fn watch_policies(policy_dir: PathBuf, state: BrokerState) -> Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                let _ = tx.blocking_send(event);
            }
        },
        notify::Config::default(),
    )?;

    watcher.watch(&policy_dir, RecursiveMode::NonRecursive)?;

    while let Some(event) = rx.recv().await {
        if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)) {
            for path in event.paths {
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    info!("Policy file changed: {:?}, reloading...", path);
                    match load_policies(&policy_dir).await {
                        Ok(new_policies) => {
                            let mut policies = state.policies.write().await;
                            *policies = new_policies;
                            let count = policies.len();
                            let _ = state.event_tx.send(BrokerEvent::PolicyChanged {
                                compartment_type: path.file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("unknown")
                                    .to_string()
                            });
                            info!("Reloaded {} policies", count);
                        }
                        Err(e) => error!("Failed to reload policies: {}", e),
                    }
                }
            }
        }
    }

    Ok(())
}

// ============================================================================
// Token Management
// ============================================================================

fn generate_nonce() -> String {
    let mut bytes = [0u8; 16];
    SystemRandom::new().fill(&mut bytes).unwrap();
    hex::encode(bytes)
}

fn sign_token(signing_key: &Ed25519KeyPair, token_data: &[u8]) -> String {
    let sig = signing_key.sign(token_data);
    BASE64.encode(sig.as_ref())
}

fn verify_token(signing_key: &Ed25519KeyPair, token_data: &[u8], signature: &str) -> bool {
    let sig_bytes = match BASE64.decode(signature) {
        Ok(b) => b,
        Err(_) => return false,
    };
    signature::UnparsedPublicKey::new(&signature::ED25519, signing_key.public_key().as_ref())
        .verify(token_data, &sig_bytes)
        .is_ok()
}

async fn issue_token(
    state: &BrokerState,
    compartment_type: &str,
    compartment_name: &str,
    capabilities: Vec<Capability>,
    ttl_seconds: Option<u64>,
) -> Result<CapabilityToken> {
    let policies = state.policies.read().await;
    let policy = policies.get(compartment_type)
        .ok_or_else(|| anyhow::anyhow!("Unknown compartment type: {}", compartment_type))?;

    // Validate requested capabilities against policy
    let mut granted = Vec::new();
    let mut denied = Vec::new();

    for cap in &capabilities {
        if policy.compartment_type.capabilities.iter().any(|allowed| allowed == cap) {
            granted.push(cap.clone());
        } else {
            denied.push(cap.clone());
        }
    }

    if !denied.is_empty() {
        return Err(anyhow::anyhow!("Denied capabilities: {:?}", denied));
    }

    // Check instance limits
    if let Some(max) = policy.max_instances {
        let active_count = state.active_tokens.read().await
            .values()
            .filter(|t| t.compartment_type == compartment_type)
            .count() as u32;
        if active_count >= max {
            return Err(anyhow::anyhow!("Max instances reached for {}", compartment_type));
        }
    }

    // Check approval requirement
    if policy.requires_approval {
        // In production, this would integrate with an approval system
        warn!("Compartment type {} requires approval (not implemented)", compartment_type);
    }

    // Calculate TTL
    let ttl = ttl_seconds
        .map(Duration::from_secs)
        .unwrap_or(state.default_ttl)
        .min(state.max_ttl);

    let now = SystemTime::now();
    let issued_at = DateTime::<Utc>::from(now);
    let expires_at = DateTime::<Utc>::from(now + ttl);

    let token_id = Uuid::new_v4();
    let nonce = generate_nonce();

    // Create token data for signing
    let token_data = serde_json::json!({
        "token_id": token_id,
        "compartment_type": compartment_type,
        "compartment_name": compartment_name,
        "capabilities": granted,
        "issued_at": issued_at,
        "expires_at": expires_at,
        "nonce": nonce,
    }).to_string();

    let signature = sign_token(&state.signing_key, token_data.as_bytes());

    let token = CapabilityToken {
        token_id,
        compartment_type: compartment_type.to_string(),
        compartment_name: compartment_name.to_string(),
        capabilities: granted,
        issued_at,
        expires_at,
        nonce,
        signature,
    };

    // Store token
    state.active_tokens.write().await.insert(token_id, token.clone());

    // Emit event
    let _ = state.event_tx.send(BrokerEvent::TokenIssued { token: token.clone() });

    info!("Issued token {} for {} ({})", token_id, compartment_name, compartment_type);
    Ok(token)
}

async fn revoke_token(state: &BrokerState, token_id: Uuid) -> bool {
    let mut tokens = state.active_tokens.write().await;
    if let Some(token) = tokens.remove(&token_id) {
        let _ = state.event_tx.send(BrokerEvent::TokenRevoked {
            token_id,
            compartment_name: token.compartment_name
        });
        info!("Revoked token {}", token_id);
        true
    } else {
        false
    }
}

async fn cleanup_expired_tokens(state: &BrokerState) {
    let now = DateTime::<Utc>::from(SystemTime::now());
    let mut tokens = state.active_tokens.write().await;
    let expired: Vec<_> = tokens.iter()
        .filter(|(_, t)| t.expires_at < now)
        .map(|(id, _)| *id)
        .collect();

    for id in expired {
        if let Some(token) = tokens.remove(&id) {
            let _ = state.event_tx.send(BrokerEvent::TokenExpired { token_id: id });
            debug!("Expired token {}", id);
        }
    }
}

// ============================================================================
// Request Handlers
// ============================================================================

#[instrument(skip(state))]
async fn handle_validate(
    State(state): State<BrokerState>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, StatusCode> {
    let policies = state.policies.read().await;
    let policy = match policies.get(&req.compartment_type) {
        Some(p) => p,
        None => {
            return Ok(Json(ValidateResponse {
                allowed: false,
                granted_capabilities: vec![],
                denied_capabilities: req.requested_capabilities.clone(),
                message: format!("Unknown compartment type: {}", req.compartment_type),
                token: None,
            }));
        }
    };

    // Check context
    if let Some(ctx) = &req.context {
        if !policy.allowed_contexts.is_empty() && !policy.allowed_contexts.contains(ctx) {
            return Ok(Json(ValidateResponse {
                allowed: false,
                granted_capabilities: vec![],
                denied_capabilities: req.requested_capabilities.clone(),
                message: format!("Context '{}' not allowed for compartment type '{}'", ctx, req.compartment_type),
                token: None,
            }));
        }
    }

    let mut granted = Vec::new();
    let mut denied = Vec::new();

    for cap in &req.requested_capabilities {
        if policy.compartment_type.capabilities.iter().any(|allowed| allowed == cap) {
            granted.push(cap.clone());
        } else {
            denied.push(cap.clone());
        }
    }

    let allowed = denied.is_empty();
    let message = if allowed {
        "All capabilities granted".to_string()
    } else {
        format!("Denied capabilities: {:?}", denied)
    };

    Ok(Json(ValidateResponse {
        allowed,
        granted_capabilities: granted,
        denied_capabilities: denied,
        message,
        token: None,
    }))
}

#[instrument(skip(state))]
async fn handle_issue_token(
    State(state): State<BrokerState>,
    Json(req): Json<IssueTokenRequest>,
) -> Result<Json<CapabilityToken>, StatusCode> {
    match issue_token(
        &state,
        &req.compartment_type,
        &req.compartment_name,
        req.requested_capabilities,
        req.ttl_seconds,
    ).await {
        Ok(token) => Ok(Json(token)),
        Err(e) => {
            error!("Failed to issue token: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

#[instrument(skip(state))]
async fn handle_revoke_token(
    State(state): State<BrokerState>,
    axum::extract::Path(token_id): axum::extract::Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let success = revoke_token(&state, token_id).await;
    Ok(Json(serde_json::json!({ "success": success })))
}

async fn handle_list_tokens(State(state): State<BrokerState>) -> Json<Vec<CapabilityToken>> {
    let tokens = state.active_tokens.read().await;
    Json(tokens.values().cloned().collect())
}

async fn handle_list_policies(State(state): State<BrokerState>) -> Json<Vec<CompartmentPolicy>> {
    let policies = state.policies.read().await;
    Json(policies.values().cloned().collect())
}

async fn handle_get_policy(
    State(state): State<BrokerState>,
    axum::extract::Path(compartment_type): axum::extract::Path<String>,
) -> Json<Option<CompartmentPolicy>> {
    let policies = state.policies.read().await;
    Json(policies.get(&compartment_type).cloned())
}

async fn handle_reload_policies(State(state): State<BrokerState>) -> Json<serde_json::Value> {
    match load_policies(&state.policy_dir).await {
        Ok(new_policies) => {
            let count = new_policies.len();
            *state.policies.write().await = new_policies;
            let _ = state.event_tx.send(BrokerEvent::PolicyChanged {
                compartment_type: "all".to_string()
            });
            Json(serde_json::json!({ "success": true, "count": count }))
        }
        Err(e) => {
            error!("Failed to reload policies: {}", e);
            Json(serde_json::json!({ "success": false, "error": e.to_string() }))
        }
    }
}

async fn handle_health(State(state): State<BrokerState>) -> Json<HealthResponse> {
    let uptime = SystemTime::now().duration_since(state.start_time).unwrap_or_default();
    let policies = state.policies.read().await;
    let tokens = state.active_tokens.read().await;

    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime.as_secs(),
        policies_loaded: policies.len(),
        active_tokens: tokens.len(),
    })
}

// ============================================================================
// WebSocket Handler
// ============================================================================

async fn handle_ws(
    ws: WebSocketUpgrade,
    State(state): State<BrokerState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_connection(socket, state))
}

async fn handle_ws_connection(socket: axum::extract::ws::WebSocket, state: BrokerState) {
    let (mut sender, mut receiver) = socket.split();
    let mut event_rx = state.event_tx.subscribe();
    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));

    // Send initial policy list
    let policies = state.policies.read().await;
    let policy_list = WsResponse::PolicyList {
        policies: policies.values().cloned().collect()
    };
    if let Ok(msg) = serde_json::to_string(&policy_list) {
        let _ = sender.send(axum::extract::ws::Message::Text(msg.into())).await;
    }

    loop {
        tokio::select! {
            // Handle incoming messages
            msg = receiver.next() => {
                match msg {
                    Some(Ok(axum::extract::ws::Message::Text(text))) => {
                        if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text) {
                            let response = handle_ws_message(ws_msg, &state).await;
                            if let Ok(msg) = serde_json::to_string(&response) {
                                let _ = sender.send(axum::extract::ws::Message::Text(msg.into())).await;
                            }
                        }
                    }
                    Some(Ok(axum::extract::ws::Message::Close(_))) => {
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
            // Handle broker events
            event = event_rx.recv() => {
                if let Ok(event) = event {
                    let response = WsResponse::Event { event };
                    if let Ok(msg) = serde_json::to_string(&response) {
                        let _ = sender.send(axum::extract::ws::Message::Text(msg.into())).await;
                    }
                }
            }
            // Send periodic ping
            _ = ping_interval.tick() => {
                let ping = WsResponse::Pong;
                if let Ok(msg) = serde_json::to_string(&ping) {
                    if sender.send(axum::extract::ws::Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
}

async fn handle_ws_message(msg: WsMessage, state: &BrokerState) -> WsResponse {
    match msg {
        WsMessage::Validate { request } => {
            let response = handle_validate(State(state.clone()), Json(request)).await;
            match response {
                Ok(Json(r)) => WsResponse::ValidateResponse { response: r },
                Err(_) => WsResponse::Error { message: "Validation failed".to_string() },
            }
        }
        WsMessage::IssueToken { request } => {
            match issue_token(state, &request.compartment_type, &request.compartment_name,
                request.requested_capabilities, request.ttl_seconds).await {
                Ok(token) => WsResponse::TokenIssued { token },
                Err(e) => WsResponse::Error { message: e.to_string() },
            }
        }
        WsMessage::RevokeToken { token_id } => {
            let success = revoke_token(state, token_id).await;
            WsResponse::TokenRevoked { token_id, success }
        }
        WsMessage::ListTokens => {
            let tokens = state.active_tokens.read().await.values().cloned().collect();
            WsResponse::TokenList { tokens }
        }
        WsMessage::ListPolicies => {
            let policies = state.policies.read().await.values().cloned().collect();
            WsResponse::PolicyList { policies }
        }
        WsMessage::GetPolicy { compartment_type } => {
            let policy = state.policies.read().await.get(&compartment_type).cloned();
            WsResponse::Policy { policy }
        }
        WsMessage::ReloadPolicies => {
            match load_policies(&state.policy_dir).await {
                Ok(new_policies) => {
                    let count = new_policies.len();
                    *state.policies.write().await = new_policies;
                    WsResponse::PoliciesReloaded { count }
                }
                Err(e) => WsResponse::Error { message: e.to_string() },
            }
        }
        WsMessage::SubscribeEvents => {
            // Events are automatically sent via broadcast channel
            WsResponse::PolicyList { policies: vec![] } // Acknowledge
        }
        WsMessage::Ping => WsResponse::Pong,
    }
}

// ============================================================================
// Unix Socket Handler (for CLI)
// ============================================================================

async fn run_unix_socket_server(socket_path: PathBuf, state: BrokerState) -> Result<()> {
    // Remove existing socket
    if socket_path.exists() {
        tokio::fs::remove_file(&socket_path).await.ok();
    }

    // Ensure parent directory exists
    if let Some(parent) = socket_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let listener = UnixListener::bind(&socket_path)?;
    info!("Unix socket listening on {:?}", socket_path);

    // Set socket permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&socket_path).await?.permissions();
        perms.set_mode(0o660);
        tokio::fs::set_permissions(&socket_path, perms).await?;
    }

    loop {
        let (stream, _) = listener.accept().await?;
        let state = state.clone();

        tokio::spawn(async move {
            let mut framed = Framed::new(stream, LinesCodec::new());
            while let Some(line) = framed.next().await {
                match line {
                    Ok(line) => {
                        if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&line) {
                            let response = handle_ws_message(ws_msg, &state).await;
                            if let Ok(msg) = serde_json::to_string(&response) {
                                let _ = framed.send(msg).await;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Unix socket error: {}", e);
                        break;
                    }
                }
            }
        });
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| args.log_level.into());

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    if args.json_logs {
        subscriber.json().init();
    } else {
        subscriber.init();
    }

    info!("Starting Sentinel Capability Broker v{}", env!("CARGO_PKG_VERSION"));

    // Load signing key
    let signing_key_bytes = tokio::fs::read(&args.signing_key).await
        .with_context(|| format!("Failed to read signing key: {:?}", args.signing_key))?;
    let signing_key = Ed25519KeyPair::from_pkcs8(&signing_key_bytes)
        .map_err(|_| anyhow::anyhow!("Invalid Ed25519 private key"))?;

    // Load policies
    let policies = load_policies(&args.policy_dir).await?;

    // Create shared state
    let (event_tx, _) = broadcast::channel(1000);
    let state = BrokerState {
        policies: Arc::new(RwLock::new(policies)),
        active_tokens: Arc::new(RwLock::new(HashMap::new())),
        signing_key: Arc::new(signing_key),
        event_tx,
        policy_dir: args.policy_dir.clone(),
        default_ttl: Duration::from_secs(args.default_ttl),
        max_ttl: Duration::from_secs(args.max_ttl),
        start_time: SystemTime::now(),
    };

    // Start policy watcher
    let watcher_state = state.clone();
    let policy_dir = args.policy_dir.clone();
    tokio::spawn(async move {
        if let Err(e) = watch_policies(policy_dir, watcher_state).await {
            error!("Policy watcher error: {}", e);
        }
    });

    // Start token cleanup task
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            cleanup_expired_tokens(&cleanup_state).await;
        }
    });

    // Build HTTP router
    let app = Router::new()
        .route("/health", get(handle_health))
        .route("/validate", post(handle_validate))
        .route("/tokens", post(handle_issue_token))
        .route("/tokens", get(handle_list_tokens))
        .route("/tokens/:id", axum::routing::delete(handle_revoke_token))
        .route("/policies", get(handle_list_policies))
        .route("/policies/:type", get(handle_get_policy))
        .route("/policies/reload", post(handle_reload_policies))
        .route("/ws", get(handle_ws))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state.clone());

    // Start HTTP server
    let http_addr = args.http_addr.clone();
    let http_state = state.clone();
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(&http_addr).await.unwrap();
        info!("HTTP API listening on {}", http_addr);
        axum::serve(listener, app).await.unwrap();
    });

    // Start Unix socket server (blocking)
    run_unix_socket_server(args.socket, state).await?;

    Ok(())
}