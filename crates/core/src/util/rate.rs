//! Counter to rate conversion.

use std::time::Duration;

/// Per-second rate between two counter totals; `None` on reset or zero interval.
pub fn per_sec(prev: f64, cur: f64, dt: Duration) -> Option<f64> {
    let secs = dt.as_secs_f64();
    (cur >= prev && secs > 0.0).then(|| (cur - prev) / secs)
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
}
