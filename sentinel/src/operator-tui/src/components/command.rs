//! Command panel component

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::state::{AppState, parse_color};

pub struct CommandPanel {
    state: std::sync::Arc<tokio::sync::RwLock<AppState>>,
}

impl CommandPanel {
    pub fn new(state: std::sync::Arc<tokio::sync::RwLock<AppState>>) -> Self {
        Self { state }
    }

    pub async fn handle_key(&mut self, key: crossterm::event::KeyCode, state: &mut AppState) -> anyhow::Result<()> {
        match key {
            crossterm::event::KeyCode::Char(c) => {
                state.command_buffer.push(c);
            }
            crossterm::event::KeyCode::Backspace => {
                state.command_buffer.pop();
            }
            crossterm::event::KeyCode::Enter => {
                self.execute_command(&state.command_buffer, state).await?;
                state.exit_mode();
            }
            crossterm::event::KeyCode::Esc => {
                state.exit_mode();
            }
            _ => {}
        }
        Ok(())
    }

    async fn execute_command(&self, cmd: &str, state: &mut AppState) -> anyhow::Result<()> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        match parts[0] {
            "execute" | "exec" => {
                if parts.len() < 2 {
                    state.set_status("Usage: :execute <task description>".to_string());
                    return Ok(());
                }
                let task_desc = parts[1..].join(" ");
                state.set_status(format!("Executing: {}", task_desc));
                // In real implementation: call orchestrator API
            }
            "compartment" | "comp" => {
                if parts.len() < 2 {
                    state.set_status("Usage: :compartment <create|start|stop|destroy> <name> [type]".to_string());
                    return Ok(());
                }
                match parts[1] {
                    "create" => {
                        if parts.len() < 4 {
                            state.set_status("Usage: :compartment create <name> <type>".to_string());
                        } else {
                            state.set_status(format!("Creating compartment {} ({})", parts[2], parts[3]));
                        }
                    }
                    "start" => {
                        if parts.len() < 3 {
                            state.set_status("Usage: :compartment start <name>".to_string());
                        } else {
                            state.set_status(format!("Starting compartment {}", parts[2]));
                        }
                    }
                    "stop" => {
                        if parts.len() < 3 {
                            state.set_status("Usage: :compartment stop <name>".to_string());
                        } else {
                            state.set_status(format!("Stopping compartment {}", parts[2]));
                        }
                    }
                    "destroy" => {
                        if parts.len() < 3 {
                            state.set_status("Usage: :compartment destroy <name>".to_string());
                        } else {
                            state.set_status(format!("Destroying compartment {}", parts[2]));
                        }
                    }
                    _ => {
                        state.set_status(format!("Unknown compartment command: {}", parts[1]));
                    }
                }
            }
            "task" => {
                if parts.len() < 2 {
                    state.set_status("Usage: :task <cancel|status> <id>".to_string());
                    return Ok(());
                }
                match parts[1] {
                    "cancel" => {
                        if parts.len() < 3 {
                            state.set_status("Usage: :task cancel <id>".to_string());
                        } else {
                            state.set_status(format!("Cancelling task {}", parts[2]));
                        }
                    }
                    "status" => {
                        if parts.len() < 3 {
                            state.set_status("Usage: :task status <id>".to_string());
                        } else {
                            state.set_status(format!("Getting status for task {}", parts[2]));
                        }
                    }
                    _ => {
                        state.set_status(format!("Unknown task command: {}", parts[1]));
                    }
                }
            }
            "theme" => {
                if parts.len() < 2 {
                    state.set_status("Usage: :theme <name>".to_string());
                } else {
                    state.set_status(format!("Theme change to {} not yet implemented", parts[1]));
                }
            }
            "help" => {
                state.toggle_help();
            }
            "refresh" | "reload" => {
                state.set_status("Refreshing...".to_string());
            }
            _ => {
                state.set_status(format!("Unknown command: {}", parts[0]));
            }
        }

        Ok(())
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, bg: Color, fg: Color, accent: Color, border_color: Color) {
        let popup_area = centered_rect(60, 15, area);
        frame.render_widget(Clear, popup_area);

        let input = Paragraph::new(format!(":{}", self.get_command_buffer()))
            .style(Style::default().bg(bg).fg(fg))
            .block(
                Block::default()
                    .title(" Command (Enter to execute, Esc to cancel) ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(accent))
                    .border_type(ratatui::symbols::border::Rounded),
            )
            .alignment(Alignment::Left);

        frame.render_widget(input, popup_area);
    }

    fn get_command_buffer(&self) -> String {
        // This is a simplified version - in real implementation would read from state
        String::new()
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Percentage((100 - percent_y) / 2),
            ratatui::layout::Constraint::Percentage(percent_y),
            ratatui::layout::Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage((100 - percent_x) / 2),
            ratatui::layout::Constraint::Percentage(percent_x),
            ratatui::layout::Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}