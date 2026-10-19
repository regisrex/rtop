use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use crate::app::{App, InputMode};
use crate::system::{SortKey, SystemMonitor};
use crate::ui::util::format_bytes;

pub fn render(frame: &mut Frame, area: Rect, app: &App, sys: &SystemMonitor) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(5)])
        .split(area);

    render_filter_bar(frame, rows[0], app, "name or cmdline");

    let filtered = app.filtered_processes(sys);
    let selected = app.process_selected.min(filtered.len().saturating_sub(1));

    let sort_label = |key: SortKey, label: &str| {
        if sys.sort_key == key {
            format!("{label} {}", if sys.sort_desc { "▼" } else { "▲" })
        } else {
            label.to_string()
        }
    };

    let header = Row::new(vec![
        Cell::from("PID"),
        Cell::from(sort_label(SortKey::Cpu, "CPU%")),
        Cell::from(sort_label(SortKey::Mem, "MEM")),
        Cell::from("STATUS"),
        Cell::from(sort_label(SortKey::Name, "NAME")),
        Cell::from("CMD"),
    ])
    .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD));

    let body_rows: Vec<Row> = filtered
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let style = if i == selected {
                Style::default().bg(Color::Rgb(30, 38, 50))
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(p.pid.to_string()),
                Cell::from(format!("{:.1}", p.cpu_usage)),
                Cell::from(format_bytes(p.memory)),
                Cell::from(p.status.clone()),
                Cell::from(p.name.clone()),
                Cell::from(p.command.clone()),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        body_rows,
        [
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(20),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(format!(" processes ({}) ", filtered.len())),
    );

    frame.render_widget(table, rows[1]);
}

pub fn render_filter_bar(frame: &mut Frame, area: Rect, app: &App, placeholder: &str) {
    let text = if app.input_mode == InputMode::Filter {
        Line::from(vec![
            Span::styled("/ ", Style::default().fg(Color::Rgb(232, 162, 61))),
            Span::raw(app.filter_buffer.clone()),
            Span::styled("▏", Style::default().add_modifier(Modifier::SLOW_BLINK)),
        ])
    } else {
        let active = match app.tab {
            crate::app::Tab::Processes => &app.process_filter,
            crate::app::Tab::Docker => &app.docker_filter,
            _ => "",
        };
        if active.is_empty() {
            Line::from(Span::styled(
                format!("/ filter by {placeholder}"),
                Style::default().fg(Color::DarkGray),
            ))
        } else {
            Line::from(vec![
                Span::styled("/ ", Style::default().fg(Color::Rgb(232, 162, 61))),
                Span::raw(active),
            ])
        }
    };
    frame.render_widget(Paragraph::new(text), area);
}
