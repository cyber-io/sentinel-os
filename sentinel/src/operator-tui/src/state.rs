//! Application state management

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::{DateTime, Utc};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::api::{ApiClient, CompartmentInfo, TaskInfo, LogEntry, MetricData, ThreatInfo};
use crate::AppConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Compartments,
    Tasks,
    Logs,
    Metrics,
    Threats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Normal,
    Command,
    Search,
    Help,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompartmentSummary {
    pub name: String,
    pub compartment_type: String,
    pub status: String,
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub uptime: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub id: String,
    pub description: String,
    pub status: String,
    pub progress: f32,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatSummary {
    pub id: String,
    pub severity: String,
    pub title: String,
    pub source: String,
    pub detected_at: DateTime<Utc>,
    pub status: String,
}

pub struct AppState {
    pub config: AppConfig,
    pub api_client: ApiClient,
    pub active_panel: Panel,
    pub view_mode: ViewMode,
    pub show_help: bool,

    // Data
    pub compartments: Vec<CompartmentSummary>,
    pub tasks: Vec<TaskSummary>,
    pub logs: VecDeque<LogEntry>,
    pub metrics: HashMap<String, MetricData>,
    pub threats: Vec<ThreatSummary>,

    // UI state
    pub selected_compartment: Option<usize>,
    pub selected_task: Option<usize>,
    pub log_filter: String,
    pub search_query: String,
    pub command_buffer: String,
    pub status_message: Option<String>,
    pub status_time: Option<Instant>,

    // Refresh
    pub last_refresh: Instant,
    pub auto_refresh_interval: Duration,

    // Scroll positions
    pub compartments_scroll: usize,
    pub tasks_scroll: usize,
    pub logs_scroll: usize,
    pub threats_scroll: usize,
}

impl AppState {
    pub async fn new(config: AppConfig, api_url: String, ws_url: String) -> Result<Self> {
        let api_client = ApiClient::new(api_url, ws_url).await?;

        Ok(Self {
            config,
            api_client,
            active_panel: Panel::Compartments,
            view_mode: ViewMode::Normal,
            show_help: false,
            compartments: Vec::new(),
            tasks: Vec::new(),
            logs: VecDeque::with_capacity(10000),
            metrics: HashMap::new(),
            threats: Vec::new(),
            selected_compartment: None,
            selected_task: None,
            log_filter: String::new(),
            search_query: String::new(),
            command_buffer: String::new(),
            status_message: None,
            status_time: None,
            last_refresh: Instant::now(),
            auto_refresh_interval: Duration::from_secs(5),
            compartments_scroll: 0,
            tasks_scroll: 0,
            logs_scroll: 0,
            threats_scroll: 0,
        })
    }

    pub fn next_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Compartments => Panel::Tasks,
            Panel::Tasks => Panel::Logs,
            Panel::Logs => Panel::Metrics,
            Panel::Metrics => Panel::Threats,
            Panel::Threats => Panel::Compartments,
        };
        self.clear_selection();
    }

    pub fn prev_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Compartments => Panel::Threats,
            Panel::Tasks => Panel::Compartments,
            Panel::Logs => Panel::Tasks,
            Panel::Metrics => Panel::Logs,
            Panel::Threats => Panel::Metrics,
        };
        self.clear_selection();
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        if self.show_help {
            self.view_mode = ViewMode::Help;
        } else {
            self.view_mode = ViewMode::Normal;
        }
    }

    pub fn enter_command_mode(&mut self) {
        self.view_mode = ViewMode::Command;
        self.command_buffer.clear();
    }

    pub fn enter_search_mode(&mut self) {
        self.view_mode = ViewMode::Search;
        self.search_query.clear();
    }

    pub fn exit_mode(&mut self) {
        self.view_mode = ViewMode::Normal;
        self.command_buffer.clear();
        self.search_query.clear();
    }

    pub fn clear_selection(&mut self) {
        self.selected_compartment = None;
        self.selected_task = None;
    }

    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some(msg);
        self.status_time = Some(Instant::now());
    }

    pub fn should_auto_refresh(&self) -> bool {
        self.last_refresh.elapsed() >= self.auto_refresh_interval
    }

    pub async fn refresh_data(&mut self) -> Result<()> {
        self.last_refresh = Instant::now();

        // Fetch compartments
        if let Ok(compartments) = self.api_client.get_compartments().await {
            self.compartments = compartments.into_iter().map(|c| CompartmentSummary {
                name: c.name,
                compartment_type: c.compartment_type,
                status: format!("{:?}", c.state),
                cpu_usage: c.cpu_usage.unwrap_or(0.0),
                memory_usage: c.memory_usage.unwrap_or(0.0),
                uptime: c.started_at.map(|t| format_duration(t)).unwrap_or("N/A".to_string()),
            }).collect();
        }

        // Fetch tasks
        if let Ok(tasks) = self.api_client.get_tasks().await {
            self.tasks = tasks.into_iter().map(|t| TaskSummary {
                id: t.id,
                description: t.description,
                status: format!("{:?}", t.status),
                progress: t.progress,
                started_at: t.created_at,
            }).collect();
        }

        // Fetch threats
        if let Ok(threats) = self.api_client.get_threats().await {
            self.threats = threats.into_iter().map(|t| ThreatSummary {
                id: t.id,
                severity: t.severity,
                title: t.title,
                source: t.source,
                detected_at: t.detected_at,
                status: t.status,
            }).collect();
        }

        // Fetch metrics
        if let Ok(metrics) = self.api_client.get_metrics().await {
            self.metrics = metrics;
        }

        // Add log entry
        self.logs.push_back(LogEntry {
            timestamp: Utc::now(),
            level: "INFO".to_string(),
            source: "tui".to_string(),
            message: "Data refreshed".to_string(),
        });

        Ok(())
    }

    pub fn add_log(&mut self, entry: LogEntry) {
        if self.logs.len() >= 10000 {
            self.logs.pop_front();
        }
        self.logs.push_back(entry);
    }
}

fn format_duration(dt: DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(dt);

    let secs = diff.num_seconds();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

pub fn parse_color(s: &str) -> Color {
    if s.starts_with('#') && s.len() == 7 {
        if let Ok(r) = u8::from_str_radix(&s[1..3], 16) {
            if let Ok(g) = u8::from_str_radix(&s[3..5], 16) {
                if let Ok(b) = u8::from_str_radix(&s[5..7], 16) {
                    return Color::Rgb(r, g, b);
                }
            }
        }
    }
    Color::White
}