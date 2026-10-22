//! Threats panel component

use std::sync::Arc;
use crossterm::event::KeyCode;
use tokio::sync::RwLock;
use anyhow::Result;

use crate::state::AppState;

pub struct ThreatsPanel {
    state: Arc<RwLock<AppState>>,
}

impl ThreatsPanel {
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
                } else if !state.threats.is_empty() {
                    state.selected_task = Some(0);
                }
            }
            KeyCode::Down => {
                if let Some(idx) = state.selected_task {
                    if idx + 1 < state.threats.len() {
                        state.selected_task = Some(idx + 1);
                    }
                } else if !state.threats.is_empty() {
                    state.selected_task = Some(0);
                }
            }
            KeyCode::Enter => {
                if let Some(idx) = state.selected_task {
                    if let Some(threat) = state.threats.get(idx) {
                        state.set_status(format!("Threat: {} [{}]", threat.title, threat.severity));
                    }
                }
            }
            KeyCode::Char('i') => {
                if let Some(idx) = state.selected_task {
                    if let Some(threat) = state.threats.get(idx) {
                        state.set_status(format!("Investigating {}...", threat.id));
                        // In real implementation: trigger investigation
                    }
                }
            }
            KeyCode::Char('r') => {
                if let Some(idx) = state.selected_task {
                    if let Some(threat) = state.threats.get(idx) {
                        state.set_status(format!("Marking {} as resolved", threat.id));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn update(&mut self, _state: &mut AppState) -> Result<()> {
        Ok(())
    }
}