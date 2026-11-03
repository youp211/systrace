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

    /// False when probe found nothing usable, e.g. a missing runtime library.
    fn active(&self) -> bool {
        self.caps.iter().any(|(_, s)| s.is_usable())
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

    /// Samples every active module once; samples for unusable metrics are dropped.
    pub fn tick(&mut self) {
        let now = Instant::now();
        for slot in self.slots.iter_mut().filter(|s| s.active()) {
            slot.module.sample(&self.ctx, &mut self.sink);
            for (id, value) in self.sink.drain() {
                if slot.usable(&id) {
                    self.store.ingest(id, value, now);
                }
            }
        }
        self.store.end_tick();
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

    /// Non-empty panels whose metrics are all usable.
    pub fn panels(&self) -> Vec<Panel> {
        self.slots
            .iter()
            .flat_map(|s| {
                s.module.panels().into_iter().filter(|p| {
                    !p.widgets.is_empty() && p.widgets.iter().all(|w| s.usable(w.metric()))
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::metric::{MetricDesc, Unit};

    const M: MetricId = MetricId::new("test.value");

    struct Fake {
        status: Availability,
        sampled: Arc<AtomicBool>,
    }

    impl Module for Fake {
        fn id(&self) -> &'static str {
            "test"
        }

        fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
            Capabilities::all([M], self.status.clone())
        }

        fn metrics(&self) -> Vec<MetricDesc> {
            vec![MetricDesc::gauge(M, Unit::Number)]
        }

        fn sample(&mut self, _ctx: &Ctx, out: &mut Sink) {
            self.sampled.store(true, Ordering::Relaxed);
            out.push(M, 1.0);
        }
    }

    /// Host with one fake module, ticked once; returns whether it was sampled.
    fn run(status: Availability) -> (Host, bool) {
        let sampled = Arc::new(AtomicBool::new(false));
        let mut host = Host::new(vec![Box::new(Fake {
            status,
            sampled: sampled.clone(),
        })]);
        host.tick();
        (host, sampled.load(Ordering::Relaxed))
    }

    #[test]
    fn samples_usable_modules() {
        let (host, sampled) = run(Availability::Available);
        assert!(sampled);
        assert_eq!(host.store().latest(&M), Some(1.0));
    }

    #[test]
    fn skips_unusable_modules_but_reports_them() {
        let (host, sampled) = run(Availability::unsupported("missing library"));
        assert!(!sampled);
        assert_eq!(host.capabilities().count(), 1);
    }
}
