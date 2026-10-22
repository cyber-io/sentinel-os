//! Logs panel component

use std::sync::Arc;
use crossterm::event::KeyCode;
use tokio::sync::RwLock;
use anyhow::Result;

use crate::state::AppState;

pub struct LogsPanel {
    state: Arc<RwLock<AppState>>,
}

impl LogsPanel {
    pub fn new(state: Arc<RwLock<AppState>>) -> Self {
        Self { state }
    }

    pub async fn handle_key(&mut self, key: KeyCode, state: &mut AppState) -> Result<()> {
        match key {
            KeyCode::Up => {
                if state.logs_scroll > 0 {
                    state.logs_scroll -= 1;
                }
            }
            KeyCode::Down => {
                state.logs_scroll = state.logs_scroll.saturating_add(1);
            }
            KeyCode::PageUp => {
                state.logs_scroll = state.logs_scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                state.logs_scroll = state.logs_scroll.saturating_add(10);
            }
            KeyCode::Home => {
                state.logs_scroll = 0;
            }
            KeyCode::End => {
                state.logs_scroll = state.logs.len().saturating_sub(1);
            }
            KeyCode::Char('f') => {
                // Filter
                state.log_filter = String::new();
                state.enter_search_mode();
            }
            KeyCode::Char('c') => {
                // Clear filter
                state.log_filter.clear();
                state.set_status("Log filter cleared".to_string());
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn update(&mut self, _state: &mut AppState) -> Result<()> {
        Ok(())
    }
}