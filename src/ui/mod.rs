mod docker;
mod help;
mod overview;
mod processes;
mod util;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};

use crate::app::{App, PendingAction, Tab};
use crate::docker::DockerMonitor;
use crate::system::SystemMonitor;

const ACCENT: Color = Color::Rgb(232, 162, 61);
const DANGER: Color = Color::Rgb(226, 76, 76);

pub fn draw(frame: &mut Frame, app: &App, sys: &SystemMonitor, docker: &DockerMonitor) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(5), Constraint::Length(1)])
        .split(area);

    render_tabs(frame, rows[0], app);

    let content = Block::default().borders(Borders::NONE);
    frame.render_widget(content, rows[1]);
    let inner = rows[1];

    match app.tab {
        Tab::Overview => overview::render(frame, inner, app, sys),
        Tab::Processes => processes::render(frame, inner, app, sys),
        Tab::Docker => docker::render(frame, inner, app, docker),
    }

    render_status(frame, rows[2], app);

    if app.show_help {
        help::render(frame, area);
    } else if let Some(action) = &app.pending_action {
        render_confirm(frame, area, action);
    }
}

fn render_tabs(frame: &mut Frame, area: Rect, app: &App) {
    let titles = ["1 overview", "2 processes", "3 docker"];
    let selected = match app.tab {
        Tab::Overview => 0,
        Tab::Processes => 1,
        Tab::Docker => 2,
    };
    let tabs = Tabs::new(titles.iter().map(|t| Line::from(*t)))
        .select(selected)
        .divider(" ")
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(ACCENT)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        );
    frame.render_widget(tabs, area);
}

fn render_status(frame: &mut Frame, area: Rect, app: &App) {
    let line = if let Some(msg) = app.status_text() {
        Line::from(Span::styled(msg, Style::default().fg(ACCENT)))
    } else {
        let hints: &[(&str, &str)] = match app.tab {
            Tab::Overview => &[("tab", "switch"), ("←/→", "metric"), ("z", "window"), ("?", "help"), ("q", "quit")],
            Tab::Processes => &[
                ("tab", "switch"),
                ("/", "filter"),
                ("c/M/p/n", "sort"),
                ("x", "kill"),
                ("?", "help"),
                ("q", "quit"),
            ],
            Tab::Docker => &[
                ("tab", "switch"),
                ("/", "filter"),
                ("a", "all/running"),
                ("s", "stop"),
                ("?", "help"),
                ("q", "quit"),
            ],
        };
        let mut spans = Vec::new();
        for (i, (k, d)) in hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(*k, Style::default().fg(Color::DarkGray)));
            spans.push(Span::raw(format!(" {d}")));
        }
        Line::from(spans)
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn render_confirm(frame: &mut Frame, area: Rect, action: &PendingAction) {
    let popup = centered(area, 54, 5);
    frame.render_widget(Clear, popup);

    let msg = match action {
        PendingAction::KillProcess(pid, name) => format!("Send SIGKILL to {name} (pid {pid})?"),
        PendingAction::StopContainer(_, name) => format!("Stop container {name}?"),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DANGER))
        .title(" confirm ");
    let text = vec![
        Line::from(msg),
        Line::raw(""),
        Line::from(vec![
            Span::styled("y", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::raw(" confirm   "),
            Span::styled("any other key", Style::default().fg(Color::DarkGray)),
            Span::raw(" cancel"),
        ]),
    ];
    frame.render_widget(Paragraph::new(text).block(block), popup);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    area
}
