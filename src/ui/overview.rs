use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, Gauge, GraphType, Paragraph, Sparkline};

use crate::app::{App, ChartMetric};
use crate::gpu::GpuMonitor;
use crate::system::SystemMonitor;
use crate::ui::util::{format_bytes, format_rate, load_color};

pub fn render(frame: &mut Frame, area: Rect, app: &App, sys: &SystemMonitor, gpu: &GpuMonitor) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(core_rows(sys.core_count) as u16 * 2),
            Constraint::Length(5),
            Constraint::Min(10),
            Constraint::Length(6),
        ])
        .split(area);

    render_header(frame, rows[0], sys);
    render_cores(frame, rows[1], sys);
    render_gauges(frame, rows[2], sys, gpu);
    render_focused_chart(frame, rows[3], app, sys, gpu);
    render_spark_strip(frame, rows[4], app, sys, gpu);
}

fn render_header(frame: &mut Frame, area: Rect, sys: &SystemMonitor) {
    let uptime = sysinfo::System::uptime();
    let load = sysinfo::System::load_average();
    let line = Line::from(vec![
        Span::styled(
            sys.hostname.clone(),
            Style::default()
                .fg(Color::Rgb(232, 162, 61))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::raw(format!("{} ({})", sys.os_version, sys.kernel_version)),
        Span::raw("   "),
        Span::styled(sys.cpu_brand.clone(), Style::default().fg(Color::DarkGray)),
        Span::raw("   "),
        Span::styled(
            format!("up {}", format_uptime(uptime)),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("   "),
        Span::styled(
            format!("load {:.2} {:.2} {:.2}", load.one, load.five, load.fifteen),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn format_uptime(secs: u64) -> String {
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d}d {h:02}h")
    } else {
        format!("{h:02}h {m:02}m")
    }
}

fn core_rows(core_count: usize) -> usize {
    let cols = core_cols(core_count);
    core_count.div_ceil(cols)
}

fn core_cols(core_count: usize) -> usize {
    core_count.clamp(1, 8)
}

fn render_cores(frame: &mut Frame, area: Rect, sys: &SystemMonitor) {
    let cols = core_cols(sys.core_count);
    let rows = core_rows(sys.core_count);

    let row_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(2); rows])
        .split(area);

    for r in 0..rows {
        let col_areas = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Ratio(1, cols as u32); cols])
            .split(row_areas[r]);
        for c in 0..cols {
            let idx = r * cols + c;
            if idx >= sys.per_core_cpu.len() {
                continue;
            }
            let pct = sys.per_core_cpu[idx];
            let gauge = Gauge::default()
                .block(Block::default())
                .ratio((pct as f64 / 100.0).clamp(0.0, 1.0))
                .label(format!("C{idx} {pct:>3.0}%"))
                .gauge_style(Style::default().fg(load_color(pct as f64)));
            frame.render_widget(gauge, col_areas[c]);
        }
    }
}

fn render_gauges(frame: &mut Frame, area: Rect, sys: &SystemMonitor, gpu: &GpuMonitor) {
    let cpu_pct = sys.overall_cpu as f64;
    let mem_pct = if sys.mem_total > 0 {
        sys.mem_used as f64 / sys.mem_total as f64 * 100.0
    } else {
        0.0
    };
    let swap_pct = if sys.swap_total > 0 {
        sys.swap_used as f64 / sys.swap_total as f64 * 100.0
    } else {
        0.0
    };

    let mut specs = vec![
        (
            "CPU".to_string(),
            Some(cpu_pct),
            format!("{cpu_pct:.1}%  ({} cores)", sys.core_count),
        ),
        (
            "MEM".to_string(),
            Some(mem_pct),
            format!("{} / {}", format_bytes(sys.mem_used), format_bytes(sys.mem_total)),
        ),
        (
            "SWAP".to_string(),
            Some(swap_pct),
            format!("{} / {}", format_bytes(sys.swap_used), format_bytes(sys.swap_total)),
        ),
    ];

    specs.push(match gpu.primary_gpu() {
        Some(g) => (
            format!("{} ({})", g.name, g.vendor.label()),
            g.util_pct,
            match (g.util_pct, g.mem_used, g.mem_total) {
                (Some(p), Some(used), Some(total)) => {
                    format!("{p:.1}%  {} / {}", format_bytes(used), format_bytes(total))
                }
                (Some(p), Some(used), None) => format!("{p:.1}%  {}", format_bytes(used)),
                (Some(p), None, _) => format!("{p:.1}%"),
                (None, _, _) => "n/a".to_string(),
            },
        ),
        None => ("GPU".to_string(), None, "n/a".to_string()),
    });

    specs.push(match &gpu.npu {
        Some(n) => (
            n.name.clone(),
            n.util_pct_approx,
            match (n.util_pct_approx, n.power_w) {
                (Some(p), Some(w)) => format!("~{p:.0}%  {w:.1}W"),
                (None, Some(w)) => format!("{w:.1}W"),
                _ => "n/a".to_string(),
            },
        ),
        None => ("NPU".to_string(), None, "n/a".to_string()),
    });

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, specs.len() as u32); specs.len()])
        .split(area);

    for (i, (title, pct, detail)) in specs.into_iter().enumerate() {
        let color = pct.map(load_color).unwrap_or(Color::DarkGray);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {title} "))
            .border_style(Style::default().fg(Color::DarkGray));
        let gauge = Gauge::default()
            .block(block)
            .ratio((pct.unwrap_or(0.0) / 100.0).clamp(0.0, 1.0))
            .label(detail)
            .gauge_style(Style::default().fg(color));
        frame.render_widget(gauge, cols[i]);
    }
}

struct Series {
    name: &'static str,
    color: Color,
    points: Vec<(f64, f64)>,
}

struct FocusedSeries {
    unit: &'static str,
    series: Vec<Series>,
    y_max: f64,
    reading: String,
}

fn focused_series(app: &App, sys: &SystemMonitor, gpu: &GpuMonitor) -> FocusedSeries {
    let window = app.window_len;
    match app.chart_metric {
        ChartMetric::Cpu => FocusedSeries {
            unit: "%",
            series: vec![Series {
                name: "cpu",
                color: Color::Rgb(232, 162, 61),
                points: windowed(&sys.cpu_history.as_points(), window),
            }],
            y_max: 100.0,
            reading: format!("{:.1}%", sys.overall_cpu),
        },
        ChartMetric::Memory => {
            let mem_pct = if sys.mem_total > 0 {
                sys.mem_used as f64 / sys.mem_total as f64 * 100.0
            } else {
                0.0
            };
            FocusedSeries {
                unit: "%",
                series: vec![Series {
                    name: "mem",
                    color: Color::Rgb(139, 127, 214),
                    points: windowed(&sys.mem_history.as_points(), window),
                }],
                y_max: 100.0,
                reading: format!("{mem_pct:.1}%"),
            }
        }
        ChartMetric::Network => {
            let rx = windowed(&sys.net_rx_history.as_points(), window);
            let tx = windowed(&sys.net_tx_history.as_points(), window);
            let y_max = rx
                .iter()
                .chain(tx.iter())
                .map(|(_, v)| *v)
                .fold(1.0_f64, f64::max)
                * 1.2;
            FocusedSeries {
                unit: "B/s",
                series: vec![
                    Series { name: "rx", color: Color::Rgb(63, 199, 184), points: rx },
                    Series { name: "tx", color: Color::Rgb(226, 76, 76), points: tx },
                ],
                y_max,
                reading: format!(
                    "rx {} · tx {}",
                    format_rate(sys.net_rx_bps),
                    format_rate(sys.net_tx_bps)
                ),
            }
        }
        ChartMetric::Disk => {
            let read = windowed(&sys.disk_read_history.as_points(), window);
            let write = windowed(&sys.disk_write_history.as_points(), window);
            let y_max = read
                .iter()
                .chain(write.iter())
                .map(|(_, v)| *v)
                .fold(1.0_f64, f64::max)
                * 1.2;
            FocusedSeries {
                unit: "B/s",
                series: vec![
                    Series { name: "read", color: Color::Rgb(232, 162, 61), points: read },
                    Series { name: "write", color: Color::Rgb(226, 105, 138), points: write },
                ],
                y_max,
                reading: format!(
                    "r {} · w {}",
                    format_rate(sys.disk_read_bps),
                    format_rate(sys.disk_write_bps)
                ),
            }
        }
        ChartMetric::Gpu => {
            let reading = match gpu.primary_gpu().and_then(|g| g.util_pct) {
                Some(p) => format!("{p:.1}%"),
                None => "n/a".to_string(),
            };
            FocusedSeries {
                unit: "%",
                series: vec![Series {
                    name: "gpu",
                    color: Color::Rgb(232, 162, 61),
                    points: windowed(&gpu.gpu_history.as_points(), window),
                }],
                y_max: 100.0,
                reading,
            }
        }
        ChartMetric::Npu => {
            let reading = match &gpu.npu {
                Some(n) => match (n.util_pct_approx, n.power_w) {
                    (Some(p), Some(w)) => format!("~{p:.0}%  {w:.1}W"),
                    _ => "n/a".to_string(),
                },
                None => "n/a".to_string(),
            };
            FocusedSeries {
                unit: "%",
                series: vec![Series {
                    name: "npu",
                    color: Color::Rgb(139, 127, 214),
                    points: windowed(&gpu.npu_history.as_points(), window),
                }],
                y_max: 100.0,
                reading,
            }
        }
    }
}

fn windowed(points: &[(f64, f64)], window: usize) -> Vec<(f64, f64)> {
    let start = points.len().saturating_sub(window);
    points[start..]
        .iter()
        .enumerate()
        .map(|(i, (_, v))| (i as f64, *v))
        .collect()
}

fn render_focused_chart(frame: &mut Frame, area: Rect, app: &App, sys: &SystemMonitor, gpu: &GpuMonitor) {
    let data = focused_series(app, sys, gpu);
    let title = match app.chart_metric {
        ChartMetric::Cpu => "CPU",
        ChartMetric::Memory => "MEMORY",
        ChartMetric::Network => "NETWORK",
        ChartMetric::Disk => "DISK I/O",
        ChartMetric::Gpu => "GPU",
        ChartMetric::Npu => "NPU (approx, Apple Neural Engine)",
    };

    let datasets: Vec<Dataset> = data
        .series
        .iter()
        .map(|s| {
            Dataset::default()
                .name(s.name)
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(s.color))
                .data(&s.points)
        })
        .collect();

    let x_max = (app.window_len.saturating_sub(1)) as f64;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(format!(
            " {title} · last {}s ",
            app.window_len
        ))
        .title_bottom(Line::from(data.reading).right_aligned());

    let chart = Chart::new(datasets)
        .block(block)
        .x_axis(Axis::default().bounds([0.0, x_max.max(1.0)]))
        .y_axis(
            Axis::default()
                .bounds([0.0, data.y_max.max(1.0)])
                .labels(vec![
                    Line::from("0"),
                    Line::from(format!("{:.0}", data.y_max / 2.0)),
                    Line::from(format!("{:.0}{}", data.y_max, data.unit)),
                ]),
        );
    frame.render_widget(chart, area);
}

fn render_spark_strip(frame: &mut Frame, area: Rect, app: &App, sys: &SystemMonitor, gpu: &GpuMonitor) {
    let metrics = [
        ChartMetric::Cpu,
        ChartMetric::Memory,
        ChartMetric::Network,
        ChartMetric::Disk,
        ChartMetric::Gpu,
        ChartMetric::Npu,
    ];
    let others: Vec<ChartMetric> = metrics.into_iter().filter(|m| *m != app.chart_metric).collect();

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, others.len() as u32); others.len()])
        .split(area);

    for (i, metric) in others.into_iter().enumerate() {
        let (label, value, data, color) = match metric {
            ChartMetric::Cpu => (
                "CPU",
                format!("{:.1}%", sys.overall_cpu),
                sys.cpu_history.as_u64(1.0),
                Color::Rgb(232, 162, 61),
            ),
            ChartMetric::Memory => {
                let pct = if sys.mem_total > 0 {
                    sys.mem_used as f64 / sys.mem_total as f64 * 100.0
                } else {
                    0.0
                };
                (
                    "MEM",
                    format!("{pct:.1}%"),
                    sys.mem_history.as_u64(1.0),
                    Color::Rgb(139, 127, 214),
                )
            }
            ChartMetric::Network => (
                "NET",
                format!("{}/s", format_bytes((sys.net_rx_bps + sys.net_tx_bps) as u64)),
                sys.net_rx_history.as_u64(1.0),
                Color::Rgb(63, 199, 184),
            ),
            ChartMetric::Disk => (
                "DISK",
                format!("{}/s", format_bytes((sys.disk_read_bps + sys.disk_write_bps) as u64)),
                sys.disk_write_history.as_u64(1.0),
                Color::Rgb(226, 105, 138),
            ),
            ChartMetric::Gpu => (
                "GPU",
                match gpu.primary_gpu().and_then(|g| g.util_pct) {
                    Some(p) => format!("{p:.1}%"),
                    None => "n/a".to_string(),
                },
                gpu.gpu_history.as_u64(1.0),
                Color::Rgb(232, 162, 61),
            ),
            ChartMetric::Npu => (
                "NPU",
                match gpu.npu.as_ref().and_then(|n| n.util_pct_approx) {
                    Some(p) => format!("~{p:.0}%"),
                    None => "n/a".to_string(),
                },
                gpu.npu_history.as_u64(1.0),
                Color::Rgb(139, 127, 214),
            ),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(format!(" {label}  {value} "));
        let spark = Sparkline::default().block(block).data(data).style(Style::default().fg(color));
        frame.render_widget(spark, cols[i]);
    }
}
