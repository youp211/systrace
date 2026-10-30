//! Central metric store.

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use crate::metric::{Kind, MetricDesc, MetricId};
use crate::util::rate;

#[derive(Debug, Default)]
pub struct Store {
    descs: BTreeMap<MetricId, MetricDesc>,
    latest: BTreeMap<MetricId, f64>,
    counters: HashMap<MetricId, (f64, Instant)>,
}

impl Store {
    pub fn register(&mut self, desc: MetricDesc) {
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
                self.latest.insert(id, value);
            }
            Kind::Counter => {
                let prev = self.counters.insert(id.clone(), (value, now));
                match prev.and_then(|(v, t)| rate::per_sec(v, value, now - t)) {
                    Some(r) => self.latest.insert(id, r),
                    None => self.latest.remove(&id),
                };
            }
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::Unit;
    use std::time::Duration;

    const BYTES: MetricId = MetricId::new("test.bytes");

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
}
