//! Counter to rate conversion, used by modules before they push.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::metric::MetricId;

/// Per-second rate between two counter totals; `None` on reset or zero interval.
pub fn per_sec(prev: f64, cur: f64, dt: Duration) -> Option<f64> {
    let secs = dt.as_secs_f64();
    (cur >= prev && secs > 0.0).then(|| (cur - prev) / secs)
}

/// A module's previous running totals, keyed by metric.
#[derive(Debug, Default)]
pub struct Rates(HashMap<MetricId, (f64, Instant)>);

impl Rates {
    /// Rate since the last total for `id`; `None` on the first total or a counter reset.
    pub fn update(&mut self, id: &MetricId, total: f64, now: Instant) -> Option<f64> {
        let prev = match self.0.get_mut(id) {
            Some(slot) => Some(std::mem::replace(slot, (total, now))),
            None => {
                self.0.insert(id.clone(), (total, now));
                None
            }
        };
        prev.and_then(|(v, t)| per_sec(v, total, now.saturating_duration_since(t)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_rate() {
        assert_eq!(
            per_sec(100.0, 300.0, Duration::from_millis(500)),
            Some(400.0)
        );
        assert_eq!(per_sec(300.0, 100.0, Duration::from_secs(1)), None);
        assert_eq!(per_sec(100.0, 300.0, Duration::ZERO), None);
    }

    #[test]
    fn keys_rates_by_metric() {
        const A: MetricId = MetricId::new("test.a");
        const B: MetricId = MetricId::new("test.b");
        let mut rates = Rates::default();
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_secs(1);

        assert_eq!(rates.update(&A, 100.0, t0), None);
        assert_eq!(rates.update(&B, 5000.0, t0), None);
        assert_eq!(rates.update(&A, 150.0, t1), Some(50.0));
        assert_eq!(rates.update(&B, 5000.0, t1), Some(0.0));
    }

    #[test]
    fn tracks_totals_per_metric() {
        const BYTES: MetricId = MetricId::new("test.bytes");
        let mut rates = Rates::default();
        let t0 = Instant::now();

        assert_eq!(rates.update(&BYTES, 1000.0, t0), None);
        assert_eq!(
            rates.update(&BYTES, 3000.0, t0 + Duration::from_secs(2)),
            Some(1000.0)
        );
        // A reset yields no rate, then rates resume from the new total.
        assert_eq!(
            rates.update(&BYTES, 10.0, t0 + Duration::from_secs(3)),
            None
        );
        assert_eq!(
            rates.update(&BYTES, 20.0, t0 + Duration::from_secs(4)),
            Some(10.0)
        );
    }
}
