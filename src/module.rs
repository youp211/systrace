//! The module contract.

use std::time::Instant;

use serde::Serialize;

use crate::display::Panel;
use crate::metric::{Capabilities, MetricDesc, MetricId};
use crate::util::select;

/// Per-tick context. Target selection lands here in Phase 2.
#[derive(Debug)]
pub struct Ctx {
    /// When this tick started; the same for every module, so rates line up.
    pub now: Instant,
}

impl Default for Ctx {
    fn default() -> Self {
        Self {
            now: Instant::now(),
        }
    }
}

/// Samples pushed by a module during one tick.
#[derive(Debug, Default)]
pub struct Sink {
    samples: Vec<(MetricId, f64)>,
}

impl Sink {
    /// Records one value in the metric's declared unit. Non-finite values
    /// (NaN, ±inf) are stored as a gap.
    pub fn push(&mut self, id: MetricId, value: f64) {
        self.samples.push((id, value));
    }

    /// What has been pushed since the host last collected, for tests and tools.
    pub fn samples(&self) -> &[(MetricId, f64)] {
        &self.samples
    }

    pub(crate) fn drain(&mut self) -> std::vec::Drain<'_, (MetricId, f64)> {
        self.samples.drain(..)
    }
}

/// Standard header every module declares once, as `const HEADER` at the top of its file.
///
/// The module converts what it reads into the units its `MetricDesc`s declare
/// (bytes, Hz, °C, percent, per-second rates via `util::rate::Rates`) before pushing.
/// Core stores pushed values exactly as they arrive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Header {
    /// Stable id; the first segment of every metric the module owns.
    pub id: &'static str,
    /// Human-readable name.
    pub name: &'static str,
    /// What the module reads from the OS.
    pub reads: &'static str,
}

impl Header {
    /// True when `metric` belongs to this module: `cpu` owns `cpu` and `cpu.*`.
    pub fn owns(&self, metric: &str) -> bool {
        select::in_subtree(self.id, metric)
    }
}

pub trait Module: Send {
    fn header(&self) -> &'static Header;

    fn probe(&mut self, ctx: &Ctx) -> Capabilities;

    fn metrics(&self) -> Vec<MetricDesc> {
        vec![]
    }

    fn sample(&mut self, _ctx: &Ctx, _out: &mut Sink) {}

    fn panels(&self) -> Vec<Panel> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_owns_its_prefix() {
        let h = Header {
            id: "cpu",
            name: "CPU",
            reads: "sysinfo",
        };
        assert!(h.owns("cpu"));
        assert!(h.owns("cpu.core.0.util"));
        assert!(!h.owns("cpux.util"));
        assert!(!h.owns("memory.used"));
    }
}
