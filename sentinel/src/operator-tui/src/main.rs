//! Sentinel OS Operator TUI
//!
//! Terminal user interface for security operations management.

mod app;
mod ui;
mod widgets;
mod components;
mod events;
mod state;
mod api;

use std::io;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};
use tokio::sync::mpsc;
use tracing::{info, error};

use app::App;
use events::EventHandler;
use state::AppState;

#[derive(Parser, Debug)]
#[command(name = "sentinel-tui", version, about = "Sentinel OS Operator TUI")]
struct Args {
    /// API server address
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    api: String,

    /// WebSocket address for real-time updates
    #[arg(long, default_value = "ws://127.0.0.1:8080/ws")]
    ws: String,

    /// Config file
    #[arg(short, long)]
    config: Option<std::path::PathBuf>,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

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
        .with_writer(io::stderr)
        .init();

    info!("Starting Sentinel Operator TUI v{}", env!("CARGO_PKG_VERSION"));

    // Load config
    let config = load_config(args.config).await?;

    // Initialize app state
    let app_state = Arc::new(AppState::new(config, args.api, args.ws).await?);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new(app_state.clone());

    // Event handler
    let events = EventHandler::new(250);

    // Main loop
    let result = run_app(&mut terminal, &mut app, &events).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        error!("Application error: {}", e);
        return Err(e);
    }

    Ok(())
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    events: &EventHandler,
) -> Result<()> {
    loop {
        terminal.draw(|f| app.render(f))?;

        match events.next().await? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if app.handle_key(key.code).await? {
                    break;
                }
            }
            Event::Resize(_, _) => {
                app.on_resize();
            }
            _ => {}
        }

        app.update().await?;
    }
    Ok(())
}

async fn load_config(config_path: Option<std::path::PathBuf>) -> Result<AppConfig> {
    if let Some(path) = config_path {
        let content = tokio::fs::read_to_string(path).await?;
        Ok(toml::from_str(&content)?)
    } else {
        // Try default locations
        for path in &[
            dirs::config_dir().map(|d| d.join("sentinel/tui.toml")),
            Some(std::path::PathBuf::from("/etc/sentinel/tui.toml")),
        ] {
            if let Some(p) = path {
                if p.exists() {
                    let content = tokio::fs::read_to_string(p).await?;
                    return Ok(toml::from_str(&content)?);
                }
            }
        }
        Ok(AppConfig::default())
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AppConfig {
    pub theme: Theme,
    pub keybindings: KeyBindings,
    pub panels: PanelConfig,
    pub auto_refresh_secs: u64,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Theme {
    pub name: String,
    pub bg: String,
    pub fg: String,
    pub accent: String,
    pub warning: String,
    pub error: String,
    pub success: String,
    pub border: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct KeyBindings {
    pub quit: String,
    pub help: String,
    pub next_panel: String,
    pub prev_panel: String,
    pub refresh: String,
    pub search: String,
    pub command: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PanelConfig {
    pub show_compartments: bool,
    pub show_tasks: bool,
    pub show_logs: bool,
    pub show_metrics: bool,
    pub show_threats: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: Theme {
                name: "dark".to_string(),
                bg: "#1a1b26".to_string(),
                fg: "#c0caf5".to_string(),
                accent: "#7aa2f7".to_string(),
                warning: "#e0af68".to_string(),
                error: "#f7768e".to_string(),
                success: "#9ece6a".to_string(),
                border: "#3b4261".to_string(),
            },
            keybindings: KeyBindings {
                quit: "q".to_string(),
                help: "?".to_string(),
                next_panel: "Tab".to_string(),
                prev_panel: "BackTab".to_string(),
                refresh: "r".to_string(),
                search: "/".to_string(),
                command: ":".to_string(),
            },
            panels: PanelConfig {
                show_compartments: true,
                show_tasks: true,
                show_logs: true,
                show_metrics: true,
                show_threats: true,
            },
            auto_refresh_secs: 5,
        }
    }
}