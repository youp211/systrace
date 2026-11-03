//! Every compiled-in module follows the plugin contract, so modules stay plug and play.

use std::collections::{HashMap, HashSet};
use std::thread;
use std::time::Duration;

use systrace::metric::{MetricId, Unit};
use systrace::module::{Ctx, Module, Sink};

fn probed() -> Vec<Box<dyn Module>> {
    let ctx = Ctx::default();
    let mut modules = systrace::modules::all();
    for m in &mut modules {
        m.probe(&ctx);
    }
    modules
}

#[test]
fn headers_are_complete_and_unique() {
    let mut ids = HashSet::new();
    for m in probed() {
        let h = m.header();
        assert!(ids.insert(h.id), "duplicate module id {}", h.id);
        assert!(
            !h.name.is_empty() && !h.reads.is_empty(),
            "{} header is incomplete",
            h.id
        );
        assert!(
            h.id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{} is not a lowercase id",
            h.id
        );
    }
}

#[test]
fn modules_only_declare_their_own_metrics() {
    let ctx = Ctx::default();
    for mut m in systrace::modules::all() {
        let h = m.header();
        let caps = m.probe(&ctx);
        for (id, _) in caps.iter() {
            assert!(h.owns(id.as_str()), "{} reports capability {id}", h.id);
        }
        for desc in m.metrics() {
            assert!(h.owns(desc.id.as_str()), "{} declares {}", h.id, desc.id);
            assert!(
                caps.get(&desc.id).is_some(),
                "{} declares {} without a capability",
                h.id,
                desc.id
            );
        }
    }
}

#[test]
fn panels_reference_declared_metrics() {
    for m in probed() {
        let declared: HashSet<_> = m.metrics().into_iter().map(|d| d.id).collect();
        for panel in m.panels() {
            for widget in &panel.widgets {
                assert!(
                    declared.contains(widget.metric()),
                    "{} panel {} shows undeclared {}",
                    m.header().id,
                    panel.id,
                    widget.metric()
                );
            }
        }
    }
}

/// `module.subject.metric`: lowercase segments of `[a-z0-9_]` joined by dots.
fn well_formed(id: &str) -> bool {
    !id.is_empty()
        && id.split('.').all(|seg| {
            !seg.is_empty()
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
}

#[test]
fn metric_ids_are_well_formed_and_unique() {
    for m in probed() {
        let mut seen = HashSet::new();
        for desc in m.metrics() {
            let id = desc.id.as_str().to_string();
            assert!(
                well_formed(&id),
                "{} declares malformed id {id:?}",
                m.header().id
            );
            assert!(
                seen.insert(id.clone()),
                "{} declares {id} twice",
                m.header().id
            );
        }
    }
}

#[test]
fn usable_capabilities_have_metrics() {
    let ctx = Ctx::default();
    for mut m in systrace::modules::all() {
        let caps = m.probe(&ctx);
        let declared: HashSet<MetricId> = m.metrics().into_iter().map(|d| d.id).collect();
        for (id, status) in caps.iter() {
            assert!(
                !status.is_usable() || declared.contains(id),
                "{} reports {id} usable but never declares it",
                m.header().id
            );
        }
    }
}

/// Two real samples per module, past sysinfo's CPU window: everything pushed is
/// declared, owned, finite and, for percentages, between 0 and 100.
#[test]
fn samples_are_declared_finite_and_in_range() {
    let mut modules = probed();
    let units: Vec<HashMap<MetricId, Unit>> = modules
        .iter()
        .map(|m| m.metrics().into_iter().map(|d| (d.id, d.unit)).collect())
        .collect();
    for _ in 0..2 {
        thread::sleep(Duration::from_millis(250));
        let ctx = Ctx::default();
        for (m, units) in modules.iter_mut().zip(&units) {
            let mut sink = Sink::default();
            m.sample(&ctx, &mut sink);
            let h = m.header();
            for (id, v) in sink.samples() {
                let unit = units.get(id);
                assert!(unit.is_some(), "{} pushed undeclared {id}", h.id);
                assert!(h.owns(id.as_str()), "{} pushed {id}", h.id);
                assert!(v.is_finite(), "{} pushed {v} for {id}", h.id);
                if unit == Some(&Unit::Percent) {
                    assert!((0.0..=100.0 + 1e-6).contains(v), "{id} = {v}%");
                }
            }
        }
    }
}
