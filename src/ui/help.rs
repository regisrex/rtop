use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub fn render(frame: &mut Frame, area: Rect) {
    let popup = centered(area, 58, 22);
    frame.render_widget(Clear, popup);

    let key = |k: &'static str| Span::styled(format!("{k:<16}"), Style::default().fg(Color::Rgb(232, 162, 61)));
    let desc = |d: &'static str| Span::raw(d);
    let line = |k, d| Line::from(vec![key(k), desc(d)]);

    let lines = vec![
        Line::from(Span::styled(
            "global",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        line("q / ctrl+c", "quit"),
        line("tab/shift+tab", "cycle view"),
        line("1 2 3", "jump to view"),
        line("?", "toggle this help"),
        Line::raw(""),
        Line::from(Span::styled(
            "overview",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        line("<- / ->  / m", "cycle focused chart"),
        line("z", "60s / 120s / 180s window"),
        Line::raw(""),
        Line::from(Span::styled(
            "processes",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        line("/", "filter by name / cmdline"),
        line("c  M  p  n", "sort cpu / mem / pid / name"),
        line("x", "kill selected (confirm)"),
        Line::raw(""),
        Line::from(Span::styled("docker", Style::default().add_modifier(Modifier::BOLD))),
        line("/", "filter by name / image / status"),
        line("a", "toggle all / running-only"),
        line("s", "stop selected (confirm)"),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(232, 162, 61)))
        .title(" help ");
    frame.render_widget(Paragraph::new(lines).block(block), popup);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    area
}
