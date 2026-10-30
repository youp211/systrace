//! The module contract.

use crate::display::Panel;
use crate::metric::{Capabilities, MetricDesc, MetricId};

/// Per-tick context. Target selection lands here in Phase 2.
#[derive(Debug, Default)]
pub struct Ctx {}

/// Samples pushed by a module during one tick.
#[derive(Debug, Default)]
pub struct Sink {
    samples: Vec<(MetricId, f64)>,
}

impl Sink {
    pub fn push(&mut self, id: MetricId, value: f64) {
        self.samples.push((id, value));
    }

    pub(crate) fn drain(&mut self) -> std::vec::Drain<'_, (MetricId, f64)> {
        self.samples.drain(..)
    }
}

pub trait Module: Send {
    fn id(&self) -> &'static str;

    fn probe(&mut self, ctx: &Ctx) -> Capabilities;

    fn metrics(&self) -> Vec<MetricDesc> {
        vec![]
    }

    fn sample(&mut self, _ctx: &Ctx, _out: &mut Sink) {}

    fn panels(&self) -> Vec<Panel> {
        vec![]
    }
}
