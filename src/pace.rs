//! Sampling cadence shared by the interactive front ends.

use std::time::{Duration, Instant};

/// Fixed-rate schedule: ticks land on a steady grid while the caller keeps up,
/// and after a stall the next tick is one full interval out, never a burst.
#[derive(Clone, Copy, Debug)]
pub struct Pace {
    interval: Duration,
    next: Instant,
}

impl Pace {
    /// First tick due one interval after `now`.
    pub fn new(interval: Duration, now: Instant) -> Self {
        Self {
            interval,
            next: later(now, interval),
        }
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    pub fn due(&self, now: Instant) -> bool {
        now >= self.next
    }

    /// How long to wait before the next tick is due.
    pub fn wait(&self, now: Instant) -> Duration {
        self.next.saturating_duration_since(now)
    }

    /// Call after ticking at `now`.
    pub fn advance(&mut self, now: Instant) {
        let on_grid = later(self.next, self.interval);
        self.next = if on_grid > now {
            on_grid
        } else {
            later(now, self.interval)
        };
    }
}

/// `t + d`, or `t` if that would overflow `Instant`.
fn later(t: Instant, d: Duration) -> Instant {
    t.checked_add(d).unwrap_or(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: Duration = Duration::from_secs(1);

    #[test]
    fn keeps_a_steady_grid() {
        let t0 = Instant::now();
        let mut pace = Pace::new(SEC, t0);
        assert!(!pace.due(t0));
        assert_eq!(pace.wait(t0), SEC);
        // Ticking a little late doesn't shift the grid.
        let late = t0 + Duration::from_millis(1050);
        assert!(pace.due(late));
        pace.advance(late);
        assert_eq!(pace.wait(late), Duration::from_millis(950));
    }

    #[test]
    fn a_stall_gives_one_tick_not_a_burst() {
        let t0 = Instant::now();
        let mut pace = Pace::new(SEC, t0);
        let stalled = t0 + Duration::from_secs(5);
        pace.advance(stalled);
        assert!(!pace.due(stalled));
        assert_eq!(pace.wait(stalled), SEC);
    }

    #[test]
    fn huge_intervals_do_not_overflow() {
        let t0 = Instant::now();
        let mut pace = Pace::new(Duration::MAX, t0);
        pace.advance(t0);
        assert_eq!(pace.interval(), Duration::MAX);
    }
}
