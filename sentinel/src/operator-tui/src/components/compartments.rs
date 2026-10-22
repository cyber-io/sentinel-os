//! Compartments panel component

use std::sync::Arc;
use crossterm::event::KeyCode;
use ratatui::style::Color;
use tokio::sync::RwLock;
use tracing::debug;

use crate::state::AppState;
use crate::api::ApiClient;

pub struct CompartmentsPanel {
    state: Arc<RwLock<AppState>>,
    api_client: Arc<ApiClient>,
}

impl CompartmentsPanel {
    pub fn new(state: Arc<RwLock<AppState>>) -> Self {
        // We'll get the api_client from state when needed
        Self {
            state: state.clone(),
            api_client: Arc::new(ApiClient::dummy()), // placeholder
        }
    }

    pub async fn handle_key(&mut self, key: KeyCode, state: &mut AppState) -> anyhow::Result<()> {
        match key {
            KeyCode::Up => {
                if let Some(idx) = state.selected_compartment {
                    if idx > 0 {
                        state.selected_compartment = Some(idx - 1);
                    }
                } else if !state.compartments.is_empty() {
                    state.selected_compartment = Some(0);
                }
            }
            KeyCode::Down => {
                if let Some(idx) = state.selected_compartment {
                    if idx + 1 < state.compartments.len() {
                        state.selected_compartment = Some(idx + 1);
                    }
                } else if !state.compartments.is_empty() {
                    state.selected_compartment = Some(0);
                }
            }
            KeyCode::Enter => {
                if let Some(idx) = state.selected_compartment {
                    if let Some(comp) = state.compartments.get(idx) {
                        state.set_status(format!("Selected compartment: {}", comp.name));
                    }
                }
            }
            KeyCode::Char('s') => {
                if let Some(idx) = state.selected_compartment {
                    if let Some(comp) = state.compartments.get(idx) {
                        // Start compartment
                        state.set_status(format!("Starting {}...", comp.name));
                        // In real implementation: self.api_client.start_compartment(&comp.name).await?;
                    }
                }
            }
            KeyCode::Char('x') => {
                if let Some(idx) = state.selected_compartment {
                    if let Some(comp) = state.compartments.get(idx) {
                        // Stop compartment
                        state.set_status(format!("Stopping {}...", comp.name));
                    }
                }
            }
            KeyCode::Char('d') => {
                if let Some(idx) = state.selected_compartment {
                    if let Some(comp) = state.compartments.get(idx) {
                        // Destroy compartment (with confirmation)
                        state.set_status(format!("Destroy {}? (y/n)", comp.name));
                    }
                }
            }
            KeyCode::Char('n') => {
                // New compartment
                state.set_status("New compartment creation not yet implemented".to_string());
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn update(&mut self, state: &mut AppState) -> anyhow::Result<()> {
        // Auto-update handled by AppState refresh
        Ok(())
    }
}

// Dummy implementation for compilation
impl ApiClient {
    pub fn dummy() -> Self {
        unimplemented!("ApiClient::dummy() should not be called")
    }
}