//! UI rendering

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
};

use crate::state::{AppState, Panel, ViewMode, parse_color};
use crate::components::help::HelpPanel;
use crate::components::command::CommandPanel;

pub fn draw_ui(frame: &mut Frame, state: &AppState, help_panel: &mut HelpPanel, command_panel: &mut CommandPanel) {
    let theme = &state.config.theme;
    let bg = parse_color(&theme.bg);
    let fg = parse_color(&theme.fg);
    let accent = parse_color(&theme.accent);
    let border_color = parse_color(&theme.border);
    let warning = parse_color(&theme.warning);
    let error = parse_color(&theme.error);
    let success = parse_color(&theme.success);

    let area = frame.area();

    // Main layout
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),     // Body
            Constraint::Length(3),  // Footer
        ])
        .split(area);

    // Header
    draw_header(frame, main_chunks[0], state, bg, fg, accent);

    // Body - tabs and content
    let body_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Tabs
            Constraint::Min(0),     // Content
        ])
        .split(main_chunks[1]);

    draw_tabs(frame, body_chunks[0], state, accent, border_color);
    draw_content(frame, body_chunks[1], state, bg, fg, accent, border_color, warning, error, success);

    // Footer
    draw_footer(frame, main_chunks[2], state, bg, fg, accent);

    // Overlays
    if state.show_help {
        help_panel.render(frame, area, bg, fg, accent, border_color);
    }

    if state.view_mode == ViewMode::Command {
        command_panel.render(frame, area, bg, fg, accent, border_color);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, state: &AppState, bg: Color, fg: Color, accent: Color) {
    let block = Block::default()
        .style(Style::default().bg(bg).fg(fg))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .border_type(border::Rounded);

    let title = Line::from(vec![
        Span::styled("█ SENTINEL OS ", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        Span::styled("Operator Terminal", Style::default().fg(fg)),
    ]);

    let status = if let Some(msg) = &state.status_message {
        if state.status_time.map_or(false, |t| t.elapsed().as_secs() < 5) {
            Span::styled(format!(" | {}", msg), Style::default().fg(fg))
        } else {
            Span::raw("")
        }
    } else {
        Span::raw("")
    };

    let version = Span::styled(
        format!(" v{} ", env!("CARGO_PKG_VERSION")),
        Style::default().fg(accent),
    );

    let header = Paragraph::new(Text::from(vec![
        Line::from(vec![title, status, version]),
    ]))
    .block(block)
    .alignment(Alignment::Left);

    frame.render_widget(header, area);
}

fn draw_tabs(frame: &mut Frame, area: Rect, state: &AppState, accent: Color, border_color: Color) {
    let tabs = Tabs::new(vec![
        "📦 Compartments",
        "📋 Tasks",
        "📜 Logs",
        "📊 Metrics",
        "🛡️ Threats",
    ])
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(border_color)),
    )
    .style(Style::default().fg(Color::DarkGray))
    .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
    .select(state.active_panel as usize)
    .divider(" | ");

    frame.render_widget(tabs, area);
}

fn draw_content(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    bg: Color,
    fg: Color,
    accent: Color,
    border_color: Color,
    warning: Color,
    error: Color,
    success: Color,
) {
    match state.active_panel {
        Panel::Compartments => draw_compartments_panel(frame, area, state, bg, fg, accent, border_color, warning, error, success),
        Panel::Tasks => draw_tasks_panel(frame, area, state, bg, fg, accent, border_color, warning, error, success),
        Panel::Logs => draw_logs_panel(frame, area, state, bg, fg, accent, border_color),
        Panel::Metrics => draw_metrics_panel(frame, area, state, bg, fg, accent, border_color),
        Panel::Threats => draw_threats_panel(frame, area, state, bg, fg, accent, border_color, warning, error, success),
    }
}

fn draw_compartments_panel(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    bg: Color,
    fg: Color,
    accent: Color,
    border_color: Color,
    warning: Color,
    error: Color,
    success: Color,
) {
    let items: Vec<ListItem> = state.compartments.iter().enumerate().map(|(i, c)| {
        let status_style = match c.status.as_str() {
            "Running" => Style::default().fg(success),
            "Starting" | "Creating" => Style::default().fg(warning),
            "Stopping" | "Stopped" => Style::default().fg(Color::DarkGray),
            "Failed" | "Destroying" => Style::default().fg(error),
            _ => Style::default().fg(fg),
        };

        let selected = state.selected_compartment == Some(i);
        let style = if selected {
            Style::default().bg(Color::DarkGray).fg(fg)
        } else {
            Style::default().fg(fg)
        };

        let content = vec![
            Line::from(vec![
                Span::styled(&c.name, Style::default().add_modifier(Modifier::BOLD).fg(accent)),
                Span::raw(" "),
                Span::styled(&c.compartment_type, Style::default().fg(Color::DarkGray)),
                Span::raw(" | "),
                Span::styled(&c.status, status_style.add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled(format!("CPU: {:.1}% ", c.cpu_usage), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("MEM: {:.1}% ", c.memory_usage), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("UP: {}", c.uptime), Style::default().fg(Color::DarkGray)),
            ]),
        ];

        ListItem::new(content).style(style)
    }).collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Compartments ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(
                    if state.active_panel == Panel::Compartments { accent } else { border_color }
                ))
                .border_type(border::Rounded),
        )
        .style(Style::default().bg(bg).fg(fg));

    frame.render_widget(list, area);
}

fn draw_tasks_panel(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    bg: Color,
    fg: Color,
    accent: Color,
    border_color: Color,
    warning: Color,
    error: Color,
    success: Color,
) {
    let items: Vec<ListItem> = state.tasks.iter().enumerate().map(|(i, t)| {
        let status_style = match t.status.as_str() {
            "Completed" => Style::default().fg(success),
            "Running" | "Planning" => Style::default().fg(accent),
            "Pending" => Style::default().fg(warning),
            "Failed" => Style::default().fg(error),
            "Cancelled" => Style::default().fg(Color::DarkGray),
            _ => Style::default().fg(fg),
        };

        let selected = state.selected_task == Some(i);
        let style = if selected {
            Style::default().bg(Color::DarkGray).fg(fg)
        } else {
            Style::default().fg(fg)
        };

        let progress_bar = format_progress_bar(t.progress, 20);

        let content = vec![
            Line::from(vec![
                Span::styled(&t.id[..8.min(t.id.len())], Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(&t.description, Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled(&t.status, status_style.add_modifier(Modifier::BOLD)),
                Span::raw(" "),
                Span::styled(progress_bar, Style::default().fg(accent)),
                Span::raw(format!(" {:.0}%", t.progress * 100.0)),
            ]),
        ];

        ListItem::new(content).style(style)
    }).collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Tasks ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(
                    if state.active_panel == Panel::Tasks { accent } else { border_color }
                ))
                .border_type(border::Rounded),
        )
        .style(Style::default().bg(bg).fg(fg));

    frame.render_widget(list, area);
}

fn draw_logs_panel(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    bg: Color,
    fg: Color,
    accent: Color,
    border_color: Color,
) {
    let filtered_logs: Vec<&LogEntry> = state.logs.iter()
        .rev()
        .filter(|l| state.log_filter.is_empty() || l.message.contains(&state.log_filter))
        .take(area.height as usize)
        .collect();

    let items: Vec<ListItem> = filtered_logs.iter().map(|l| {
        let level_style = match l.level.as_str() {
            "ERROR" => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            "WARN" => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            "INFO" => Style::default().fg(Color::Green),
            "DEBUG" => Style::default().fg(Color::DarkGray),
            _ => Style::default().fg(fg),
        };

        let content = Line::from(vec![
            Span::styled(l.timestamp.format("%H:%M:%S").to_string(), Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(format!("[{:<5}]", l.level), level_style),
            Span::raw(" "),
            Span::styled(&l.source, Style::default().fg(accent)),
            Span::raw(": "),
            Span::styled(&l.message, Style::default().fg(fg)),
        ]);

        ListItem::new(content)
    }).collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(" Logs{} ", if state.log_filter.is_empty() { String::new() } else { format!(" [filter: {}]", state.log_filter) }))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(
                    if state.active_panel == Panel::Logs { accent } else { border_color }
                ))
                .border_type(border::Rounded),
        )
        .style(Style::default().bg(bg).fg(fg));

    frame.render_widget(list, area);
}

fn draw_metrics_panel(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    bg: Color,
    fg: Color,
    accent: Color,
    border_color: Color,
) {
    let metrics: Vec<(&String, &MetricData)> = state.metrics.iter().collect();

    let items: Vec<ListItem> = metrics.iter().map(|(name, m)| {
        let content = Line::from(vec![
            Span::styled(name, Style::default().add_modifier(Modifier::BOLD).fg(accent)),
            Span::raw(": "),
            Span::styled(format!("{:.2}", m.value), Style::default().fg(fg)),
            Span::raw(" "),
            Span::styled(
                m.labels.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join(", "),
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        ListItem::new(content)
    }).collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Metrics ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(
                    if state.active_panel == Panel::Metrics { accent } else { border_color }
                ))
                .border_type(border::Rounded),
        )
        .style(Style::default().bg(bg).fg(fg));

    frame.render_widget(list, area);
}

fn draw_threats_panel(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    bg: Color,
    fg: Color,
    accent: Color,
    border_color: Color,
    warning: Color,
    error: Color,
    success: Color,
) {
    let items: Vec<ListItem> = state.threats.iter().enumerate().map(|(i, t)| {
        let severity_style = match t.severity.to_lowercase().as_str() {
            "critical" => Style::default().fg(error).add_modifier(Modifier::BOLD),
            "high" => Style::default().fg(Color::Red),
            "medium" => Style::default().fg(warning),
            "low" => Style::default().fg(Color::Blue),
            _ => Style::default().fg(fg),
        };

        let selected = state.selected_task == Some(i); // reuse selected_task for threats
        let style = if selected {
            Style::default().bg(Color::DarkGray).fg(fg)
        } else {
            Style::default().fg(fg)
        };

        let status_style = match t.status.to_lowercase().as_str() {
            "open" => Style::default().fg(error),
            "investigating" => Style::default().fg(warning),
            "resolved" => Style::default().fg(success),
            "closed" => Style::default().fg(Color::DarkGray),
            _ => Style::default().fg(fg),
        };

        let content = vec![
            Line::from(vec![
                Span::styled(&t.severity.to_uppercase(), severity_style.add_modifier(Modifier::BOLD)),
                Span::raw(" "),
                Span::styled(&t.title, Style::default().add_modifier(Modifier::BOLD).fg(fg)),
                Span::raw(" "),
                Span::styled(&t.status, status_style),
            ]),
            Line::from(vec![
                Span::styled(&t.source, Style::default().fg(accent)),
                Span::raw(" | "),
                Span::styled(t.detected_at.format("%Y-%m-%d %H:%M").to_string(), Style::default().fg(Color::DarkGray)),
            ]),
        ];

        ListItem::new(content).style(style)
    }).collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Threats ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(
                    if state.active_panel == Panel::Threats { accent } else { border_color }
                ))
                .border_type(border::Rounded),
        )
        .style(Style::default().bg(bg).fg(fg));

    frame.render_widget(list, area);
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &AppState, bg: Color, fg: Color, accent: Color) {
    let help_text = match state.view_mode {
        ViewMode::Normal => "Tab:Next Panel | ?:Help | r:Refresh | :Command | /:Search | q:Quit",
        ViewMode::Command => "Enter:Execute | Esc:Cancel",
        ViewMode::Search => "Enter:Search | Esc:Cancel",
        ViewMode::Help => "Esc:Close Help",
    };

    let footer = Paragraph::new(help_text)
        .style(Style::default().bg(bg).fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(accent)),
        );

    frame.render_widget(footer, area);
}

fn format_progress_bar(progress: f32, width: usize) -> String {
    let filled = (progress * width as f32) as usize;
    let empty = width - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}