//! Module host: probes, registers and samples modules into the store.

use std::time::Instant;

use crate::display::Panel;
use crate::metric::{Availability, Capabilities, MetricId};
use crate::module::{Ctx, Header, Module, Sink};
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
                let header = module.header();
                // A module only speaks for metrics under its own id, and the first
                // description of an id wins. tests/conformance.rs flags violations.
                let caps = module.probe(&ctx).retain(|id| header.owns(id.as_str()));
                for desc in module.metrics() {
                    if header.owns(desc.id.as_str()) && store.desc(&desc.id).is_none() {
                        store.register(desc);
                    }
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

    /// Samples every active module once. Samples for metrics the module can't
    /// use, doesn't own or never declared are dropped.
    pub fn tick(&mut self) {
        self.ctx.now = Instant::now();
        for slot in self.slots.iter_mut().filter(|s| s.active()) {
            slot.module.sample(&self.ctx, &mut self.sink);
            for (id, value) in self.sink.drain() {
                if slot.usable(&id) {
                    self.store.ingest(id, value);
                }
            }
        }
        self.store.end_tick();
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn capabilities(&self) -> impl Iterator<Item = (&'static Header, &Capabilities)> {
        self.slots.iter().map(|s| (s.module.header(), &s.caps))
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
    use crate::display::Widget;
    use crate::metric::{MetricDesc, Unit};

    const M: MetricId = MetricId::new("test.value");
    const OTHER: MetricId = MetricId::new("test.other");

    struct Fake {
        status: Availability,
        sampled: Arc<AtomicBool>,
    }

    impl Module for Fake {
        fn header(&self) -> &'static Header {
            &Header {
                id: "test",
                name: "Test",
                reads: "nothing; stand-in",
            }
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
        assert!(matches!(
            host.availability(&M),
            Some(Availability::Unavailable { .. })
        ));
    }

    /// Module with an id, the caps it claims, the descs it declares and what it pushes.
    struct Scripted {
        header: &'static Header,
        caps: Vec<(MetricId, Availability)>,
        descs: Vec<MetricDesc>,
        pushes: Vec<(MetricId, f64)>,
        panels: Vec<Panel>,
    }

    impl Module for Scripted {
        fn header(&self) -> &'static Header {
            self.header
        }

        fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
            self.caps
                .iter()
                .fold(Capabilities::default(), |c, (id, s)| {
                    c.set([id.clone()], s.clone())
                })
        }

        fn metrics(&self) -> Vec<MetricDesc> {
            self.descs.clone()
        }

        fn sample(&mut self, _ctx: &Ctx, out: &mut Sink) {
            for (id, v) in &self.pushes {
                out.push(id.clone(), *v);
            }
        }

        fn panels(&self) -> Vec<Panel> {
            self.panels.clone()
        }
    }

    const TEST: Header = Header {
        id: "test",
        name: "Test",
        reads: "nothing",
    };
    const INTRUDER: Header = Header {
        id: "intruder",
        name: "Intruder",
        reads: "nothing",
    };

    #[test]
    fn keeps_only_usable_declared_finite_samples() {
        let module = Scripted {
            header: &TEST,
            caps: vec![
                (M, Availability::Available),
                (OTHER, Availability::unsupported("no")),
                (MetricId::new("test.undeclared"), Availability::Available),
            ],
            descs: vec![
                MetricDesc::gauge(M, Unit::Number),
                MetricDesc::gauge(OTHER, Unit::Number),
            ],
            pushes: vec![
                (M, 1.0),
                (OTHER, 2.0),
                (MetricId::new("test.undeclared"), 3.0),
                (MetricId::new("elsewhere.x"), 4.0),
            ],
            panels: vec![],
        };
        let mut host = Host::new(vec![Box::new(module)]);
        host.tick();
        let stored: Vec<_> = host
            .store()
            .latest_all()
            .map(|(d, v)| (d.id.to_string(), v))
            .collect();
        assert_eq!(stored, [("test.value".to_string(), 1.0)]);
    }

    #[test]
    fn non_finite_samples_become_gaps() {
        let module = Scripted {
            header: &TEST,
            caps: vec![(M, Availability::Available)],
            descs: vec![MetricDesc::gauge(M, Unit::Number)],
            pushes: vec![(M, f64::NAN)],
            panels: vec![],
        };
        let mut host = Host::new(vec![Box::new(module)]);
        host.tick();
        assert_eq!(host.store().latest(&M), None);
        assert_eq!(host.store().history(&M).collect::<Vec<_>>(), [None]);
    }

    #[test]
    fn modules_cannot_claim_another_modules_metrics() {
        let owner = Scripted {
            header: &TEST,
            caps: vec![(M, Availability::Available)],
            descs: vec![
                MetricDesc::gauge(M, Unit::Bytes),
                MetricDesc::gauge(M, Unit::Percent),
            ],
            pushes: vec![(M, 1.0)],
            panels: vec![],
        };
        let intruder = Scripted {
            header: &INTRUDER,
            caps: vec![(M, Availability::Available)],
            descs: vec![MetricDesc::gauge(M, Unit::Percent)],
            pushes: vec![(M, 999.0)],
            panels: vec![],
        };
        let mut host = Host::new(vec![Box::new(owner), Box::new(intruder)]);
        host.tick();
        assert_eq!(host.store().desc(&M).map(|d| d.unit), Some(Unit::Bytes));
        assert_eq!(host.store().latest(&M), Some(1.0));
        let intruder_caps = host.capabilities().find(|(h, _)| h.id == "intruder");
        assert_eq!(intruder_caps.map(|(_, c)| c.iter().count()), Some(0));
    }

    #[test]
    fn panels_need_every_widget_usable() {
        let module = Scripted {
            header: &TEST,
            caps: vec![
                (M, Availability::Available),
                (OTHER, Availability::unsupported("no")),
            ],
            descs: vec![
                MetricDesc::gauge(M, Unit::Number),
                MetricDesc::gauge(OTHER, Unit::Number),
            ],
            pushes: vec![],
            panels: vec![
                Panel::new("ok", "Ok").widget(Widget::series("m", M)),
                Panel::new("mixed", "Mixed")
                    .widget(Widget::series("m", M))
                    .widget(Widget::series("o", OTHER)),
                Panel::new("empty", "Empty"),
            ],
        };
        let host = Host::new(vec![Box::new(module)]);
        let ids: Vec<_> = host.panels().iter().map(|p| p.id).collect();
        assert_eq!(ids, ["ok"]);
    }
}
