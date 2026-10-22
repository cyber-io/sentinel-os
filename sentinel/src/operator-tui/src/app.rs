//! Main application logic

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::state::{AppState, Panel, ViewMode};
use crate::ui::draw_ui;
use crate::components::{
    compartments::CompartmentsPanel,
    tasks::TasksPanel,
    logs::LogsPanel,
    metrics::MetricsPanel,
    threats::ThreatsPanel,
    help::HelpPanel,
    command::CommandPanel,
};

pub struct App {
    state: Arc<RwLock<AppState>>,
    compartments_panel: CompartmentsPanel,
    tasks_panel: TasksPanel,
    logs_panel: LogsPanel,
    metrics_panel: MetricsPanel,
    threats_panel: ThreatsPanel,
    help_panel: HelpPanel,
    command_panel: CommandPanel,
    should_quit: bool,
}

impl App {
    pub fn new(state: Arc<RwLock<AppState>>) -> Self {
        Self {
            compartments_panel: CompartmentsPanel::new(state.clone()),
            tasks_panel: TasksPanel::new(state.clone()),
            logs_panel: LogsPanel::new(state.clone()),
            metrics_panel: MetricsPanel::new(state.clone()),
            threats_panel: ThreatsPanel::new(state.clone()),
            help_panel: HelpPanel::new(),
            command_panel: CommandPanel::new(state.clone()),
            state,
            should_quit: false,
        }
    }

    pub async fn handle_key(&mut self, key: KeyCode) -> Result<bool> {
        let mut state = self.state.write().await;

        // Global shortcuts
        match key {
            KeyCode::Char('q') => {
                self.should_quit = true;
                return Ok(true);
            }
            KeyCode::Char('?') => {
                state.toggle_help();
                return Ok(false);
            }
            KeyCode::Tab => {
                state.next_panel();
                return Ok(false);
            }
            KeyCode::BackTab => {
                state.prev_panel();
                return Ok(false);
            }
            KeyCode::Char('r') => {
                state.trigger_refresh().await;
                return Ok(false);
            }
            KeyCode::Char(':') => {
                state.enter_command_mode();
                return Ok(false);
            }
            KeyCode::Char('/') => {
                state.enter_search_mode();
                return Ok(false);
            }
            KeyCode::Esc => {
                if state.view_mode == ViewMode::Command || state.view_mode == ViewMode::Search {
                    state.exit_mode();
                } else if state.show_help {
                    state.toggle_help();
                }
                return Ok(false);
            }
            _ => {}
        }

        // Panel-specific handling
        if !state.show_help {
            match state.active_panel {
                Panel::Compartments => {
                    self.compartments_panel.handle_key(key, &mut state).await?;
                }
                Panel::Tasks => {
                    self.tasks_panel.handle_key(key, &mut state).await?;
                }
                Panel::Logs => {
                    self.logs_panel.handle_key(key, &mut state).await?;
                }
                Panel::Metrics => {
                    self.metrics_panel.handle_key(key, &mut state).await?;
                }
                Panel::Threats => {
                    self.threats_panel.handle_key(key, &mut state).await?;
                }
            }
        }

        // Command mode handling
        if state.view_mode == ViewMode::Command {
            self.command_panel.handle_key(key, &mut state).await?;
        }

        Ok(self.should_quit)
    }

    pub fn on_resize(&mut self) {
        // Handle resize if needed
    }

    pub async fn update(&mut self) -> Result<()> {
        let mut state = self.state.write().await;

        // Auto-refresh
        if state.should_auto_refresh() {
            state.refresh_data().await?;
        }

        // Update panels
        self.compartments_panel.update(&mut state).await?;
        self.tasks_panel.update(&mut state).await?;
        self.logs_panel.update(&mut state).await?;
        self.metrics_panel.update(&mut state).await?;
        self.threats_panel.update(&mut state).await?;

        Ok(())
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let state = self.state.blocking_read();
        draw_ui(frame, &state, &mut self.help_panel, &mut self.command_panel);
    }
}