//! Central metric store.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::time::Instant;

use crate::metric::{Kind, MetricDesc, MetricId};
use crate::util::rate;

/// Ticks of history kept per metric.
pub const HISTORY: usize = 300;

#[derive(Debug, Default)]
pub struct Store {
    descs: BTreeMap<MetricId, MetricDesc>,
    latest: BTreeMap<MetricId, f64>,
    counters: HashMap<MetricId, (f64, Instant)>,
    /// One entry per tick, oldest first; `None` marks a gap.
    history: HashMap<MetricId, VecDeque<Option<f64>>>,
    fresh: HashSet<MetricId>,
}

impl Store {
    pub fn register(&mut self, desc: MetricDesc) {
        self.history.entry(desc.id.clone()).or_default();
        self.descs.insert(desc.id.clone(), desc);
    }

    /// Records one sample; counters become rates once two totals exist.
    pub fn ingest(&mut self, id: MetricId, value: f64, now: Instant) {
        let Some(desc) = self.descs.get(&id) else {
            debug_assert!(false, "sample for unregistered metric {id}");
            return;
        };
        match desc.kind {
            Kind::Gauge => {
                self.latest.insert(id.clone(), value);
            }
            Kind::Counter => {
                let prev = self.counters.insert(id.clone(), (value, now));
                match prev.and_then(|(v, t)| rate::per_sec(v, value, now - t)) {
                    Some(r) => self.latest.insert(id.clone(), r),
                    None => self.latest.remove(&id),
                };
            }
        }
        self.fresh.insert(id);
    }

    /// Closes a tick: appends each metric's fresh value, or a gap, to its history.
    pub fn end_tick(&mut self) {
        for (id, ring) in &mut self.history {
            let value = self
                .fresh
                .contains(id)
                .then(|| self.latest.get(id).copied())
                .flatten();
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
    use std::time::Duration;

    const BYTES: MetricId = MetricId::new("test.bytes");
    const LEVEL: MetricId = MetricId::new("test.level");

    #[test]
    fn counter_becomes_rate() {
        let mut store = Store::default();
        store.register(MetricDesc::counter(BYTES, Unit::BytesPerSec));
        let t0 = Instant::now();

        store.ingest(BYTES, 1000.0, t0);
        assert_eq!(store.latest(&BYTES), None);

        store.ingest(BYTES, 3000.0, t0 + Duration::from_secs(2));
        assert_eq!(store.latest(&BYTES), Some(1000.0));

        store.ingest(BYTES, 10.0, t0 + Duration::from_secs(3));
        assert_eq!(store.latest(&BYTES), None);
    }

    #[test]
    fn history_records_gaps() {
        let mut store = Store::default();
        store.register(MetricDesc::gauge(LEVEL, Unit::Number));
        let now = Instant::now();

        store.ingest(LEVEL, 1.0, now);
        store.end_tick();
        store.end_tick();
        store.ingest(LEVEL, 3.0, now);
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
