//! Central metric store. Values are stored exactly as modules push them.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use crate::metric::{MetricDesc, MetricId};

/// Ticks of history kept per metric.
pub const HISTORY: usize = 300;

#[derive(Debug, Default)]
pub struct Store {
    descs: BTreeMap<MetricId, MetricDesc>,
    /// Values from the most recent tick only.
    latest: BTreeMap<MetricId, f64>,
    /// One entry per tick, oldest first; `None` marks a gap.
    history: HashMap<MetricId, VecDeque<Option<f64>>>,
    fresh: HashSet<MetricId>,
}

impl Store {
    pub fn register(&mut self, desc: MetricDesc) {
        self.history.entry(desc.id.clone()).or_default();
        self.descs.insert(desc.id.clone(), desc);
    }

    /// Records one sample for the current tick. Unregistered ids are ignored,
    /// and a non-finite value leaves a gap, so output never shows NaN or inf.
    pub fn ingest(&mut self, id: MetricId, value: f64) {
        if !value.is_finite() || !self.descs.contains_key(&id) {
            return;
        }
        self.latest.insert(id.clone(), value);
        self.fresh.insert(id);
    }

    /// Closes a tick: each metric's fresh value, or a gap, goes into its history.
    /// A metric with no sample this tick has no latest value, so nothing shows stale.
    pub fn end_tick(&mut self) {
        for (id, ring) in &mut self.history {
            let value = if self.fresh.contains(id) {
                self.latest.get(id).copied()
            } else {
                self.latest.remove(id);
                None
            };
            if ring.len() == HISTORY {
                ring.pop_front();
            }
            ring.push_back(value);
        }
        self.fresh.clear();
    }

    pub fn desc(&self, id: &MetricId) -> Option<&MetricDesc> {
        self.descs.get(id)
    }

    pub fn descs(&self) -> impl Iterator<Item = &MetricDesc> {
        self.descs.values()
    }

    pub fn latest(&self, id: &MetricId) -> Option<f64> {
        self.latest.get(id).copied()
    }

    pub fn latest_all(&self) -> impl Iterator<Item = (&MetricDesc, f64)> {
        self.latest
            .iter()
            .filter_map(|(id, v)| Some((self.descs.get(id)?, *v)))
    }

    /// Per-tick values, oldest first; `None` where no sample arrived.
    pub fn history(&self, id: &MetricId) -> impl Iterator<Item = Option<f64>> + '_ {
        self.history.get(id).into_iter().flatten().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::Unit;

    const BYTES: MetricId = MetricId::new("test.bytes");
    const LEVEL: MetricId = MetricId::new("test.level");

    #[test]
    fn stores_values_as_pushed() {
        let mut store = Store::default();
        store.register(MetricDesc::counter(BYTES, Unit::BytesPerSec));
        store.ingest(BYTES, 1000.0);
        store.end_tick();
        assert_eq!(store.latest(&BYTES), Some(1000.0));
    }

    #[test]
    fn ignores_non_finite_and_unregistered_values() {
        let mut store = Store::default();
        store.register(MetricDesc::gauge(LEVEL, Unit::Number));
        for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            store.ingest(LEVEL, v);
            store.end_tick();
            assert_eq!(store.latest(&LEVEL), None);
        }
        store.ingest(BYTES, 1.0);
        store.end_tick();
        assert_eq!(store.latest(&BYTES), None);
        assert!(store.history(&LEVEL).all(|v| v.is_none()));
    }

    #[test]
    fn history_records_gaps_and_clears_stale_values() {
        let mut store = Store::default();
        store.register(MetricDesc::gauge(LEVEL, Unit::Number));

        store.ingest(LEVEL, 1.0);
        store.end_tick();
        store.end_tick();
        assert_eq!(store.latest(&LEVEL), None);
        store.ingest(LEVEL, 3.0);
        store.end_tick();
        let seen: Vec<_> = store.history(&LEVEL).collect();
        assert_eq!(seen, [Some(1.0), None, Some(3.0)]);

        for _ in 0..HISTORY {
            store.end_tick();
        }
        assert_eq!(store.history(&LEVEL).count(), HISTORY);
        assert!(store.history(&LEVEL).all(|v| v.is_none()));
    }
}
