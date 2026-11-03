//! Every compiled-in module follows the plugin contract, so modules stay plug and play.

use std::collections::HashSet;

use systrace::module::{Ctx, Module};

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
