//! Bounded wall-frame sampling for Starfall performance diagnostics.
//! Samples measure wall frame intervals, never GPU execution time.

use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameSummary {
    pub samples: usize,
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}

#[derive(Debug)]
pub struct FrameSamples {
    samples: VecDeque<f64>,
    capacity: usize,
}

impl FrameSamples {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::new(),
            capacity,
        }
    }

    pub fn record_ms(&mut self, ms: f64) {
        if self.capacity == 0 || !ms.is_finite() || ms <= 0.0 {
            return;
        }
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(ms);
    }

    /// Exact nearest-rank percentiles over the retained window. Call on demand;
    /// collecting samples does not sort or allocate after the window fills.
    pub fn summary(&self) -> FrameSummary {
        if self.samples.is_empty() {
            return FrameSummary::default();
        }
        let mut sorted: Vec<_> = self.samples.iter().copied().collect();
        sorted.sort_unstable_by(f64::total_cmp);
        let samples = sorted.len();
        let percentile = |p: f64| sorted[((samples as f64 * p).ceil() as usize).saturating_sub(1)];
        FrameSummary {
            samples,
            mean_ms: sorted.iter().map(|value| value / samples as f64).sum(),
            p50_ms: percentile(0.50),
            p95_ms: percentile(0.95),
            p99_ms: percentile(0.99),
            max_ms: sorted[samples - 1],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percentiles_track_spikes_and_age_out_old_samples() {
        let mut frames = FrameSamples::new(100);
        frames.record_ms(f64::NAN);
        frames.record_ms(f64::INFINITY);
        frames.record_ms(-1.0);
        frames.record_ms(0.0);
        assert_eq!(frames.summary().samples, 0);
        for ms in 1..=100 {
            frames.record_ms(ms as f64);
        }
        assert_eq!(
            frames.summary(),
            FrameSummary {
                samples: 100,
                mean_ms: 50.5,
                p50_ms: 50.0,
                p95_ms: 95.0,
                p99_ms: 99.0,
                max_ms: 100.0,
            }
        );
        for _ in 0..100 {
            frames.record_ms(16.0);
        }
        assert_eq!(frames.summary().max_ms, 16.0);
        let mut disabled = FrameSamples::new(0);
        disabled.record_ms(12.0);
        assert_eq!(disabled.summary(), FrameSummary::default());
    }
}
