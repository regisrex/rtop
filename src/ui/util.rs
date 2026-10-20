use ratatui::style::{Color, Style};
use ratatui::text::Span;

/// Dim, theme-independent track color for the unfilled portion of a bar —
/// rendered explicitly rather than left blank, so every bar always shows its
/// full 0-100% extent regardless of the current value.
pub const TRACK: Color = Color::Rgb(42, 52, 66);

/// Green -> amber -> red based on load percentage, used for every gauge/meter.
pub fn load_color(pct: f64) -> Color {
    if pct < 50.0 {
        Color::Rgb(76, 195, 138)
    } else if pct < 80.0 {
        Color::Rgb(232, 162, 61)
    } else {
        Color::Rgb(226, 76, 76)
    }
}

/// A fixed-width horizontal bar: `width` characters wide, `pct` of it filled
/// with `color`, the remainder drawn as a visible dim track — never just
/// blank space — so the bar's full range reads clearly at any value,
/// including 0%. Used instead of `ratatui::widgets::Gauge` wherever several
/// bars need to line up in a tight grid: `Gauge` centers its label within the
/// *whole* widget, which looks fine at a high ratio but strands the label far
/// from a tiny fill sliver at a low one.
pub fn track_bar<'a>(width: u16, pct: Option<f64>, color: Color) -> Vec<Span<'a>> {
    let width = width.max(1) as usize;
    let filled = pct
        .map(|p| (((p / 100.0).clamp(0.0, 1.0)) * width as f64).round() as usize)
        .unwrap_or(0)
        .min(width);

    let mut spans = Vec::with_capacity(2);
    if filled > 0 {
        spans.push(Span::styled("█".repeat(filled), Style::default().fg(color)));
    }
    if width > filled {
        spans.push(Span::styled(
            "░".repeat(width - filled),
            Style::default().fg(TRACK),
        ));
    }
    spans
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn format_rate(bytes_per_sec: f64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec as u64))
}

const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Renders a `History` as a compact string of block characters, for use inside
/// table cells where a real `Sparkline` widget can't be embedded.
pub fn text_sparkline(values: &[f64], width: usize) -> String {
    if values.is_empty() {
        return " ".repeat(width);
    }
    let start = values.len().saturating_sub(width);
    let slice = &values[start..];
    let max = slice.iter().cloned().fold(0.0_f64, f64::max).max(1.0);
    let mut s: String = slice
        .iter()
        .map(|v| {
            let idx = ((*v / max) * (BLOCKS.len() - 1) as f64).round() as usize;
            BLOCKS[idx.min(BLOCKS.len() - 1)]
        })
        .collect();
    while s.chars().count() < width {
        s.push(' ');
    }
    s
}
