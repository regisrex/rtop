use sysinfo::{Disks, Networks, Pid, ProcessRefreshKind, RefreshKind, System, UpdateKind};

use crate::history::History;

pub const HISTORY_LEN: usize = 180;

pub struct ProcessRow {
    pub pid: Pid,
    pub name: String,
    pub cpu_usage: f32,
    pub memory: u64,
    pub status: String,
    pub command: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Cpu,
    Mem,
    Pid,
    Name,
}

pub struct SystemMonitor {
    sys: System,
    networks: Networks,
    disks: Disks,

    pub hostname: String,
    pub os_version: String,
    pub kernel_version: String,
    pub cpu_brand: String,
    pub core_count: usize,

    pub overall_cpu: f32,
    pub per_core_cpu: Vec<f32>,
    pub cpu_history: History,
    pub per_core_history: Vec<History>,

    pub mem_total: u64,
    pub mem_used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub mem_history: History,
    pub swap_history: History,

    pub net_rx_bps: f64,
    pub net_tx_bps: f64,
    pub net_rx_history: History,
    pub net_tx_history: History,

    pub disk_read_bps: f64,
    pub disk_write_bps: f64,
    pub disk_read_history: History,
    pub disk_write_history: History,

    pub processes: Vec<ProcessRow>,
    pub sort_key: SortKey,
    pub sort_desc: bool,

    first_tick: bool,
}

impl SystemMonitor {
    pub fn new() -> Self {
        let mut sys = System::new_with_specifics(
            RefreshKind::everything()
                .with_processes(ProcessRefreshKind::everything())
                .with_cpu(sysinfo::CpuRefreshKind::everything()),
        );
        sys.refresh_all();

        let core_count = sys.cpus().len();
        let cpu_brand = sys
            .cpus()
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_default();

        Self {
            hostname: System::host_name().unwrap_or_else(|| "unknown".into()),
            os_version: System::long_os_version().unwrap_or_else(|| "unknown os".into()),
            kernel_version: System::kernel_version().unwrap_or_else(|| "?".into()),
            cpu_brand,
            core_count,
            networks: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            overall_cpu: 0.0,
            per_core_cpu: vec![0.0; core_count],
            cpu_history: History::new(HISTORY_LEN),
            per_core_history: (0..core_count).map(|_| History::new(HISTORY_LEN)).collect(),
            mem_total: 0,
            mem_used: 0,
            swap_total: 0,
            swap_used: 0,
            mem_history: History::new(HISTORY_LEN),
            swap_history: History::new(HISTORY_LEN),
            net_rx_bps: 0.0,
            net_tx_bps: 0.0,
            net_rx_history: History::new(HISTORY_LEN),
            net_tx_history: History::new(HISTORY_LEN),
            disk_read_bps: 0.0,
            disk_write_bps: 0.0,
            disk_read_history: History::new(HISTORY_LEN),
            disk_write_history: History::new(HISTORY_LEN),
            processes: Vec::new(),
            sort_key: SortKey::Cpu,
            sort_desc: true,
            first_tick: true,
            sys,
        }
    }

    /// Call this roughly once per `interval_secs` seconds.
    pub fn refresh(&mut self, interval_secs: f64) {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        self.sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::everything()
                .with_cpu()
                .with_memory()
                .with_user(UpdateKind::OnlyIfNotSet),
        );
        self.networks.refresh(true);
        self.disks.refresh_specifics(true, sysinfo::DiskRefreshKind::everything());

        self.overall_cpu = self.sys.global_cpu_usage();
        self.cpu_history.push(self.overall_cpu as f64);

        for (i, cpu) in self.sys.cpus().iter().enumerate() {
            self.per_core_cpu[i] = cpu.cpu_usage();
            self.per_core_history[i].push(cpu.cpu_usage() as f64);
        }

        self.mem_total = self.sys.total_memory();
        self.mem_used = self.sys.used_memory();
        self.swap_total = self.sys.total_swap();
        self.swap_used = self.sys.used_swap();
        let mem_pct = if self.mem_total > 0 {
            self.mem_used as f64 / self.mem_total as f64 * 100.0
        } else {
            0.0
        };
        let swap_pct = if self.swap_total > 0 {
            self.swap_used as f64 / self.swap_total as f64 * 100.0
        } else {
            0.0
        };
        self.mem_history.push(mem_pct);
        self.swap_history.push(swap_pct);

        if self.first_tick {
            self.first_tick = false;
        } else {
            let rx: u64 = self.networks.list().values().map(|n| n.received()).sum();
            let tx: u64 = self
                .networks
                .list()
                .values()
                .map(|n| n.transmitted())
                .sum();
            self.net_rx_bps = rx as f64 / interval_secs;
            self.net_tx_bps = tx as f64 / interval_secs;

            let read: u64 = self
                .disks
                .list()
                .iter()
                .map(|d| d.usage().read_bytes)
                .sum();
            let write: u64 = self
                .disks
                .list()
                .iter()
                .map(|d| d.usage().written_bytes)
                .sum();
            self.disk_read_bps = read as f64 / interval_secs;
            self.disk_write_bps = write as f64 / interval_secs;
        }
        self.net_rx_history.push(self.net_rx_bps);
        self.net_tx_history.push(self.net_tx_bps);
        self.disk_read_history.push(self.disk_read_bps);
        self.disk_write_history.push(self.disk_write_bps);

        self.rebuild_process_list();
    }

    fn rebuild_process_list(&mut self) {
        self.processes = self
            .sys
            .processes()
            .iter()
            .map(|(pid, p)| ProcessRow {
                pid: *pid,
                name: p.name().to_string_lossy().to_string(),
                cpu_usage: p.cpu_usage(),
                memory: p.memory(),
                status: p.status().to_string(),
                command: p
                    .cmd()
                    .iter()
                    .map(|s| s.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
            })
            .collect();

        let key = self.sort_key;
        let desc = self.sort_desc;
        self.processes.sort_by(|a, b| {
            let ord = match key {
                SortKey::Cpu => a.cpu_usage.partial_cmp(&b.cpu_usage).unwrap(),
                SortKey::Mem => a.memory.cmp(&b.memory),
                SortKey::Pid => a.pid.cmp(&b.pid),
                SortKey::Name => a.name.cmp(&b.name),
            };
            if desc {
                ord.reverse()
            } else {
                ord
            }
        });
    }

    pub fn kill_process(&self, pid: Pid) -> bool {
        self.sys
            .process(pid)
            .map(|p| p.kill())
            .unwrap_or(false)
    }
}
