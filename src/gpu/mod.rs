#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos::Backend;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::Backend;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use other::Backend;

use crate::history::History;

const HISTORY_LEN: usize = 180;

// Only a subset of these variants is constructed on any single target
// (e.g. only `Apple` on macOS) — not dead code, just platform-specific.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Apple,
    Nvidia,
    Amd,
    Intel,
}

impl GpuVendor {
    pub fn label(self) -> &'static str {
        match self {
            GpuVendor::Apple => "Apple",
            GpuVendor::Nvidia => "NVIDIA",
            GpuVendor::Amd => "AMD",
            GpuVendor::Intel => "Intel",
        }
    }
}

pub struct GpuStat {
    pub name: String,
    pub vendor: GpuVendor,
    pub util_pct: Option<f64>,
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
}

pub struct NpuStat {
    pub name: String,
    pub power_w: Option<f64>,
    pub util_pct_approx: Option<f64>,
}

pub struct GpuMonitor {
    backend: Backend,
    pub gpus: Vec<GpuStat>,
    pub npu: Option<NpuStat>,
    pub gpu_history: History,
    pub npu_history: History,
}

impl GpuMonitor {
    pub fn new() -> Self {
        Self {
            backend: Backend::new(),
            gpus: Vec::new(),
            npu: None,
            gpu_history: History::new(HISTORY_LEN),
            npu_history: History::new(HISTORY_LEN),
        }
    }

    pub fn refresh(&mut self) {
        let (gpus, npu) = self.backend.sample();

        let primary_pct = gpus.iter().find_map(|g| g.util_pct).unwrap_or(0.0);
        self.gpu_history.push(primary_pct);

        let npu_pct = npu.as_ref().and_then(|n| n.util_pct_approx).unwrap_or(0.0);
        self.npu_history.push(npu_pct);

        self.gpus = gpus;
        self.npu = npu;
    }

    /// The GPU backing the gauge/chart: first one reporting real utilization,
    /// falling back to the first detected GPU (present but unread) if none do.
    pub fn primary_gpu(&self) -> Option<&GpuStat> {
        self.gpus
            .iter()
            .find(|g| g.util_pct.is_some())
            .or_else(|| self.gpus.first())
    }
}
