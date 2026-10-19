use ratatui::style::Color;

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
