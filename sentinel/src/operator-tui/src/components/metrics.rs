//! Metrics panel component

use std::sync::Arc;
use crossterm::event::KeyCode;
use tokio::sync::RwLock;
use anyhow::Result;

use crate::state::AppState;

pub struct MetricsPanel {
    state: Arc<RwLock<AppState>>,
}

impl MetricsPanel {
    pub fn new(state: Arc<RwLock<AppState>>) -> Self {
        Self { state }
    }

    pub async fn handle_key(&mut self, key: KeyCode, _state: &mut AppState) -> Result<()> {
        match key {
            KeyCode::Char('r') => {
                // Refresh metrics
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn update(&mut self, _state: &mut AppState) -> Result<()> {
        Ok(())
    }
}