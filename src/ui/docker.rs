use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use crate::app::App;
use crate::docker::DockerMonitor;
use crate::ui::processes::render_filter_bar;
use crate::ui::util::{format_bytes, load_color, text_sparkline};

pub fn render(frame: &mut Frame, area: Rect, app: &App, docker: &DockerMonitor) {
    if !docker.available {
        render_unavailable(frame, area, docker);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(5)])
        .split(area);

    render_filter_bar(frame, rows[0], app, "name, image or status");

    let filtered = app.filtered_containers(docker);
    let selected = app.docker_selected.min(filtered.len().saturating_sub(1));

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

    let header = Row::new(vec![
        Cell::from("NAME"),
        Cell::from("IMAGE"),
        Cell::from("STATUS"),
        Cell::from("CPU"),
        Cell::from("CPU%"),
        Cell::from("MEM"),
        Cell::from("NET rx/tx"),
        Cell::from("PIDS"),
    ])
    .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD));

    let body_rows: Vec<Row> = filtered[offset..window_end]
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let style = if offset + i == selected {
                Style::default().bg(Color::Rgb(30, 38, 50))
            } else if !c.running {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            let status_span = if c.running {
                format!("● {}", c.status)
            } else {
                format!("○ {}", c.status)
            };
            let spark = text_sparkline(&c.cpu_history.values(), 10);
            Row::new(vec![
                Cell::from(c.name.as_str()),
                Cell::from(c.image.as_str()),
                Cell::from(status_span),
                Cell::from(spark).style(Style::default().fg(load_color(c.cpu_pct))),
                Cell::from(format!("{:.1}", c.cpu_pct)),
                Cell::from(if c.running {
                    format!("{} / {}", format_bytes(c.mem_usage), format_bytes(c.mem_limit))
                } else {
                    "—".into()
                }),
                Cell::from(if c.running {
                    format!(
                        "{}/s / {}/s",
                        format_bytes(c.net_rx_bps as u64),
                        format_bytes(c.net_tx_bps as u64)
                    )
                } else {
                    "—".into()
                }),
                Cell::from(if c.running { c.pids.to_string() } else { "—".into() }),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        body_rows,
        [
            Constraint::Length(18),
            Constraint::Length(20),
            Constraint::Length(16),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(18),
            Constraint::Length(20),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(format!(
                " containers ({}{}) ",
                filtered.len(),
                if app.docker_show_all { "" } else { ", running only" }
            )),
    );

    frame.render_widget(table, rows[1]);
}

fn render_unavailable(frame: &mut Frame, area: Rect, docker: &DockerMonitor) {
    let msg = docker
        .error
        .as_deref()
        .unwrap_or("docker daemon unreachable");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(226, 76, 76)))
        .title(" docker ");
    let text = format!("⚠ {msg}\n\nNo daemon detected at startup — this panel will stay empty until rtop is restarted with Docker reachable.");
    let p = Paragraph::new(text)
        .block(block)
        .style(Style::default().fg(Color::Rgb(243, 185, 185)))
        .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(p, area);
}
