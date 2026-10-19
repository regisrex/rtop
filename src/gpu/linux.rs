use std::fs;

use super::{GpuStat, GpuVendor, NpuStat};

const VENDOR_AMD: &str = "0x1002";
const VENDOR_NVIDIA: &str = "0x10de";
const VENDOR_INTEL: &str = "0x8086";

/// Linux GPU backend. NVIDIA is read through NVML (dynamically loaded, so a
/// machine without the NVIDIA driver just reports none); AMD is read from the
/// standard `amdgpu` sysfs files (no root needed); Intel integrated GPUs are
/// detected but not read (no equivalent no-root busy% file for i915/Xe).
///
/// NPU is unconditionally unavailable here — Linux NPU sysfs interfaces
/// (Intel `ivpu`, AMD `amdxdna`) are too new/unstable to implement without
/// hardware to verify against.
pub struct Backend {
    nvml: Option<nvml_wrapper::Nvml>,
}

impl Backend {
    pub fn new() -> Self {
        Self {
            nvml: nvml_wrapper::Nvml::init().ok(),
        }
    }

    pub fn sample(&mut self) -> (Vec<GpuStat>, Option<NpuStat>) {
        let mut gpus = Vec::new();

        if let Some(nvml) = &self.nvml {
            gpus.extend(sample_nvidia(nvml));
        }

        gpus.extend(sample_sysfs(self.nvml.is_some()));

        (gpus, None)
    }
}

fn sample_nvidia(nvml: &nvml_wrapper::Nvml) -> Vec<GpuStat> {
    let Ok(count) = nvml.device_count() else {
        return Vec::new();
    };

    (0..count)
        .filter_map(|i| nvml.device_by_index(i).ok())
        .map(|device| {
            let name = device.name().unwrap_or_else(|_| "NVIDIA GPU".to_string());
            let util = device.utilization_rates().ok();
            let mem = device.memory_info().ok();
            GpuStat {
                name,
                vendor: GpuVendor::Nvidia,
                util_pct: util.map(|u| u.gpu as f64),
                mem_used: mem.as_ref().map(|m| m.used),
                mem_total: mem.as_ref().map(|m| m.total),
            }
        })
        .collect()
}

/// Reads `/sys/class/drm/card*/device/` for AMD (via `gpu_busy_percent`,
/// standard on the `amdgpu` driver) and detects Intel integrated GPUs
/// (present but unread). Skips NVIDIA entries when NVML already covered them.
fn sample_sysfs(skip_nvidia: bool) -> Vec<GpuStat> {
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };

    let mut stats = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        // Only bare card directories (e.g. "card0"), not connector entries
        // like "card0-DP-1".
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }

        let device_dir = entry.path().join("device");
        let Some(vendor) = read_trimmed(&device_dir.join("vendor")) else {
            continue;
        };

        match vendor.as_str() {
            VENDOR_AMD => {
                let busy = read_trimmed(&device_dir.join("gpu_busy_percent"))
                    .and_then(|s| s.parse::<f64>().ok());
                let mem_used = read_trimmed(&device_dir.join("mem_info_vram_used"))
                    .and_then(|s| s.parse::<u64>().ok());
                let mem_total = read_trimmed(&device_dir.join("mem_info_vram_total"))
                    .and_then(|s| s.parse::<u64>().ok());
                stats.push(GpuStat {
                    name: "AMD GPU".to_string(),
                    vendor: GpuVendor::Amd,
                    util_pct: busy,
                    mem_used,
                    mem_total,
                });
            }
            VENDOR_NVIDIA if !skip_nvidia => {
                stats.push(GpuStat {
                    name: "NVIDIA GPU".to_string(),
                    vendor: GpuVendor::Nvidia,
                    util_pct: None,
                    mem_used: None,
                    mem_total: None,
                });
            }
            VENDOR_INTEL => {
                stats.push(GpuStat {
                    name: "Intel GPU".to_string(),
                    vendor: GpuVendor::Intel,
                    util_pct: None,
                    mem_used: None,
                    mem_total: None,
                });
            }
            _ => {}
        }
    }
    stats
}

fn read_trimmed(path: &std::path::Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}
