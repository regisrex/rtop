use super::{GpuStat, NpuStat};

/// Fallback backend for platforms without GPU/NPU support wired up.
pub struct Backend;

impl Backend {
    pub fn new() -> Self {
        Self
    }

    pub fn sample(&mut self) -> (Vec<GpuStat>, Option<NpuStat>) {
        (Vec::new(), None)
    }
}
