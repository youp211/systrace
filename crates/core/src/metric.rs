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
impl Ord for MetricId {
    fn cmp(&self, other: &Self) -> Ordering {
        chunks(self.as_str())
            .cmp(&chunks(other.as_str()))
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

/// Splits into digit and non-digit runs.
fn chunks(s: &str) -> Vec<Chunk<'_>> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(first) = rest.chars().next() {
        let digit = first.is_ascii_digit();
        let end = rest
            .find(|c: char| c.is_ascii_digit() != digit)
            .unwrap_or(rest.len());
        let (run, tail) = rest.split_at(end);
        out.push(match run.parse() {
            Ok(n) if digit => Chunk::Num(n),
            _ => Chunk::Text(run),
        });
        rest = tail;
    }
    out
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
    Count,
    Number,
}

/// Gauges are stored as-is; counters are pushed as totals and stored as per-second rates.
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
    Degraded { reason: String },
    Unavailable { reason: String, fix: Option<String> },
}

impl Availability {
    pub fn degraded(reason: impl Into<String>) -> Self {
        Self::Degraded {
            reason: reason.into(),
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
}
