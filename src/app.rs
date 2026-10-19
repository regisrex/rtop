use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sysinfo::Pid;
use tokio::sync::RwLock;

use crate::docker::DockerMonitor;
use crate::system::{SortKey, SystemMonitor};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Processes,
    Docker,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChartMetric {
    Cpu,
    Memory,
    Network,
    Disk,
    Gpu,
    Npu,
}

impl ChartMetric {
    pub fn next(self) -> Self {
        match self {
            ChartMetric::Cpu => ChartMetric::Memory,
            ChartMetric::Memory => ChartMetric::Network,
            ChartMetric::Network => ChartMetric::Disk,
            ChartMetric::Disk => ChartMetric::Gpu,
            ChartMetric::Gpu => ChartMetric::Npu,
            ChartMetric::Npu => ChartMetric::Cpu,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            ChartMetric::Cpu => ChartMetric::Npu,
            ChartMetric::Memory => ChartMetric::Cpu,
            ChartMetric::Network => ChartMetric::Memory,
            ChartMetric::Disk => ChartMetric::Network,
            ChartMetric::Gpu => ChartMetric::Disk,
            ChartMetric::Npu => ChartMetric::Gpu,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Filter,
}

#[derive(Clone)]
pub enum PendingAction {
    KillProcess(Pid, String),
    StopContainer(String, String),
}

pub enum Signal {
    Quit,
    None,
}

pub struct App {
    pub tab: Tab,
    pub chart_metric: ChartMetric,
    pub window_len: usize,

    pub input_mode: InputMode,
    pub filter_buffer: String,
    pub process_filter: String,
    pub docker_filter: String,

    pub process_selected: usize,
    pub docker_selected: usize,
    pub docker_show_all: bool,

    pub pending_action: Option<PendingAction>,
    pub show_help: bool,
    pub status_line: Option<(String, std::time::Instant)>,
}

impl App {
    pub fn new() -> Self {
        Self {
            tab: Tab::Overview,
            chart_metric: ChartMetric::Cpu,
            window_len: 180,
            input_mode: InputMode::Normal,
            filter_buffer: String::new(),
            process_filter: String::new(),
            docker_filter: String::new(),
            process_selected: 0,
            docker_selected: 0,
            docker_show_all: true,
            pending_action: None,
            show_help: false,
            status_line: None,
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_line = Some((msg.into(), std::time::Instant::now()));
    }

    pub fn status_text(&self) -> Option<&str> {
        self.status_line.as_ref().and_then(|(msg, at)| {
            if at.elapsed().as_secs() < 4 {
                Some(msg.as_str())
            } else {
                None
            }
        })
    }

    pub async fn handle_key(
        &mut self,
        key: KeyEvent,
        sys: &Arc<RwLock<SystemMonitor>>,
        docker: &Arc<RwLock<DockerMonitor>>,
    ) -> Signal {
        // Confirmation popup takes priority over everything else.
        if let Some(action) = self.pending_action.clone() {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.pending_action = None;
                    match action {
                        PendingAction::KillProcess(pid, name) => {
                            let ok = sys.read().await.kill_process(pid);
                            self.set_status(if ok {
                                format!("sent SIGKILL to {name} ({pid})")
                            } else {
                                format!("failed to kill {name} ({pid})")
                            });
                        }
                        PendingAction::StopContainer(id, name) => {
                            let res = docker.read().await.stop_container(&id).await;
                            self.set_status(match res {
                                Ok(_) => format!("stopped container {name}"),
                                Err(e) => format!("failed to stop {name}: {e}"),
                            });
                        }
                    }
                }
                _ => {
                    self.pending_action = None;
                    self.set_status("cancelled");
                }
            }
            return Signal::None;
        }

        if self.show_help {
            if matches!(key.code, KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q')) {
                self.show_help = false;
            }
            return Signal::None;
        }

        if self.input_mode == InputMode::Filter {
            match key.code {
                KeyCode::Esc => {
                    self.input_mode = InputMode::Normal;
                    self.filter_buffer.clear();
                    self.apply_filter();
                }
                KeyCode::Enter => {
                    self.input_mode = InputMode::Normal;
                }
                KeyCode::Backspace => {
                    self.filter_buffer.pop();
                    self.apply_filter();
                }
                KeyCode::Char(c) => {
                    self.filter_buffer.push(c);
                    self.apply_filter();
                }
                _ => {}
            }
            return Signal::None;
        }

        match key.code {
            KeyCode::Char('q') => return Signal::Quit,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Signal::Quit;
            }
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Tab => {
                self.tab = match self.tab {
                    Tab::Overview => Tab::Processes,
                    Tab::Processes => Tab::Docker,
                    Tab::Docker => Tab::Overview,
                };
            }
            KeyCode::BackTab => {
                self.tab = match self.tab {
                    Tab::Overview => Tab::Docker,
                    Tab::Processes => Tab::Overview,
                    Tab::Docker => Tab::Processes,
                };
            }
            KeyCode::Char('1') => self.tab = Tab::Overview,
            KeyCode::Char('2') => self.tab = Tab::Processes,
            KeyCode::Char('3') => self.tab = Tab::Docker,
            KeyCode::Char('m') if self.tab == Tab::Overview => {
                self.chart_metric = self.chart_metric.next();
            }
            KeyCode::Left if self.tab == Tab::Overview => {
                self.chart_metric = self.chart_metric.prev();
            }
            KeyCode::Right if self.tab == Tab::Overview => {
                self.chart_metric = self.chart_metric.next();
            }
            KeyCode::Char('z') if self.tab == Tab::Overview => {
                self.window_len = match self.window_len {
                    60 => 120,
                    120 => 180,
                    _ => 60,
                };
            }
            KeyCode::Char('/') if self.tab != Tab::Overview => {
                self.input_mode = InputMode::Filter;
                self.filter_buffer = match self.tab {
                    Tab::Processes => self.process_filter.clone(),
                    Tab::Docker => self.docker_filter.clone(),
                    Tab::Overview => String::new(),
                };
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1, sys, docker).await,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1, sys, docker).await,
            KeyCode::Char('c') if self.tab == Tab::Processes => {
                self.set_process_sort(sys, SortKey::Cpu).await;
            }
            KeyCode::Char('M') if self.tab == Tab::Processes => {
                self.set_process_sort(sys, SortKey::Mem).await;
            }
            KeyCode::Char('p') if self.tab == Tab::Processes => {
                self.set_process_sort(sys, SortKey::Pid).await;
            }
            KeyCode::Char('n') if self.tab == Tab::Processes => {
                self.set_process_sort(sys, SortKey::Name).await;
            }
            KeyCode::Char('x') if self.tab == Tab::Processes => {
                let guard = sys.read().await;
                let rows = self.filtered_processes(&guard);
                if let Some(row) = rows.get(self.process_selected) {
                    self.pending_action =
                        Some(PendingAction::KillProcess(row.pid, row.name.clone()));
                }
            }
            KeyCode::Char('a') if self.tab == Tab::Docker => {
                self.docker_show_all = !self.docker_show_all;
                self.docker_selected = 0;
            }
            KeyCode::Char('s') if self.tab == Tab::Docker => {
                let guard = docker.read().await;
                let rows = self.filtered_containers(&guard);
                if let Some(row) = rows.get(self.docker_selected) {
                    self.pending_action = Some(PendingAction::StopContainer(
                        row.id.clone(),
                        row.name.clone(),
                    ));
                }
            }
            _ => {}
        }
        Signal::None
    }

    fn apply_filter(&mut self) {
        match self.tab {
            Tab::Processes => self.process_filter = self.filter_buffer.clone(),
            Tab::Docker => self.docker_filter = self.filter_buffer.clone(),
            Tab::Overview => {}
        }
    }

    async fn move_selection(
        &mut self,
        delta: i32,
        sys: &Arc<RwLock<SystemMonitor>>,
        docker: &Arc<RwLock<DockerMonitor>>,
    ) {
        match self.tab {
            Tab::Processes => {
                let guard = sys.read().await;
                let len = self.filtered_processes(&guard).len();
                let max = len.saturating_sub(1) as i32;
                self.process_selected = (self.process_selected as i32 + delta).clamp(0, max) as usize;
            }
            Tab::Docker => {
                let guard = docker.read().await;
                let len = self.filtered_containers(&guard).len();
                let max = len.saturating_sub(1) as i32;
                self.docker_selected = (self.docker_selected as i32 + delta).clamp(0, max) as usize;
            }
            Tab::Overview => {}
        }
    }

    async fn set_process_sort(&mut self, sys: &Arc<RwLock<SystemMonitor>>, key: SortKey) {
        let mut guard = sys.write().await;
        if guard.sort_key == key {
            guard.sort_desc = !guard.sort_desc;
        } else {
            guard.sort_key = key;
            guard.sort_desc = true;
        }
    }

    pub fn filtered_processes<'a>(
        &self,
        sys: &'a SystemMonitor,
    ) -> Vec<&'a crate::system::ProcessRow> {
        let needle = self.process_filter.to_lowercase();
        sys.processes
            .iter()
            .filter(|p| {
                needle.is_empty()
                    || p.name.to_lowercase().contains(&needle)
                    || p.command.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn filtered_containers<'a>(
        &self,
        docker: &'a DockerMonitor,
    ) -> Vec<&'a crate::docker::ContainerRow> {
        let needle = self.docker_filter.to_lowercase();
        docker
            .containers
            .iter()
            .filter(|c| self.docker_show_all || c.running)
            .filter(|c| {
                needle.is_empty()
                    || c.name.to_lowercase().contains(&needle)
                    || c.image.to_lowercase().contains(&needle)
                    || c.status.to_lowercase().contains(&needle)
            })
            .collect()
    }
}
