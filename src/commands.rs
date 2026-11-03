//! CLI commands: tui, gui, doctor, modules, metrics, snapshot.

use std::io::{self, Write};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use systrace::export::{self, Format, Record};
use systrace::fmt;
use systrace::host::Host;
use systrace::metric::{Availability, Unit};
use systrace::util::select;

pub fn doctor(problems: bool, format: Format) -> io::Result<()> {
    let host = host();
    let rows: Vec<DoctorRow> = host
        .capabilities()
        .flat_map(|(header, caps)| {
            caps.iter()
                .filter(move |(_, status)| !problems || **status != Availability::Available)
                .map(move |(id, status)| {
                    let (reason, fix) = match status {
                        Availability::Available => (None, None),
                        Availability::Degraded { reason, fix }
                        | Availability::Unavailable { reason, fix } => {
                            (Some(reason.clone()), fix.clone())
                        }
                    };
                    DoctorRow {
                        module: header.id,
                        metric: id.to_string(),
                        status: status.label(),
                        reason,
                        fix,
                    }
                })
        })
        .collect();
    print(&rows, format)
}

pub fn modules(format: Format) -> io::Result<()> {
    let host = host();
    let rows: Vec<ModuleRow> = host
        .capabilities()
        .map(|(header, caps)| {
            let count = |label| caps.iter().filter(|(_, s)| s.label() == label).count();
            ModuleRow {
                module: header.id,
                name: header.name,
                reads: header.reads,
                available: count("available"),
                degraded: count("degraded"),
                unavailable: count("unavailable"),
            }
        })
        .collect();
    print(&rows, format)
}

pub fn metrics(selectors: &[String], format: Format) -> io::Result<()> {
    let host = host();
    check_selectors(&host, selectors)?;
    let rows: Vec<MetricRow> = host
        .store()
        .descs()
        .filter(|d| select::matches_any(selectors, d.id.as_str()))
        .map(|d| MetricRow {
            metric: d.id.to_string(),
            unit: name(&d.unit),
            kind: name(&d.kind),
            detect: d.detect.as_ref().map(name),
            status: host
                .availability(&d.id)
                .map_or("unknown", Availability::label),
        })
        .collect();
    print(&rows, format)
}

pub fn snapshot(selectors: &[String], interval: Duration, format: Format) -> io::Result<()> {
    let mut host = host();
    check_selectors(&host, selectors)?;
    // Rates and CPU usage need two samples.
    host.tick();
    thread::sleep(interval);
    host.tick();

    let rows: Vec<SampleRow> = host
        .store()
        .latest_all()
        .filter(|(d, _)| select::matches_any(selectors, d.id.as_str()))
        .map(|(d, value)| SampleRow {
            metric: d.id.to_string(),
            value,
            unit: d.unit,
        })
        .collect();
    print(&rows, format)
}

#[cfg(feature = "tui")]
pub fn tui(interval: Duration) -> io::Result<()> {
    systrace::tui::run(host(), interval)
}

#[cfg(feature = "gui")]
pub fn gui(interval: Duration) -> io::Result<()> {
    systrace::gui::run(host(), interval)
}

/// A selector that matches no metric is a usage error (exit 2), not an empty result.
fn check_selectors(host: &Host, selectors: &[String]) -> io::Result<()> {
    let unmatched: Vec<&str> = selectors
        .iter()
        .filter(|s| {
            !host
                .store()
                .descs()
                .any(|d| select::matches(s, d.id.as_str()))
        })
        .map(String::as_str)
        .collect();
    if unmatched.is_empty() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "no metric matches {}; see `systrace metrics`",
                unmatched.join(", ")
            ),
        ))
    }
}

fn host() -> Host {
    Host::new(systrace::modules::all())
}

fn print<R: Record>(rows: &[R], format: Format) -> io::Result<()> {
    let mut out = io::stdout().lock();
    export::write(&mut out, rows, format)?;
    out.flush()
}

/// Serde name of a unit-like enum.
fn name<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

#[derive(Serialize)]
struct DoctorRow {
    module: &'static str,
    metric: String,
    status: &'static str,
    reason: Option<String>,
    fix: Option<String>,
}

impl Record for DoctorRow {
    fn header() -> &'static [&'static str] {
        &["module", "metric", "status", "reason", "fix"]
    }

    fn cells(&self) -> Vec<String> {
        vec![
            self.module.to_string(),
            self.metric.clone(),
            self.status.to_string(),
            self.reason.clone().unwrap_or_default(),
            self.fix.clone().unwrap_or_default(),
        ]
    }
}

#[derive(Serialize)]
struct ModuleRow {
    module: &'static str,
    name: &'static str,
    reads: &'static str,
    available: usize,
    degraded: usize,
    unavailable: usize,
}

impl Record for ModuleRow {
    fn header() -> &'static [&'static str] {
        &[
            "module",
            "name",
            "available",
            "degraded",
            "unavailable",
            "reads",
        ]
    }

    fn cells(&self) -> Vec<String> {
        vec![
            self.module.to_string(),
            self.name.to_string(),
            self.available.to_string(),
            self.degraded.to_string(),
            self.unavailable.to_string(),
            self.reads.to_string(),
        ]
    }
}

#[derive(Serialize)]
struct MetricRow {
    metric: String,
    unit: String,
    kind: String,
    detect: Option<String>,
    status: &'static str,
}

impl Record for MetricRow {
    fn header() -> &'static [&'static str] {
        &["metric", "unit", "kind", "detect", "status"]
    }

    fn cells(&self) -> Vec<String> {
        vec![
            self.metric.clone(),
            self.unit.clone(),
            self.kind.clone(),
            self.detect.clone().unwrap_or_default(),
            self.status.to_string(),
        ]
    }
}

#[derive(Serialize)]
struct SampleRow {
    metric: String,
    value: f64,
    unit: Unit,
}

impl Record for SampleRow {
    fn header() -> &'static [&'static str] {
        &["metric", "value", "unit"]
    }

    fn cells(&self) -> Vec<String> {
        vec![
            self.metric.clone(),
            self.value.to_string(),
            name(&self.unit),
        ]
    }

    fn table_header() -> &'static [&'static str] {
        &["metric", "value"]
    }

    fn table_cells(&self) -> Vec<String> {
        vec![self.metric.clone(), fmt::value(self.value, self.unit)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Header and cells must line up, or table and CSV columns shift.
    fn aligned<R: Record>(row: &R) -> bool {
        row.cells().len() == R::header().len() && row.table_cells().len() == R::table_header().len()
    }

    #[test]
    fn rows_match_their_headers() {
        assert!(aligned(&DoctorRow {
            module: "m",
            metric: "m.x".into(),
            status: "available",
            reason: None,
            fix: None,
        }));
        assert!(aligned(&ModuleRow {
            module: "m",
            name: "M",
            reads: "r",
            available: 1,
            degraded: 0,
            unavailable: 0,
        }));
        assert!(aligned(&MetricRow {
            metric: "m.x".into(),
            unit: "bytes".into(),
            kind: "gauge".into(),
            detect: None,
            status: "available",
        }));
        assert!(aligned(&SampleRow {
            metric: "m.x".into(),
            value: 1.0,
            unit: Unit::Bytes,
        }));
    }
}
