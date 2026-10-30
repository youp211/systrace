//! Module host: probes, registers and samples modules into the store.

use std::time::Instant;

use crate::display::Panel;
use crate::metric::{Availability, Capabilities, MetricId};
use crate::module::{Ctx, Module, Sink};
use crate::store::Store;

struct Slot {
    module: Box<dyn Module>,
    caps: Capabilities,
}

impl Slot {
    fn usable(&self, id: &MetricId) -> bool {
        self.caps.get(id).is_some_and(Availability::is_usable)
    }
}

pub struct Host {
    slots: Vec<Slot>,
    store: Store,
    ctx: Ctx,
    sink: Sink,
}

impl Host {
    pub fn new(modules: Vec<Box<dyn Module>>) -> Self {
        let ctx = Ctx::default();
        let mut store = Store::default();
        let slots = modules
            .into_iter()
            .map(|mut module| {
                let caps = module.probe(&ctx);
                for desc in module.metrics() {
                    store.register(desc);
                }
                Slot { module, caps }
            })
            .collect();
        Self {
            slots,
            store,
            ctx,
            sink: Sink::default(),
        }
    }

    /// Samples every module once; samples for unusable metrics are dropped.
    pub fn tick(&mut self) {
        let now = Instant::now();
        for slot in &mut self.slots {
            slot.module.sample(&self.ctx, &mut self.sink);
            for (id, value) in self.sink.drain() {
                if slot.usable(&id) {
                    self.store.ingest(id, value, now);
                }
            }
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn capabilities(&self) -> impl Iterator<Item = (&'static str, &Capabilities)> {
        self.slots.iter().map(|s| (s.module.id(), &s.caps))
    }

    /// Availability of a metric across all modules.
    pub fn availability(&self, id: &MetricId) -> Option<&Availability> {
        self.slots.iter().find_map(|s| s.caps.get(id))
    }

    /// Panels whose metrics are all usable.
    pub fn panels(&self) -> Vec<Panel> {
        self.slots
            .iter()
            .flat_map(|s| {
                s.module
                    .panels()
                    .into_iter()
                    .filter(|p| p.widgets.iter().all(|w| s.usable(w.metric())))
            })
            .collect()
    }
}
