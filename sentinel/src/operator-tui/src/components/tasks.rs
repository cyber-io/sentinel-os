//! Tasks panel component

use std::sync::Arc;
use crossterm::event::KeyCode;
use tokio::sync::RwLock;
use anyhow::Result;

use crate::state::AppState;

pub struct TasksPanel {
    state: Arc<RwLock<AppState>>,
}

impl TasksPanel {
    pub fn new(state: Arc<RwLock<AppState>>) -> Self {
        Self { state }
    }

    pub async fn handle_key(&mut self, key: KeyCode, state: &mut AppState) -> Result<()> {
        match key {
            KeyCode::Up => {
                if let Some(idx) = state.selected_task {
                    if idx > 0 {
                        state.selected_task = Some(idx - 1);
                    }
                } else if !state.tasks.is_empty() {
                    state.selected_task = Some(0);
                }
            }
            KeyCode::Down => {
                if let Some(idx) = state.selected_task {
                    if idx + 1 < state.tasks.len() {
                        state.selected_task = Some(idx + 1);
                    }
                } else if !state.tasks.is_empty() {
                    state.selected_task = Some(0);
                }
            }
            KeyCode::Enter => {
                if let Some(idx) = state.selected_task {
                    if let Some(task) = state.tasks.get(idx) {
                        state.set_status(format!("Task: {} ({})", task.description, task.status));
                    }
                }
            }
            KeyCode::Char('c') => {
                if let Some(idx) = state.selected_task {
                    if let Some(task) = state.tasks.get(idx) {
                        state.set_status(format!("Cancelling task {}...", task.id));
                        // In real implementation: cancel task via API
                    }
                }
            }
            KeyCode::Char('n') => {
                // New task
                state.set_status("New task creation - use command mode (:execute)".to_string());
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn update(&mut self, _state: &mut AppState) -> Result<()> {
        Ok(())
    }
}