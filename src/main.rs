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
struct Cli;

#[tokio::main]
async fn main() -> Result<()> {
    Cli::parse();

    let sys = Arc::new(RwLock::new(SystemMonitor::new()));
    let docker = Arc::new(RwLock::new(DockerMonitor::connect().await));
    let gpu = Arc::new(RwLock::new(GpuMonitor::new()));

    spawn_sys_collector(Arc::clone(&sys));
    spawn_docker_collector(Arc::clone(&docker));
    spawn_gpu_collector(Arc::clone(&gpu));

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, sys, docker, gpu).await;
    ratatui::restore();
    result
}

fn spawn_sys_collector(sys: Arc<RwLock<SystemMonitor>>) {
    tokio::spawn(async move {
        let mut last = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let dt = last.elapsed().as_secs_f64();
            last = Instant::now();
            sys.write().await.refresh(dt);
        }
    });
}

fn spawn_docker_collector(docker: Arc<RwLock<DockerMonitor>>) {
    tokio::spawn(async move {
        let mut last = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let dt = last.elapsed().as_secs_f64();
            last = Instant::now();
            docker.write().await.refresh(dt).await;
        }
    });
}

fn spawn_gpu_collector(gpu: Arc<RwLock<GpuMonitor>>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            gpu.write().await.refresh();
        }
    });
}

async fn run(
    terminal: &mut ratatui::DefaultTerminal,
    sys: Arc<RwLock<SystemMonitor>>,
    docker: Arc<RwLock<DockerMonitor>>,
    gpu: Arc<RwLock<GpuMonitor>>,
) -> Result<()> {
    let mut app = App::new();
    let mut events = EventStream::new();
    let mut render_tick = interval(Duration::from_millis(250));

    loop {
        tokio::select! {
            _ = render_tick.tick() => {
                let sys_guard = sys.read().await;
                let docker_guard = docker.read().await;
                let gpu_guard = gpu.read().await;
                terminal.draw(|f| ui::draw(f, &app, &sys_guard, &docker_guard, &gpu_guard))?;
            }
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        if let Signal::Quit = app.handle_key(key, &sys, &docker).await {
                            return Ok(());
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e.into()),
                    None => return Ok(()),
                }
            }
        }
    }
}
