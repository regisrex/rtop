mod app;
mod docker;
mod gpu;
mod history;
mod system;
mod ui;

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use crossterm::event::{Event, EventStream, KeyEventKind};
use futures_util::StreamExt;
use tokio::sync::RwLock;
use tokio::time::interval;

use app::{App, Signal};
use docker::DockerMonitor;
use gpu::GpuMonitor;
use system::SystemMonitor;

/// htop, but with time-series charts, gauges, and Docker filtering.
#[derive(Parser)]
#[command(name = "rtop", version, about)]
struct Cli {
    /// How often to refresh metrics and redraw, in milliseconds. Keystrokes
    /// (typing a filter, moving the selection) still redraw instantly
    /// regardless of this value — it only governs the idle refresh rate, so
    /// raising it lowers rtop's own CPU usage without making it feel laggy.
    #[arg(short, long, default_value_t = 1000, value_name = "MS")]
    interval: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let interval_ms = cli.interval.max(100);

    let sys = Arc::new(RwLock::new(SystemMonitor::new()));
    let docker = Arc::new(RwLock::new(DockerMonitor::connect().await));
    let gpu = Arc::new(RwLock::new(GpuMonitor::new()));

    spawn_sys_collector(Arc::clone(&sys), interval_ms);
    spawn_docker_collector(Arc::clone(&docker), interval_ms * 2);
    spawn_gpu_collector(Arc::clone(&gpu), interval_ms);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, sys, docker, gpu, interval_ms).await;
    ratatui::restore();
    result
}

fn spawn_sys_collector(sys: Arc<RwLock<SystemMonitor>>, interval_ms: u64) {
    tokio::spawn(async move {
        let mut last = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            let dt = last.elapsed().as_secs_f64();
            last = Instant::now();
            sys.write().await.refresh(dt);
        }
    });
}

fn spawn_docker_collector(docker: Arc<RwLock<DockerMonitor>>, interval_ms: u64) {
    tokio::spawn(async move {
        let mut last = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            let dt = last.elapsed().as_secs_f64();
            last = Instant::now();
            docker.write().await.refresh(dt).await;
        }
    });
}

fn spawn_gpu_collector(gpu: Arc<RwLock<GpuMonitor>>, interval_ms: u64) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            gpu.write().await.refresh();
        }
    });
}

async fn run(
    terminal: &mut ratatui::DefaultTerminal,
    sys: Arc<RwLock<SystemMonitor>>,
    docker: Arc<RwLock<DockerMonitor>>,
    gpu: Arc<RwLock<GpuMonitor>>,
    interval_ms: u64,
) -> Result<()> {
    let mut app = App::new();
    let mut events = EventStream::new();
    // Redraws at the same cadence metrics actually change at — redrawing
    // faster than that just burns CPU on an identical frame. Input still
    // feels instant: every handled key/resize triggers its own immediate
    // redraw below, independent of this tick.
    let mut render_tick = interval(Duration::from_millis(interval_ms));

    draw_frame(terminal, &app, &sys, &docker, &gpu).await?;

    loop {
        tokio::select! {
            _ = render_tick.tick() => {
                draw_frame(terminal, &app, &sys, &docker, &gpu).await?;
            }
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        if let Signal::Quit = app.handle_key(key, &sys, &docker).await {
                            return Ok(());
                        }
                        draw_frame(terminal, &app, &sys, &docker, &gpu).await?;
                    }
                    Some(Ok(Event::Resize(_, _))) => {
                        draw_frame(terminal, &app, &sys, &docker, &gpu).await?;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e.into()),
                    None => return Ok(()),
                }
            }
        }
    }
}

async fn draw_frame(
    terminal: &mut ratatui::DefaultTerminal,
    app: &App,
    sys: &Arc<RwLock<SystemMonitor>>,
    docker: &Arc<RwLock<DockerMonitor>>,
    gpu: &Arc<RwLock<GpuMonitor>>,
) -> Result<()> {
    let sys_guard = sys.read().await;
    let docker_guard = docker.read().await;
    let gpu_guard = gpu.read().await;
    terminal.draw(|f| ui::draw(f, app, &sys_guard, &docker_guard, &gpu_guard))?;
    Ok(())
}
