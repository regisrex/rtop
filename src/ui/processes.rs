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

    // Non-data rows inside the bordered block: top+bottom border (2) + header (1).
    let visible_rows = rows[1].height.saturating_sub(3) as usize;
    let offset = if filtered.len() <= visible_rows || visible_rows == 0 {
        0
    } else {
        selected
            .saturating_sub(visible_rows / 2)
            .min(filtered.len() - visible_rows)
    };
    let window_end = (offset + visible_rows).min(filtered.len());

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

    // Only build rows for the slice that will actually be drawn — with a
    // large process list, rebuilding and cloning strings for every process
    // on every 250ms render tick is the single biggest source of UI lag.
    let body_rows: Vec<Row> = filtered[offset..window_end]
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let style = if offset + i == selected {
                Style::default().bg(Color::Rgb(30, 38, 50))
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(p.pid.to_string()),
                Cell::from(format!("{:.1}", p.cpu_usage)),
                Cell::from(format_bytes(p.memory)),
                Cell::from(p.status.as_str()),
                Cell::from(p.name.as_str()),
                Cell::from(p.command.as_str()),
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
            Span::raw(app.filter_buffer.as_str()),
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
