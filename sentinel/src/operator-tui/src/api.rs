//! API client for communicating with Sentinel OS services

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{debug, error, info, warn};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompartmentInfo {
    pub name: String,
    pub compartment_type: String,
    pub backend: String,
    pub state: CompartmentState,
    pub pid: Option<u32>,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub cpu_usage: Option<f32>,
    pub memory_usage: Option<f32>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    pub cpu_cores: Option<u32>,
    pub memory_bytes: Option<u64>,
    pub disk_bytes: Option<u64>,
    pub gpu_devices: Option<u32>,
    pub pids: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub description: String,
    pub context: serde_json::Value,
    pub priority: String,
    pub required_capabilities: Vec<String>,
    pub target_compartment: Option<String>,
    pub status: TaskStatus,
    pub progress: f32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Planning,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricData {
    pub name: String,
    pub value: f64,
    pub labels: HashMap<String, String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatInfo {
    pub id: String,
    pub severity: String,
    pub title: String,
    pub description: String,
    pub source: String,
    pub detected_at: DateTime<Utc>,
    pub status: String,
    pub indicators: Vec<String>,
}

pub struct ApiClient {
    http_client: reqwest::Client,
    base_url: String,
    ws_url: String,
    ws_sender: Arc<RwLock<Option<mpsc::UnboundedSender<Message>>>>,
    event_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<ApiEvent>>>>,
}

#[derive(Debug, Clone)]
pub enum ApiEvent {
    CompartmentUpdate(CompartmentInfo),
    TaskUpdate(TaskInfo),
    LogEntry(LogEntry),
    MetricUpdate(MetricData),
    ThreatUpdate(ThreatInfo),
    HealthCheck(bool),
}

impl ApiClient {
    pub async fn new(base_url: String, ws_url: String) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        let (ws_tx, ws_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let client = Self {
            http_client,
            base_url,
            ws_url,
            ws_sender: Arc::new(RwLock::new(Some(ws_tx))),
            event_receiver: Arc::new(RwLock::new(Some(event_rx))),
        };

        // Start WebSocket connection
        client.start_ws_connection(event_tx).await?;

        Ok(client)
    }

    async fn start_ws_connection(&self, event_tx: mpsc::UnboundedSender<ApiEvent>) -> Result<()> {
        let ws_url = self.ws_url.clone();
        let ws_sender = self.ws_sender.clone();

        tokio::spawn(async move {
            loop {
                if let Err(e) = Self::ws_loop(ws_url.clone(), ws_sender.clone(), event_tx.clone()).await {
                    warn!("WebSocket error: {}, reconnecting in 5s", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        });

        Ok(())
    }

    async fn ws_loop(
        ws_url: String,
        ws_sender: Arc<RwLock<Option<mpsc::UnboundedSender<Message>>>>,
        event_tx: mpsc::UnboundedSender<ApiEvent>,
    ) -> Result<()> {
        let (ws_stream, _) = connect_async(Url::parse(&ws_url)?).await?;
        info!("WebSocket connected to {}", ws_url);

        let (mut write, mut read) = ws_stream.split();

        // Store sender
        {
            let mut sender = ws_sender.write().await;
            let (tx, mut rx) = mpsc::unbounded_channel();
            *sender = Some(tx);

            // Forward messages to WebSocket
            tokio::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    if write.send(msg).await.is_err() {
                        break;
                    }
                }
            });
        }

        // Read messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(event) = serde_json::from_str::<ApiEvent>(&text) {
                        let _ = event_tx.send(event);
                    }
                }
                Ok(Message::Ping(data)) => {
                    let _ = write.send(Message::Pong(data)).await;
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket closed");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub async fn get_compartments(&self) -> Result<Vec<CompartmentInfo>> {
        let resp = self.http_client
            .get(&format!("{}/compartments", self.base_url))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get compartments: {}", resp.status()));
        }

        let data: serde_json::Value = resp.json().await?;
        let compartments = data["compartments"].as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();

        Ok(compartments)
    }

    pub async fn get_tasks(&self) -> Result<Vec<TaskInfo>> {
        let resp = self.http_client
            .get(&format!("{}/tasks", self.base_url))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get tasks: {}", resp.status()));
        }

        let data: Vec<TaskInfo> = resp.json().await?;
        Ok(data)
    }

    pub async fn get_threats(&self) -> Result<Vec<ThreatInfo>> {
        let resp = self.http_client
            .get(&format!("{}/threats", self.base_url))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get threats: {}", resp.status()));
        }

        let data: Vec<ThreatInfo> = resp.json().await?;
        Ok(data)
    }

    pub async fn get_metrics(&self) -> Result<HashMap<String, MetricData>> {
        let resp = self.http_client
            .get(&format!("{}/metrics", self.base_url))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get metrics: {}", resp.status()));
        }

        let data: Vec<MetricData> = resp.json().await?;
        let mut map = HashMap::new();
        for m in data {
            map.insert(m.name.clone(), m);
        }
        Ok(map)
    }

    pub async fn create_compartment(&self, spec: serde_json::Value) -> Result<CompartmentInfo> {
        let resp = self.http_client
            .post(&format!("{}/compartments", self.base_url))
            .json(&spec)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to create compartment: {}", resp.status()));
        }

        let data: CompartmentInfo = resp.json().await?;
        Ok(data)
    }

    pub async fn start_compartment(&self, name: &str) -> Result<()> {
        let resp = self.http_client
            .post(&format!("{}/compartments/{}/start", self.base_url, name))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to start compartment: {}", resp.status()));
        }
        Ok(())
    }

    pub async fn stop_compartment(&self, name: &str) -> Result<()> {
        let resp = self.http_client
            .post(&format!("{}/compartments/{}/stop", self.base_url, name))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to stop compartment: {}", resp.status()));
        }
        Ok(())
    }

    pub async fn destroy_compartment(&self, name: &str) -> Result<()> {
        let resp = self.http_client
            .delete(&format!("{}/compartments/{}", self.base_url, name))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to destroy compartment: {}", resp.status()));
        }
        Ok(())
    }

    pub async fn execute_command(&self, compartment: &str, cmd: Vec<String>) -> Result<serde_json::Value> {
        let resp = self.http_client
            .post(&format!("{}/compartments/{}/exec", self.base_url, compartment))
            .json(&cmd)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to execute command: {}", resp.status()));
        }

        let data: serde_json::Value = resp.json().await?;
        Ok(data)
    }

    pub async fn execute_task(&self, task: serde_json::Value) -> Result<TaskInfo> {
        let resp = self.http_client
            .post(&format!("{}/execute", self.base_url))
            .json(&task)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to execute task: {}", resp.status()));
        }

        let data: TaskInfo = resp.json().await?;
        Ok(data)
    }

    pub async fn get_task_status(&self, task_id: &str) -> Result<TaskInfo> {
        let resp = self.http_client
            .get(&format!("{}/tasks/{}", self.base_url, task_id))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get task status: {}", resp.status()));
        }

        let data: TaskInfo = resp.json().await?;
        Ok(data)
    }

    pub async fn cancel_task(&self, task_id: &str) -> Result<()> {
        let resp = self.http_client
            .post(&format!("{}/tasks/{}/cancel", self.base_url, task_id))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to cancel task: {}", resp.status()));
        }
        Ok(())
    }

    pub fn try_recv_event(&self) -> Option<ApiEvent> {
        // This would need a different approach - using try_recv on the receiver
        None
    }
}