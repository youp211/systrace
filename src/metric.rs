//! Metric identity, description and availability.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

/// Dotted metric name.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct MetricId(Cow<'static, str>);

impl MetricId {
    pub const fn new(id: &'static str) -> Self {
        Self(Cow::Borrowed(id))
    }

    pub fn owned(id: String) -> Self {
        Self(Cow::Owned(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Natural order, so `cpu.core.2` sorts before `cpu.core.10`.
/// Ties (`x.01` vs `x.1`) fall back to the raw string, so `Equal` means `==`.
impl Ord for MetricId {
    fn cmp(&self, other: &Self) -> Ordering {
        Chunks(self.as_str())
            .cmp(Chunks(other.as_str()))
            .then_with(|| self.0.cmp(&other.0))
    }
}

impl PartialOrd for MetricId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Chunk<'a> {
    Num(u64),
    Text(&'a str),
}

/// Digit and non-digit runs of an id, without allocating.
struct Chunks<'a>(&'a str);

impl<'a> Iterator for Chunks<'a> {
    type Item = Chunk<'a>;

    fn next(&mut self) -> Option<Chunk<'a>> {
        let digit = self.0.chars().next()?.is_ascii_digit();
        let end = self
            .0
            .find(|c: char| c.is_ascii_digit() != digit)
            .unwrap_or(self.0.len());
        let (run, rest) = self.0.split_at(end);
        self.0 = rest;
        // A digit run too long for u64 compares as text.
        Some(match run.parse() {
            Ok(n) if digit => Chunk::Num(n),
            _ => Chunk::Text(run),
        })
    }
}

impl fmt::Display for MetricId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Percent,
    Bytes,
    BytesPerSec,
    Hertz,
    Celsius,
    Count,
    /// Events per second, e.g. page faults or packets.
    PerSec,
    Number,
}

/// What a value means. Both are stored as pushed: a counter's module has already
/// turned the running total into a per-second rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Gauge,
    Counter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Detect {
    Spike,
    Shift,
}

#[derive(Clone, Debug, Serialize)]
pub struct MetricDesc {
    pub id: MetricId,
    pub unit: Unit,
    pub kind: Kind,
    pub detect: Option<Detect>,
}

impl MetricDesc {
    pub fn gauge(id: MetricId, unit: Unit) -> Self {
        Self {
            id,
            unit,
            kind: Kind::Gauge,
            detect: None,
        }
    }

    pub fn counter(id: MetricId, unit: Unit) -> Self {
        Self {
            id,
            unit,
            kind: Kind::Counter,
            detect: None,
        }
    }

    pub fn detect(mut self, detect: Detect) -> Self {
        self.detect = Some(detect);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Availability {
    Available,
    Degraded { reason: String, fix: Option<String> },
    Unavailable { reason: String, fix: Option<String> },
}

impl Availability {
    pub fn degraded(reason: impl Into<String>, fix: impl Into<String>) -> Self {
        Self::Degraded {
            reason: reason.into(),
            fix: Some(fix.into()),
        }
    }

    pub fn unavailable(reason: impl Into<String>, fix: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
            fix: Some(fix.into()),
        }
    }

    pub fn unsupported(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
            fix: None,
        }
    }

    pub fn is_usable(&self) -> bool {
        !matches!(self, Self::Unavailable { .. })
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Degraded { .. } => "degraded",
            Self::Unavailable { .. } => "unavailable",
        }
    }
}

/// Per-metric availability reported by a module's probe.
#[derive(Clone, Debug, Default)]
pub struct Capabilities(BTreeMap<MetricId, Availability>);

impl Capabilities {
    pub fn all(ids: impl IntoIterator<Item = MetricId>, status: Availability) -> Self {
        Self::default().set(ids, status)
    }

    pub fn set(mut self, ids: impl IntoIterator<Item = MetricId>, status: Availability) -> Self {
        for id in ids {
            self.0.insert(id, status.clone());
        }
        self
    }

    /// Keeps only the entries whose id passes `keep`.
    pub fn retain(mut self, mut keep: impl FnMut(&MetricId) -> bool) -> Self {
        self.0.retain(|id, _| keep(id));
        self
    }

    pub fn get(&self, id: &MetricId) -> Option<&Availability> {
        self.0.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&MetricId, &Availability)> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_naturally() {
        let mut ids = [
            "cpu.core.10.util",
            "cpu.load.15",
            "cpu.core.2.util",
            "cpu.load.5",
            "cpu.util",
        ]
        .map(MetricId::new);
        ids.sort();
        let names: Vec<_> = ids.iter().map(MetricId::as_str).collect();
        assert_eq!(
            names,
            [
                "cpu.core.2.util",
                "cpu.core.10.util",
                "cpu.load.5",
                "cpu.load.15",
                "cpu.util"
            ]
        );
    }

    #[test]
    fn order_agrees_with_equality() {
        let a = MetricId::new("x.01");
        let b = MetricId::new("x.1");
        assert_ne!(a, b);
        assert_ne!(a.cmp(&b), Ordering::Equal);
        let map: BTreeMap<_, _> = [(a, 1), (b, 2)].into_iter().collect();
        assert_eq!(map.len(), 2);

        let names = [
            "",
            "a",
            "a.1",
            "a.01",
            "a.10",
            "a.2",
            "a.b",
            "1",
            "01",
            "a1b2",
            "x.99999999999999999999999",
            "x.18446744073709551615",
            "ü.1",
            "a..1",
        ];
        for x in names.map(MetricId::new) {
            for y in names.map(MetricId::new) {
                assert_eq!(x.cmp(&y), y.cmp(&x).reverse(), "{x} vs {y}");
                assert_eq!(x.cmp(&y) == Ordering::Equal, x == y, "{x} vs {y}");
            }
        }
    }
}
