//! Sentinel OS Compartment Manager
//!
//! Spawns and manages isolated compartments (VMs/containers) with capability-based access control.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use axum::{
    extract::{State, Path as AxumPath},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post, delete},
    Router,
};
use clap::Parser;
use futures::StreamExt;
use tokio::process::Command;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn, instrument};
use uuid::Uuid;

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CompartmentBackend {
    Crosvm,
    Qemu,
    Kata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompartmentSpec {
    pub name: String,
    pub compartment_type: String,
    pub backend: CompartmentBackend,
    pub capabilities: Vec<String>, // Capability strings
    pub resources: ResourceLimits,
    pub network: Option<String>, // VLAN name
    pub disk_image: Option<PathBuf>,
    pub kernel_args: Vec<String>,
    pub env_vars: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    pub cpu_cores: Option<u32>,
    pub memory_bytes: Option<u64>,
    pub disk_bytes: Option<u64>,
    pub gpu_devices: Option<u32>,
    pub pids: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompartmentStatus {
    pub name: String,
    pub compartment_type: String,
    pub backend: CompartmentBackend,
    pub state: CompartmentState,
    pub pid: Option<u32>,
    pub ip_address: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resources: ResourceLimits,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CompartmentState {
    Creating,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
    Destroying,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRequest {
    pub spec: CompartmentSpec,
    pub capability_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateResponse {
    pub success: bool,
    pub compartment: Option<CompartmentStatus>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActionResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListResponse {
    pub compartments: Vec<CompartmentStatus>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LogsResponse {
    pub logs: Vec<String>,
    pub truncated: bool,
}

#[derive(Clone)]
struct ManagerState {
    compartments: Arc<RwLock<HashMap<String, CompartmentInstance>>>,
    config: ManagerConfig,
    capability_broker_url: String,
    start_time: SystemTime,
}

struct CompartmentInstance {
    status: CompartmentStatus,
    handle: Option<CompartmentHandle>,
    log_buffer: Arc<RwLock<Vec<String>>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

#[derive(Debug)]
enum CompartmentHandle {
    Crosvm { child: tokio::process::Child, socket: PathBuf },
    Qemu { child: tokio::process::Child, pid: u32 },
    Kata { container_id: String },
}

#[derive(Parser, Debug)]
#[command(name = "sentinel-compartment-manager", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/etc/sentinel/compartment-manager.toml")]
    config: PathBuf,

    /// HTTP API address
    #[arg(long, default_value = "127.0.0.1:8082")]
    http_addr: String,

    /// Capability broker URL
    #[arg(long, default_value = "http://127.0.0.1:8081")]
    capability_broker: String,

    /// Storage pool path
    #[arg(long, default_value = "/var/lib/sentinel/compartments")]
    storage_pool: PathBuf,

    /// Default CPU cores
    #[arg(long, default_value = "2")]
    default_cpu: u32,

    /// Default memory (bytes)
    #[arg(long, default_value = "4294967296")] // 4GB
    default_memory: u64,

    /// Default disk size (bytes)
    #[arg(long, default_value = "10737418240")] // 10GB
    default_disk: u64,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[derive(Debug, Deserialize)]
struct ManagerConfig {
    default_backend: CompartmentBackend,
    kernel_path: PathBuf,
    initrd_path: Option<PathBuf>,
    base_disk_image: PathBuf,
    network_bridge_prefix: String,
    gpu_passthrough: bool,
}

// ============================================================================
// Compartment Backend Implementations
// ============================================================================

#[async_trait::async_trait]
trait Backend: Send + Sync {
    async fn create(&self, spec: &CompartmentSpec, config: &ManagerConfig, storage: &Path) -> Result<CompartmentHandle>;
    async fn start(&self, handle: &mut CompartmentHandle) -> Result<()>;
    async fn stop(&self, handle: &CompartmentHandle, force: bool) -> Result<()>;
    async fn get_status(&self, handle: &CompartmentHandle) -> Result<CompartmentState>;
    async fn get_logs(&self, handle: &CompartmentHandle, lines: usize) -> Result<Vec<String>>;
    async fn exec(&self, handle: &CompartmentHandle, cmd: &[String]) -> Result<String>;
}

struct CrosvmBackend;
struct QemuBackend;
struct KataBackend;

#[async_trait::async_trait]
impl Backend for CrosvmBackend {
    async fn create(&self, spec: &CompartmentSpec, config: &ManagerConfig, storage: &Path) -> Result<CompartmentHandle> {
        let disk_path = storage.join(format!("{}.qcow2", spec.name));

        // Create disk image if needed
        if !disk_path.exists() {
            if let Some(base) = spec.disk_image.as_ref() {
                tokio::fs::copy(base, &disk_path).await?;
            } else {
                // Create from base image
                tokio::fs::copy(&config.base_disk_image, &disk_path).await?;
            }

            // Resize if needed
            if let Some(size) = spec.resources.disk_bytes {
                let size_gb = (size as f64 / 1_073_741_824.0).ceil() as u64;
                Command::new("qemu-img")
                    .args(["resize", disk_path.to_str().unwrap(), &format!("{}G", size_gb)])
                    .output().await?;
            }
        }

        // Build crosvm command
        let mut cmd = Command::new("crosvm");
        cmd.args(["run", "--disable-sandbox=false"]);

        // Resources
        cmd.arg("--cpus").arg(spec.resources.cpu_cores.unwrap_or(2).to_string());
        cmd.arg("--memory").arg(
            spec.resources.memory_bytes
                .unwrap_or(4_294_967_296)
                .to_string()
        );

        // Disk
        cmd.arg("--rwdisk").arg(disk_path.to_str().unwrap());

        // Shared directories
        cmd.arg("--shared-dir").arg("/nix/store:/nix/store:ro");
        cmd.arg("--shared-dir").arg("/var/lib/sentinel/models:/models:ro");

        // GPU
        if config.gpu_passthrough && spec.resources.gpu_devices.unwrap_or(0) > 0 {
            cmd.arg("--gpu").arg("auto");
        }

        // Network
        if let Some(vlan) = &spec.network {
            let bridge = format!("{}{}", config.network_bridge_prefix, vlan);
            cmd.arg("--network").arg(format!("bridge={}", bridge));
        }

        // VSOCK for communication
        let vsock_port = 10000 + rand::random::<u16>() % 1000;
        cmd.arg("--vsock-port").arg(vsock_port.to_string());

        // Kernel
        cmd.arg(config.kernel_path.to_str().unwrap());

        // Kernel command line
        let mut kernel_args = vec![
            "console=hvc0".to_string(),
            "root=/dev/vda1".to_string(),
            "ro".to_string(),
            format!("sentinel.compartment_type={}", spec.compartment_type),
            format!("sentinel.compartment_name={}", spec.name),
        ];
        kernel_args.extend(spec.kernel_args.clone());
        cmd.arg("-p").arg(kernel_args.join(" "));

        // Environment
        for (k, v) in &spec.env_vars {
            cmd.env(k, v);
        }

        info!("Starting crosvm for compartment: {}", spec.name);
        debug!("Command: {:?}", cmd);

        let mut child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0);

        // Create vsock socket path for communication
        let socket = PathBuf::from(format!("/run/sentinel/compartments/{}.vsock", spec.name));

        Ok(CompartmentHandle::Crosvm { child, socket })
    }

    async fn start(&self, handle: &mut CompartmentHandle) -> Result<()> {
        // crosvm starts immediately on create
        Ok(())
    }

    async fn stop(&self, handle: &CompartmentHandle, force: bool) -> Result<()> {
        match handle {
            CompartmentHandle::Crosvm { child, .. } => {
                if force {
                    child.kill().await?;
                } else {
                    child.terminate().await?;
                }
                child.wait().await?;
            }
            _ => return Err(anyhow::anyhow!("Invalid handle type for CrosvmBackend")),
        }
        Ok(())
    }

    async fn get_status(&self, handle: &CompartmentHandle) -> Result<CompartmentState> {
        match handle {
            CompartmentHandle::Crosvm { child, .. } => {
                match child.try_wait()? {
                    Some(status) => {
                        if status.success() {
                            Ok(CompartmentState::Stopped)
                        } else {
                            Ok(CompartmentState::Failed)
                        }
                    }
                    None => Ok(CompartmentState::Running),
                }
            }
            _ => Err(anyhow::anyhow!("Invalid handle type")),
        }
    }

    async fn get_logs(&self, handle: &CompartmentHandle, lines: usize) -> Result<Vec<String>> {
        // In production, would read from vsock or log file
        Ok(vec!["[crosvm] Logs would be retrieved via vsock".to_string()])
    }

    async fn exec(&self, handle: &CompartmentHandle, cmd: &[String]) -> Result<String> {
        // Execute via vsock
        Ok(format!("[crosvm] Would execute: {:?}", cmd))
    }
}

#[async_trait::async_trait]
impl Backend for QemuBackend {
    async fn create(&self, spec: &CompartmentSpec, config: &ManagerConfig, storage: &Path) -> Result<CompartmentHandle> {
        let disk_path = storage.join(format!("{}.qcow2", spec.name));

        if !disk_path.exists() {
            if let Some(base) = spec.disk_image.as_ref() {
                tokio::fs::copy(base, &disk_path).await?;
            } else {
                tokio::fs::copy(&config.base_disk_image, &disk_path).await?;
            }
        }

        let mut cmd = Command::new("qemu-system-x86_64");

        // Resources
        cmd.args([
            "-m", &spec.resources.memory_bytes.unwrap_or(4_294_967_296).to_string(),
            "-smp", &spec.resources.cpu_cores.unwrap_or(2).to_string(),
            "-drive", &format!("file={},format=qcow2,if=virtio", disk_path.display()),
            "-kernel", config.kernel_path.to_str().unwrap(),
        ]);

        if let Some(initrd) = &config.initrd_path {
            cmd.args(["-initrd", initrd.to_str().unwrap()]);
        }

        // Network
        if let Some(vlan) = &spec.network {
            let bridge = format!("{}{}", config.network_bridge_prefix, vlan);
            cmd.args(["-netdev", &format!("bridge,id=net0,br={}", bridge), "-device", "virtio-net-pci,netdev=net0"]);
        }

        // Kernel args
        let mut kernel_args = vec![
            "console=ttyS0".to_string(),
            "root=/dev/vda1".to_string(),
            "ro".to_string(),
        ];
        kernel_args.extend(spec.kernel_args.clone());
        cmd.arg("-append").arg(kernel_args.join(" "));

        info!("Starting QEMU for compartment: {}", spec.name);
        let mut child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0);

        Ok(CompartmentHandle::Qemu { child, pid })
    }

    async fn start(&self, _handle: &mut CompartmentHandle) -> Result<()> {
        Ok(())
    }

    async fn stop(&self, handle: &CompartmentHandle, force: bool) -> Result<()> {
        match handle {
            CompartmentHandle::Qemu { child, .. } => {
                if force { child.kill().await?; } else { child.terminate().await?; }
                child.wait().await?;
            }
            _ => return Err(anyhow::anyhow!("Invalid handle type")),
        }
        Ok(())
    }

    async fn get_status(&self, handle: &CompartmentHandle) -> Result<CompartmentState> {
        match handle {
            CompartmentHandle::Qemu { child, .. } => {
                match child.try_wait()? {
                    Some(s) => Ok(if s.success() { CompartmentState::Stopped } else { CompartmentState::Failed }),
                    None => Ok(CompartmentState::Running),
                }
            }
            _ => Err(anyhow::anyhow!("Invalid handle type")),
        }
    }

    async fn get_logs(&self, _handle: &CompartmentHandle, _lines: usize) -> Result<Vec<String>> {
        Ok(vec!["[qemu] Logs from serial console".to_string()])
    }

    async fn exec(&self, _handle: &CompartmentHandle, cmd: &[String]) -> Result<String> {
        Ok(format!("[qemu] Would execute via guest agent: {:?}", cmd))
    }
}

#[async_trait::async_trait]
impl Backend for KataBackend {
    async fn create(&self, spec: &CompartmentSpec, config: &ManagerConfig, storage: &Path) -> Result<CompartmentHandle> {
        // Kata uses containerd + kata-runtime
        let container_id = Uuid::new_v4().to_string();

        // Build container spec
        let mut args = vec![
            "run".to_string(),
            "--runtime=kata".to_string(),
            "-d".to_string(),
            "--name".to_string(),
            spec.name.clone(),
        ];

        // Resources
        if let Some(cpu) = spec.resources.cpu_cores {
            args.extend(["--cpus".to_string(), cpu.to_string()]);
        }
        if let Some(mem) = spec.resources.memory_bytes {
            args.extend(["--memory".to_string(), format!("{}b", mem)]);
        }
        if let Some(gpu) = spec.resources.gpu_devices {
            if gpu > 0 {
                args.extend(["--gpus".to_string(), "all".to_string()]);
            }
        }

        // Image
        args.push("sentinel/base:latest".to_string());

        info!("Starting Kata container for compartment: {}", spec.name);
        let output = Command::new("ctr")
            .args(&args)
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to create Kata container: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        Ok(CompartmentHandle::Kata { container_id })
    }

    async fn start(&self, _handle: &mut CompartmentHandle) -> Result<()> {
        Ok(())
    }

    async fn stop(&self, handle: &CompartmentHandle, force: bool) -> Result<()> {
        match handle {
            CompartmentHandle::Kata { container_id } => {
                let mut cmd = Command::new("ctr");
                cmd.args(["task", "kill", "--signal", if force { "9" } else { "15" }, container_id]);
                cmd.output().await?;
                Command::new("ctr")
                    .args(["container", "rm", container_id])
                    .output().await?;
            }
            _ => return Err(anyhow::anyhow!("Invalid handle type")),
        }
        Ok(())
    }

    async fn get_status(&self, handle: &CompartmentHandle) -> Result<CompartmentState> {
        match handle {
            CompartmentHandle::Kata { container_id } => {
                let output = Command::new("ctr")
                    .args(["task", "ls", container_id])
                    .output().await?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.contains("RUNNING") {
                    Ok(CompartmentState::Running)
                } else if stdout.contains("STOPPED") {
                    Ok(CompartmentState::Stopped)
                } else {
                    Ok(CompartmentState::Failed)
                }
            }
            _ => Err(anyhow::anyhow!("Invalid handle type")),
        }
    }

    async fn get_logs(&self, handle: &CompartmentHandle, lines: usize) -> Result<Vec<String>> {
        match handle {
            CompartmentHandle::Kata { container_id } => {
                let output = Command::new("ctr")
                    .args(["task", "logs", "--tail", &lines.to_string(), container_id])
                    .output().await?;
                Ok(String::from_utf8_lossy(&output.stdout).lines().map(|s| s.to_string()).collect())
            }
            _ => Err(anyhow::anyhow!("Invalid handle type")),
        }
    }

    async fn exec(&self, handle: &CompartmentHandle, cmd: &[String]) -> Result<String> {
        match handle {
            CompartmentHandle::Kata { container_id } => {
                let output = Command::new("ctr")
                    .args(["task", "exec", "--exec-id", &Uuid::new_v4().to_string(), container_id])
                    .args(cmd)
                    .output().await?;
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            }
            _ => Err(anyhow::anyhow!("Invalid handle type")),
        }
    }
}

fn get_backend(backend: &CompartmentBackend) -> Box<dyn Backend> {
    match backend {
        CompartmentBackend::Crosvm => Box::new(CrosvmBackend),
        CompartmentBackend::Qemu => Box::new(QemuBackend),
        CompartmentBackend::Kata => Box::new(KataBackend),
    }
}

// ============================================================================
// Capability Broker Client
// ============================================================================

async fn validate_with_broker(
    broker_url: &str,
    compartment_type: &str,
    capabilities: &[String],
) -> Result<Vec<String>> {
    let client = reqwest::Client::new();
    let req = serde_json::json!({
        "compartment_type": compartment_type,
        "requested_capabilities": capabilities.iter().map(|c| c.parse::<crate::Capability>()).collect::<Result<Vec<_>, _>>()?,
    });

    let resp = client.post(&format!("{}/validate", broker_url))
        .json(&req)
        .send()
        .await?;

    if resp.status().is_success() {
        let val: serde_json::Value = resp.json().await?;
        Ok(val["granted_capabilities"].as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|v| v.to_string())
            .collect())
    } else {
        Err(anyhow::anyhow!("Broker validation failed: {}", resp.status()))
    }
}

// ============================================================================
// Request Handlers
// ============================================================================

#[instrument(skip(state))]
async fn handle_create(
    State(state): State<ManagerState>,
    Json(req): Json<CreateRequest>,
) -> Result<Json<CreateResponse>, StatusCode> {
    // Validate with capability broker if token provided
    let granted_caps = if let Some(token) = req.capability_token {
        // In production, would verify token with broker
        req.spec.capabilities
    } else {
        validate_with_broker(
            &state.capability_broker_url,
            &req.spec.compartment_type,
            &req.spec.capabilities,
        ).await.map_err(|e| {
            error!("Capability validation failed: {}", e);
            StatusCode::FORBIDDEN
        })?
    };

    // Check for name conflicts
    let mut compartments = state.compartments.write().await;
    if compartments.contains_key(&req.spec.name) {
        return Ok(Json(CreateResponse {
            success: false,
            compartment: None,
            error: Some(format!("Compartment '{}' already exists", req.spec.name)),
        }));
    }

    // Create compartment instance
    let backend = get_backend(&req.spec.backend);
    let storage = &state.config.storage_pool;

    let status = CompartmentStatus {
        name: req.spec.name.clone(),
        compartment_type: req.spec.compartment_type.clone(),
        backend: req.spec.backend.clone(),
        state: CompartmentState::Creating,
        pid: None,
        ip_address: None,
        created_at: chrono::Utc::now(),
        started_at: None,
        resources: req.spec.resources.clone(),
        capabilities: granted_caps.clone(),
    };

    let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
    let log_buffer = Arc::new(RwLock::new(Vec::new()));

    // Spawn compartment in background
    let spec = req.spec.clone();
    let config = state.config.clone();
    let storage = storage.to_path_buf();
    let log_buffer_clone = log_buffer.clone();
    let compartments_ref = state.compartments.clone();
    let name = req.spec.name.clone();

    tokio::spawn(async move {
        // Update state to creating
        {
            let mut comps = compartments_ref.write().await;
            if let Some(comp) = comps.get_mut(&name) {
                comp.status.state = CompartmentState::Creating;
            }
        }

        // Create compartment
        let handle = match backend.create(&spec, &config, &storage).await {
            Ok(h) => h,
            Err(e) => {
                error!("Failed to create compartment {}: {}", name, e);
                let mut comps = compartments_ref.write().await;
                if let Some(comp) = comps.get_mut(&name) {
                    comp.status.state = CompartmentState::Failed;
                    comp.log_buffer.write().await.push(format!("ERROR: {}", e));
                }
                return;
            }
        };

        // Update state
        {
            let mut comps = compartments_ref.write().await;
            if let Some(comp) = comps.get_mut(&name) {
                comp.handle = Some(handle);
                comp.status.state = CompartmentState::Starting;
            }
        }

        // Start compartment
        {
            let mut comps = compartments_ref.write().await;
            if let Some(comp) = comps.get_mut(&name) {
                if let Some(h) = &mut comp.handle {
                    if let Err(e) = backend.start(h).await {
                        error!("Failed to start compartment {}: {}", name, e);
                        comp.status.state = CompartmentState::Failed;
                        comp.log_buffer.write().await.push(format!("ERROR: {}", e));
                        return;
                    }
                    comp.status.state = CompartmentState::Running;
                    comp.status.started_at = Some(chrono::Utc::now());

                    // Extract PID if available
                    if let CompartmentHandle::Crosvm { child, .. } | CompartmentHandle::Qemu { child, .. } = h {
                        comp.status.pid = child.id();
                    }
                }
            }
        }

        // Wait for shutdown signal or process exit
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("Shutdown requested for compartment {}", name);
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(10)) => {
                    // Check process status
                    let mut comps = compartments_ref.write().await;
                    if let Some(comp) = comps.get_mut(&name) {
                        if let Some(h) = &mut comp.handle {
                            match backend.get_status(h).await {
                                Ok(CompartmentState::Running) => {}
                                Ok(state) => {
                                    comp.status.state = state;
                                    break;
                                }
                                Err(e) => {
                                    error!("Status check failed for {}: {}", name, e);
                                    comp.status.state = CompartmentState::Failed;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Stop compartment
        let mut comps = compartments_ref.write().await;
        if let Some(comp) = comps.get_mut(&name) {
            if let Some(h) = comp.handle.take() {
                let _ = backend.stop(&h, true).await;
            }
            comp.status.state = CompartmentState::Stopped;
        }
    });

    // Store instance
    let instance = CompartmentInstance {
        status,
        handle: None, // Will be set by background task
        log_buffer,
        shutdown_tx: Some(shutdown_tx),
    };

    compartments.insert(req.spec.name.clone(), instance);

    // Return initial status
    let comps = state.compartments.read().await;
    Ok(Json(CreateResponse {
        success: true,
        compartment: comps.get(&req.spec.name).map(|c| c.status.clone()),
        error: None,
    }))
}

#[instrument(skip(state))]
async fn handle_start(
    State(state): State<ManagerState>,
    AxumPath(name): AxumPath<String>,
) -> Result<Json<ActionResponse>, StatusCode> {
    let mut compartments = state.compartments.write().await;
    let comp = compartments.get_mut(&name)
        .ok_or(StatusCode::NOT_FOUND)?;

    if comp.status.state == CompartmentState::Running {
        return Ok(Json(ActionResponse {
            success: false,
            message: "Already running".to_string(),
        }));
    }

    if let Some(h) = &mut comp.handle {
        let backend = get_backend(&comp.status.backend);
        backend.start(h).await.map_err(|e| {
            error!("Failed to start {}: {}", name, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        comp.status.state = CompartmentState::Running;
        comp.status.started_at = Some(chrono::Utc::now());
    }

    Ok(Json(ActionResponse {
        success: true,
        message: "Started".to_string(),
    }))
}

#[instrument(skip(state))]
async fn handle_stop(
    State(state): State<ManagerState>,
    AxumPath(name): AxumPath<String>,
) -> Result<Json<ActionResponse>, StatusCode> {
    let mut compartments = state.compartments.write().await;
    let comp = compartments.get_mut(&name)
        .ok_or(StatusCode::NOT_FOUND)?;

    if comp.status.state == CompartmentState::Stopped {
        return Ok(Json(ActionResponse {
            success: false,
            message: "Already stopped".to_string(),
        }));
    }

    // Send shutdown signal
    if let Some(tx) = comp.shutdown_tx.take() {
        let _ = tx.send(()).await;
    }

    Ok(Json(ActionResponse {
        success: true,
        message: "Stop requested".to_string(),
    }))
}

#[instrument(skip(state))]
async fn handle_destroy(
    State(state): State<ManagerState>,
    AxumPath(name): AxumPath<String>,
) -> Result<Json<ActionResponse>, StatusCode> {
    let mut compartments = state.compartments.write().await;
    let comp = compartments.get_mut(&name)
        .ok_or(StatusCode::NOT_FOUND)?;

    // Force stop if running
    if comp.status.state == CompartmentState::Running {
        if let Some(h) = &mut comp.handle {
            let backend = get_backend(&comp.status.backend);
            let _ = backend.stop(h, true).await;
        }
    }

    comp.status.state = CompartmentState::Destroying;
    compartments.remove(&name);

    Ok(Json(ActionResponse {
        success: true,
        message: "Destroyed".to_string(),
    }))
}

async fn handle_list(State(state): State<ManagerState>) -> Json<ListResponse> {
    let compartments = state.compartments.read().await;
    Json(ListResponse {
        compartments: compartments.values().map(|c| c.status.clone()).collect(),
    })
}

async fn handle_get(State(state): State<ManagerState>, AxumPath(name): AxumPath<String>) -> Result<Json<CompartmentStatus>, StatusCode> {
    let compartments = state.compartments.read().await;
    compartments.get(&name)
        .map(|c| Json(c.status.clone()))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn handle_logs(
    State(state): State<ManagerState>,
    AxumPath(name): AxumPath<String>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<Json<LogsResponse>, StatusCode> {
    let compartments = state.compartments.read().await;
    let comp = compartments.get(&name).ok_or(StatusCode::NOT_FOUND)?;

    let lines = params.get("lines").and_then(|s| s.parse().ok()).unwrap_or(100);
    let backend = get_backend(&comp.status.backend);

    let logs = if let Some(h) = &comp.handle {
        backend.get_logs(h, lines).await.unwrap_or_default()
    } else {
        comp.log_buffer.read().await.iter().rev().take(lines).cloned().collect()
    };

    Ok(Json(LogsResponse {
        logs,
        truncated: false,
    }))
}

async fn handle_exec(
    State(state): State<ManagerState>,
    AxumPath(name): AxumPath<String>,
    Json(cmd): Json<Vec<String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let compartments = state.compartments.read().await;
    let comp = compartments.get(&name).ok_or(StatusCode::NOT_FOUND)?;

    if comp.status.state != CompartmentState::Running {
        return Err(StatusCode::CONFLICT);
    }

    let backend = get_backend(&comp.status.backend);
    let output = if let Some(h) = &comp.handle {
        backend.exec(h, &cmd).await?
    } else {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    };

    Ok(Json(serde_json::json!({ "output": output })))
}

async fn handle_health(State(state): State<ManagerState>) -> Json<serde_json::Value> {
    let uptime = SystemTime::now().duration_since(state.start_time).unwrap_or_default();
    let compartments = state.compartments.read().await;

    Json(serde_json::json!({
        "status": "healthy",
        "uptime_seconds": uptime.as_secs(),
        "compartments_total": compartments.len(),
        "compartments_running": compartments.values().filter(|c| c.status.state == CompartmentState::Running).count(),
    }))
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

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(true)
        .init();

    info!("Starting Sentinel Compartment Manager v{}", env!("CARGO_PKG_VERSION"));

    // Load config
    let config_content = tokio::fs::read_to_string(&args.config).await
        .unwrap_or_else(|_| {
            warn!("Config file not found, using defaults");
            String::new()
        });

    let config: ManagerConfig = if config_content.is_empty() {
        ManagerConfig {
            default_backend: CompartmentBackend::Crosvm,
            kernel_path: PathBuf::from("/boot/vmlinuz-linux-hardened"),
            initrd_path: Some(PathBuf::from("/boot/initramfs-linux-hardened.img")),
            base_disk_image: PathBuf::from("/var/lib/sentinel/compartments/base.qcow2"),
            network_bridge_prefix: "sentinel-".to_string(),
            gpu_passthrough: true,
        }
    } else {
        toml::from_str(&config_content)?
    };

    // Ensure storage pool exists
    tokio::fs::create_dir_all(&args.storage_pool).await?;
    tokio::fs::create_dir_all("/run/sentinel/compartments").await?;

    let state = ManagerState {
        compartments: Arc::new(RwLock::new(HashMap::new())),
        config,
        capability_broker_url: args.capability_broker,
        start_time: SystemTime::now(),
    };

    // Build router
    let app = Router::new()
        .route("/health", get(handle_health))
        .route("/compartments", post(handle_create))
        .route("/compartments", get(handle_list))
        .route("/compartments/:name", get(handle_get))
        .route("/compartments/:name/start", post(handle_start))
        .route("/compartments/:name/stop", post(handle_stop))
        .route("/compartments/:name", delete(handle_destroy))
        .route("/compartments/:name/logs", get(handle_logs))
        .route("/compartments/:name/exec", post(handle_exec))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&args.http_addr).await?;
    info!("Compartment Manager API listening on {}", args.http_addr);

    axum::serve(listener, app).await?;

    Ok(())
}