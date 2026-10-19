use std::collections::VecDeque;

/// Fixed-capacity ring buffer of samples used to back every time-series chart.
#[derive(Clone)]
pub struct History {
    samples: VecDeque<f64>,
    capacity: usize,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, value: f64) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(value);
    }

    /// Raw values, oldest first.
    pub fn values(&self) -> Vec<f64> {
        self.samples.iter().copied().collect()
    }

    /// Points formatted for ratatui's `Chart`/`Dataset`: (x, y) pairs, oldest first.
    pub fn as_points(&self) -> Vec<(f64, f64)> {
        self.samples
            .iter()
            .enumerate()
            .map(|(i, v)| (i as f64, *v))
            .collect()
    }

    /// Values only, oldest first, for the `Sparkline` widget (expects u64).
    pub fn as_u64(&self, scale: f64) -> Vec<u64> {
        self.samples.iter().map(|v| (*v * scale) as u64).collect()
    }
}
