//! Sentinel OS AI Orchestrator
//!
//! Plans and executes security tasks with LLM-based planning and critic validation.

mod planner;
mod critics;
mod task_queue;
mod models;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use axum::{
    extract::{State, Path as AxumPath, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use clap::Parser;
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use parking_lot::RwLock;
use tokio::sync::{mpsc, Mutex, broadcast};
use tokio::time::interval;
use tracing::{debug, error, info, warn, instrument};
use uuid::Uuid;

use planner::{Planner, Plan, PlanStep, StepStatus};
use critics::{CriticManager, CriticResult, CriticType};
use task_queue::{TaskQueue, Task, TaskStatus, TaskPriority};
use models::{ModelRegistry, ModelBackend, InferenceRequest, InferenceResponse};

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    pub planner_model: String,
    pub critic_models: Vec<String>,
    pub inference_endpoint: String,
    pub max_concurrent_tasks: usize,
    pub task_timeout_secs: u64,
    pub max_retries: u32,
    pub critic_threshold: f32,
    pub enable_streaming: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteRequest {
    pub task: String,
    pub context: Option<serde_json::Value>,
    pub priority: Option<TaskPriority>,
    pub required_capabilities: Option<Vec<String>>,
    pub target_compartment: Option<String>,
    pub stream: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResponse {
    pub task_id: String,
    pub status: TaskStatus,
    pub plan: Option<Plan>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusResponse {
    pub task_id: String,
    pub status: TaskStatus,
    pub plan: Option<Plan>,
    pub current_step: Option<usize>,
    pub progress: f32,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriticValidationRequest {
    pub plan: Plan,
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriticValidationResponse {
    pub passed: bool,
    pub results: Vec<CriticResult>,
    pub overall_score: f32,
}

#[derive(Clone)]
struct OrchestratorState {
    config: OrchestratorConfig,
    planner: Arc<Planner>,
    critics: Arc<CriticManager>,
    task_queue: Arc<TaskQueue>,
    model_registry: Arc<ModelRegistry>,
    active_tasks: Arc<DashMap<String, Arc<Mutex<TaskExecution>>>>,
    event_tx: broadcast::Sender<OrchestratorEvent>,
    start_time: SystemTime,
}

struct TaskExecution {
    task: Task,
    plan: Option<Plan>,
    current_step: usize,
    status: TaskStatus,
    result: Option<serde_json::Value>,
    error: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OrchestratorEvent {
    TaskCreated { task_id: String, task: String },
    TaskStarted { task_id: String },
    StepStarted { task_id: String, step_index: usize, step: PlanStep },
    StepCompleted { task_id: String, step_index: usize, result: serde_json::Value },
    StepFailed { task_id: String, step_index: usize, error: String },
    TaskCompleted { task_id: String, result: serde_json::Value },
    TaskFailed { task_id: String, error: String },
    CriticValidation { task_id: String, passed: bool, score: f32 },
}

#[derive(Parser, Debug)]
#[command(name = "sentinel-ai-orchestrator", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/etc/sentinel/ai-orchestrator.toml")]
    config: std::path::PathBuf,

    /// HTTP API address
    #[arg(long, default_value = "127.0.0.1:8083")]
    http_addr: String,

    /// Inference gateway URL
    #[arg(long, default_value = "http://127.0.0.1:11434")]
    inference_gateway: String,

    /// Compartment manager URL
    #[arg(long, default_value = "http://127.0.0.1:8082")]
    compartment_manager: String,

    /// Capability broker URL
    #[arg(long, default_value = "http://127.0.0.1:8081")]
    capability_broker: String,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

// ============================================================================
// Request Handlers
// ============================================================================

#[instrument(skip(state))]
async fn handle_execute(
    State(state): State<OrchestratorState>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, StatusCode> {
    let task_id = Uuid::new_v4().to_string();
    let priority = req.priority.unwrap_or(TaskPriority::Normal);

    // Create task
    let task = Task {
        id: task_id.clone(),
        description: req.task,
        context: req.context.unwrap_or(serde_json::json!({})),
        priority,
        required_capabilities: req.required_capabilities.unwrap_or_default(),
        target_compartment: req.target_compartment,
        status: TaskStatus::Pending,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        retry_count: 0,
    };

    // Enqueue task
    state.task_queue.enqueue(task.clone()).await.map_err(|e| {
        error!("Failed to enqueue task: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Create execution tracking
    let execution = Arc::new(Mutex::new(TaskExecution {
        task: task.clone(),
        plan: None,
        current_step: 0,
        status: TaskStatus::Pending,
        result: None,
        error: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }));

    state.active_tasks.insert(task_id.clone(), execution.clone());

    // Emit event
    let _ = state.event_tx.send(OrchestratorEvent::TaskCreated {
        task_id: task_id.clone(),
        task: task.description.clone(),
    });

    // Start execution in background
    let state_clone = state.clone();
    let execution_clone = execution.clone();
    let stream = req.stream.unwrap_or(state.config.enable_streaming);

    tokio::spawn(async move {
        if let Err(e) = execute_task(state_clone, execution_clone, stream).await {
            error!("Task execution failed: {}", e);
        }
    });

    Ok(Json(ExecuteResponse {
        task_id,
        status: TaskStatus::Pending,
        plan: None,
        result: None,
        error: None,
    }))
}

async fn execute_task(
    state: OrchestratorState,
    execution: Arc<Mutex<TaskExecution>>,
    stream: bool,
) -> Result<()> {
    let task_id = {
        let exec = execution.lock().await;
        exec.task.id.clone()
    };

    // Update status to planning
    {
        let mut exec = execution.lock().await;
        exec.status = TaskStatus::Planning;
        exec.updated_at = chrono::Utc::now();
    }

    let _ = state.event_tx.send(OrchestratorEvent::TaskStarted {
        task_id: task_id.clone(),
    });

    // Generate plan
    let plan = {
        let exec = execution.lock().await;
        state.planner.generate_plan(&exec.task.description, &exec.task.context).await?
    };

    // Validate with critics
    let critic_result = state.critics.validate_plan(&plan, &execution.lock().await.task.context).await?;

    let _ = state.event_tx.send(OrchestratorEvent::CriticValidation {
        task_id: task_id.clone(),
        passed: critic_result.passed,
        score: critic_result.overall_score,
    });

    if !critic_result.passed {
        let mut exec = execution.lock().await;
        exec.status = TaskStatus::Failed;
        exec.error = Some(format!("Critic validation failed: score {:.2} < threshold {:.2}",
            critic_result.overall_score, state.config.critic_threshold));
        exec.updated_at = chrono::Utc::now();

        let _ = state.event_tx.send(OrchestratorEvent::TaskFailed {
            task_id: task_id.clone(),
            error: exec.error.clone().unwrap(),
        });
        return Ok(());
    }

    // Store plan
    {
        let mut exec = execution.lock().await;
        exec.plan = Some(plan.clone());
        exec.status = TaskStatus::Running;
        exec.updated_at = chrono::Utc::now();
    }

    // Execute plan steps
    let steps = plan.steps.clone();
    for (idx, step) in steps.iter().enumerate() {
        // Check if task was cancelled
        {
            let exec = execution.lock().await;
            if exec.status == TaskStatus::Cancelled {
                return Ok(());
            }
        }

        let _ = state.event_tx.send(OrchestratorEvent::StepStarted {
            task_id: task_id.clone(),
            step_index: idx,
            step: step.clone(),
        });

        // Execute step with retries
        let mut step_result = None;
        let mut last_error = None;

        for attempt in 0..=state.config.max_retries {
            match execute_step(&state, &execution, step, idx).await {
                Ok(result) => {
                    step_result = Some(result);
                    last_error = None;
                    break;
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                    warn!("Step {} attempt {} failed: {}", idx, attempt + 1, e);
                    if attempt < state.config.max_retries {
                        tokio::time::sleep(Duration::from_secs(2_u64.pow(attempt))).await;
                    }
                }
            }
        }

        let result = match step_result {
            Some(r) => r,
            None => {
                let error_msg = last_error.unwrap_or_else(|| "Unknown error".to_string());
                let mut exec = execution.lock().await;
                exec.status = TaskStatus::Failed;
                exec.error = Some(format!("Step {} failed after {} retries: {}",
                    idx, state.config.max_retries + 1, error_msg));
                exec.updated_at = chrono::Utc::now();

                let _ = state.event_tx.send(OrchestratorEvent::StepFailed {
                    task_id: task_id.clone(),
                    step_index: idx,
                    error: error_msg,
                });

                let _ = state.event_tx.send(OrchestratorEvent::TaskFailed {
                    task_id: task_id.clone(),
                    error: exec.error.clone().unwrap(),
                });
                return Ok(());
            }
        };

        // Update step status
        {
            let mut exec = execution.lock().await;
            if let Some(plan) = &mut exec.plan {
                plan.steps[idx].status = StepStatus::Completed;
                plan.steps[idx].result = Some(result.clone());
            }
            exec.current_step = idx + 1;
            exec.updated_at = chrono::Utc::now();
        }

        let _ = state.event_tx.send(OrchestratorEvent::StepCompleted {
            task_id: task_id.clone(),
            step_index: idx,
            result: result.clone(),
        });
    }

    // Task completed successfully
    let final_result = {
        let mut exec = execution.lock().await;
        exec.status = TaskStatus::Completed;
        exec.result = Some(serde_json::json!({
            "steps_completed": steps.len(),
            "final_step_result": exec.plan.as_ref().and_then(|p| p.steps.last().and_then(|s| s.result.clone()))
        }));
        exec.updated_at = chrono::Utc::now();
        exec.result.clone()
    };

    let _ = state.event_tx.send(OrchestratorEvent::TaskCompleted {
        task_id: task_id.clone(),
        result: final_result.unwrap_or(serde_json::json!({})),
    });

    Ok(())
}

async fn execute_step(
    state: &OrchestratorState,
    execution: &Arc<Mutex<TaskExecution>>,
    step: &PlanStep,
    step_index: usize,
) -> Result<serde_json::Value> {
    match step.step_type.as_str() {
        "inference" => {
            let req = InferenceRequest {
                model: step.parameters.get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&state.config.planner_model)
                    .to_string(),
                prompt: step.parameters.get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                system_prompt: step.parameters.get("system_prompt")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                temperature: step.parameters.get("temperature")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.7) as f32,
                max_tokens: step.parameters.get("max_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2048) as usize,
                stream: false,
            };

            let resp = state.model_registry.infer(req).await?;
            Ok(serde_json::json!({
                "response": resp.text,
                "tokens": resp.tokens_used,
            }))
        }
        "compartment_action" => {
            // Execute action in compartment
            let compartment = step.parameters.get("compartment")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing compartment parameter"))?;

            let action = step.parameters.get("action")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing action parameter"))?;

            let client = reqwest::Client::new();
            let resp = client.post(&format!("{}/compartments/{}/exec",
                "http://127.0.0.1:8082", compartment))
                .json(&step.parameters.get("command")
                    .and_then(|v| v.as_array())
                    .unwrap_or(&vec![])
                    .iter()
                    .map(|v| v.as_str().unwrap_or("").to_string())
                    .collect::<Vec<_>>())
                .send()
                .await?;

            let result: serde_json::Value = resp.json().await?;
            Ok(result)
        }
        "tool" => {
            // Execute security tool
            let tool = step.parameters.get("tool")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing tool parameter"))?;

            let args = step.parameters.get("args")
                .and_then(|v| v.as_array())
                .unwrap_or(&vec![])
                .iter()
                .map(|v| v.as_str().unwrap_or("").to_string())
                .collect::<Vec<_>>();

            let output = tokio::process::Command::new(tool)
                .args(&args)
                .output()
                .await?;

            Ok(serde_json::json!({
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
                "exit_code": output.status.code(),
            }))
        }
        "wait" => {
            let duration = step.parameters.get("duration")
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as u64;
            tokio::time::sleep(Duration::from_secs(duration)).await;
            Ok(serde_json::json!({ "waited": duration }))
        }
        _ => Err(anyhow::anyhow!("Unknown step type: {}", step.step_type)),
    }
}

#[instrument(skip(state))]
async fn handle_task_status(
    State(state): State<OrchestratorState>,
    AxumPath(task_id): AxumPath<String>,
) -> Result<Json<TaskStatusResponse>, StatusCode> {
    let execution = state.active_tasks.get(&task_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let exec = execution.lock().await;
    let plan = exec.plan.as_ref();
    let progress = if let Some(p) = plan {
        exec.current_step as f32 / p.steps.len().max(1) as f32
    } else {
        0.0
    };

    Ok(Json(TaskStatusResponse {
        task_id: exec.task.id.clone(),
        status: exec.status.clone(),
        plan: exec.plan.clone(),
        current_step: Some(exec.current_step),
        progress,
        result: exec.result.clone(),
        error: exec.error.clone(),
        created_at: exec.created_at,
        updated_at: exec.updated_at,
    }))
}

async fn handle_list_tasks(State(state): State<OrchestratorState>) -> Json<Vec<TaskStatusResponse>> {
    let tasks: Vec<_> = state.active_tasks.iter()
        .map(|entry| {
            let exec = entry.value().blocking_lock();
            TaskStatusResponse {
                task_id: exec.task.id.clone(),
                status: exec.status.clone(),
                plan: exec.plan.clone(),
                current_step: Some(exec.current_step),
                progress: if let Some(p) = &exec.plan {
                    exec.current_step as f32 / p.steps.len().max(1) as f32
                } else { 0.0 },
                result: exec.result.clone(),
                error: exec.error.clone(),
                created_at: exec.created_at,
                updated_at: exec.updated_at,
            }
        })
        .collect();
    Json(tasks)
}

async fn handle_cancel_task(
    State(state): State<OrchestratorState>,
    AxumPath(task_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let execution = state.active_tasks.get(&task_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut exec = execution.lock().await;
    exec.status = TaskStatus::Cancelled;
    exec.updated_at = chrono::Utc::now();

    Ok(Json(serde_json::json!({ "success": true })))
}

async fn handle_validate_plan(
    State(state): State<OrchestratorState>,
    Json(req): Json<CriticValidationRequest>,
) -> Result<Json<CriticValidationResponse>, StatusCode> {
    let result = state.critics.validate_plan(&req.plan, &req.context).await
        .map_err(|e| {
            error!("Critic validation failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(CriticValidationResponse {
        passed: result.passed,
        results: result.results,
        overall_score: result.overall_score,
    }))
}

async fn handle_health(State(state): State<OrchestratorState>) -> Json<serde_json::Value> {
    let uptime = SystemTime::now().duration_since(state.start_time).unwrap_or_default();
    let active_count = state.active_tasks.len();

    Json(serde_json::json!({
        "status": "healthy",
        "uptime_seconds": uptime.as_secs(),
        "active_tasks": active_count,
        "queued_tasks": state.task_queue.len().await,
    }))
}

async fn handle_ws(
    ws: WebSocketUpgrade,
    State(state): State<OrchestratorState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_connection(socket, state))
}

async fn handle_ws_connection(mut socket: axum::extract::ws::WebSocket, state: OrchestratorState) {
    let mut rx = state.event_tx.subscribe();

    while let Ok(event) = rx.recv().await {
        if let Ok(msg) = serde_json::to_string(&event) {
            if socket.send(axum::extract::ws::Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    }
}

// ============================================================================
// Main
// ============================================================================

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

    info!("Starting Sentinel AI Orchestrator v{}", env!("CARGO_PKG_VERSION"));

    // Load config
    let config_content = tokio::fs::read_to_string(&args.config).await
        .unwrap_or_else(|_| String::new());

    let config: OrchestratorConfig = if config_content.is_empty() {
        OrchestratorConfig {
            planner_model: "sentinel-planner".to_string(),
            critic_models: vec!["sentinel-critic-safety".to_string(), "sentinel-critic-efficacy".to_string()],
            inference_endpoint: args.inference_gateway,
            max_concurrent_tasks: 10,
            task_timeout_secs: 3600,
            max_retries: 3,
            critic_threshold: 0.7,
            enable_streaming: true,
        }
    } else {
        toml::from_str(&config_content)?
    };

    // Initialize components
    let model_registry = Arc::new(ModelRegistry::new(&config.inference_endpoint).await?);
    let planner = Arc::new(Planner::new(model_registry.clone(), config.planner_model.clone()).await?);
    let critics = Arc::new(CriticManager::new(model_registry.clone(), config.critic_models.clone()).await?);
    let task_queue = Arc::new(TaskQueue::new(config.max_concurrent_tasks));

    let (event_tx, _) = broadcast::channel(1000);

    let state = OrchestratorState {
        config,
        planner,
        critics,
        task_queue,
        model_registry,
        active_tasks: Arc::new(DashMap::new()),
        event_tx,
        start_time: SystemTime::now(),
    };

    // Start task processor
    let state_clone = state.clone();
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            if let Err(e) = process_queue(&state_clone).await {
                error!("Queue processing error: {}", e);
            }
        }
    });

    // Build router
    let app = Router::new()
        .route("/health", get(handle_health))
        .route("/execute", post(handle_execute))
        .route("/tasks", get(handle_list_tasks))
        .route("/tasks/:id", get(handle_task_status))
        .route("/tasks/:id/cancel", post(handle_cancel_task))
        .route("/validate", post(handle_validate_plan))
        .route("/ws", get(handle_ws))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&args.http_addr).await?;
    info!("AI Orchestrator API listening on {}", args.http_addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn process_queue(state: &OrchestratorState) -> Result<()> {
    // Task queue processing is handled by the background execute_task spawns
    // This could be enhanced to respect max_concurrent_tasks
    Ok(())
}