//! Help panel component

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::state::{parse_color, AppConfig};

pub struct HelpPanel;

impl HelpPanel {
    pub fn new() -> Self {
        Self
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, bg: Color, fg: Color, accent: Color, border_color: Color) {
        let popup_area = centered_rect(70, 80, area);
        frame.render_widget(Clear, popup_area);

        let help_text = vec![
            Line::from(vec![
                Span::styled("SENTINEL OS OPERATOR TUI - KEYBINDINGS", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("GLOBAL", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  Tab / Shift+Tab", Style::default().fg(fg)),
                Span::styled("  - Switch panels", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  ?", Style::default().fg(fg)),
                Span::styled("  - Toggle this help", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  r", Style::default().fg(fg)),
                Span::styled("  - Refresh data", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  :", Style::default().fg(fg)),
                Span::styled("  - Enter command mode", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  /", Style::default().fg(fg)),
                Span::styled("  - Enter search/filter mode", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  Esc", Style::default().fg(fg)),
                Span::styled("  - Exit mode / Close help", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  q", Style::default().fg(fg)),
                Span::styled("  - Quit", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("COMPARTMENTS PANEL", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  ↑/↓", Style::default().fg(fg)),
                Span::styled("  - Navigate", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  Enter", Style::default().fg(fg)),
                Span::styled("  - Select compartment", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  s", Style::default().fg(fg)),
                Span::styled("  - Start compartment", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  x", Style::default().fg(fg)),
                Span::styled("  - Stop compartment", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  d", Style::default().fg(fg)),
                Span::styled("  - Destroy compartment", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  n", Style::default().fg(fg)),
                Span::styled("  - New compartment", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("TASKS PANEL", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  ↑/↓", Style::default().fg(fg)),
                Span::styled("  - Navigate", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  Enter", Style::default().fg(fg)),
                Span::styled("  - View task details", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  c", Style::default().fg(fg)),
                Span::styled("  - Cancel task", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("LOGS PANEL", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  ↑/↓", Style::default().fg(fg)),
                Span::styled("  - Scroll", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  PgUp/PgDn", Style::default().fg(fg)),
                Span::styled("  - Page scroll", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  Home/End", Style::default().fg(fg)),
                Span::styled("  - Jump to start/end", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  f", Style::default().fg(fg)),
                Span::styled("  - Filter logs", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  c", Style::default().fg(fg)),
                Span::styled("  - Clear filter", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("THREATS PANEL", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  ↑/↓", Style::default().fg(fg)),
                Span::styled("  - Navigate", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  Enter", Style::default().fg(fg)),
                Span::styled("  - View threat details", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  i", Style::default().fg(fg)),
                Span::styled("  - Investigate", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  r", Style::default().fg(fg)),
                Span::styled("  - Mark resolved", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("COMMAND MODE", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  :execute <task>", Style::default().fg(fg)),
                Span::styled("  - Execute a security task", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  :compartment create <name> <type>", Style::default().fg(fg)),
                Span::styled("  - Create compartment", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  :compartment start|stop|destroy <name>", Style::default().fg(fg)),
                Span::styled("  - Manage compartment", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  :task cancel <id>", Style::default().fg(fg)),
                Span::styled("  - Cancel task", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  :theme <name>", Style::default().fg(fg)),
                Span::styled("  - Change theme", Style::default().fg(Color::DarkGray)),
            ]),
        ];

        let paragraph = Paragraph::new(help_text)
            .style(Style::default().bg(bg).fg(fg))
            .block(
                Block::default()
                    .title(" Help (Esc to close) ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color))
                    .border_type(ratatui::symbols::border::Rounded),
            )
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left);

        frame.render_widget(paragraph, popup_area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}