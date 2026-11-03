//! CPU utilization, per-core frequency and load average via sysinfo.
//! Read-only.

use std::time::Instant;

use sysinfo::{MINIMUM_CPU_UPDATE_INTERVAL, System};

use crate::prelude::*;

pub const HEADER: Header = Header {
    id: "cpu",
    name: "CPU",
    reads: "sysinfo: utilization, load average; per-core frequency except on macOS",
};

const UTIL: MetricId = MetricId::new("cpu.util");
const LOAD: [MetricId; 3] = [
    MetricId::new("cpu.load.1"),
    MetricId::new("cpu.load.5"),
    MetricId::new("cpu.load.15"),
];

#[derive(Default)]
pub struct Cpu {
    sys: Option<System>,
    cores: usize,
    /// False where sysinfo only reports a fixed nominal frequency.
    freq: bool,
    /// When sysinfo last recomputed usage. Asked again sooner than
    /// `MINIMUM_CPU_UPDATE_INTERVAL`, it returns the previous values.
    refreshed: Option<Instant>,
}

/// Per-core util metric ids.
fn util_ids(cores: usize) -> impl Iterator<Item = MetricId> {
    (0..cores).map(|i| MetricId::owned(format!("cpu.core.{i}.util")))
}

/// Per-core freq metric ids.
fn freq_ids(cores: usize) -> impl Iterator<Item = MetricId> {
    (0..cores).map(|i| MetricId::owned(format!("cpu.core.{i}.freq")))
}

impl Module for Cpu {
    fn header(&self) -> &'static Header {
        &HEADER
    }

    fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
        let mut sys = match System::new() {
            Ok(sys) => sys,
            Err(e) => {
                let ids = self.metrics().into_iter().map(|d| d.id);
                return Capabilities::all(
                    ids,
                    Availability::unsupported(format!("cannot read CPU stats: {e}")),
                );
            }
        };
        sys.refresh_cpu_all();
        self.refreshed = Some(Instant::now());
        self.cores = sys.cpus().len();
        self.sys = Some(sys);
        // macOS returns the same nominal value for every core; live clocks need IOReport.
        self.freq = !cfg!(target_os = "macos");

        let freq = if self.freq {
            Availability::Available
        } else {
            Availability::unsupported("macOS reports only a fixed nominal frequency")
        };
        let load = match System::load_average() {
            Ok(_) => Availability::Available,
            Err(e) => Availability::unsupported(format!("load average: {e}")),
        };
        let util = std::iter::once(UTIL).chain(util_ids(self.cores));
        Capabilities::all(util, Availability::Available)
            .set(freq_ids(self.cores), freq)
            .set(LOAD, load)
    }

    fn metrics(&self) -> Vec<MetricDesc> {
        let mut out = vec![MetricDesc::gauge(UTIL, Unit::Percent).detect(Detect::Spike)];
        out.extend(util_ids(self.cores).map(|id| MetricDesc::gauge(id, Unit::Percent)));
        out.extend(freq_ids(self.cores).map(|id| MetricDesc::gauge(id, Unit::Hertz)));
        out.extend(LOAD.map(|id| MetricDesc::gauge(id, Unit::Number)));
        out
    }

    fn sample(&mut self, _ctx: &Ctx, out: &mut Sink) {
        let Some(sys) = &mut self.sys else { return };

        // Too soon after the last refresh, a "new" reading would be the old one again
        // (or, on the first tick, the average since boot), so leave a gap instead.
        let fresh = self
            .refreshed
            .is_none_or(|t| t.elapsed() >= MINIMUM_CPU_UPDATE_INTERVAL);
        if fresh {
            sys.refresh_cpu_all();
            // Taken after the call, so it is never earlier than sysinfo's own timestamp.
            self.refreshed = Some(Instant::now());
            out.push(UTIL, sys.global_cpu_usage().into());
            for (id, cpu) in util_ids(self.cores).zip(sys.cpus()) {
                out.push(id, cpu.usage().into());
            }
            if self.freq {
                for (id, cpu) in freq_ids(self.cores).zip(sys.cpus()) {
                    out.push(id, cpu.frequency() as f64 * 1e6);
                }
            }
        }

        if let Ok(load) = System::load_average() {
            for (id, v) in LOAD.into_iter().zip([load.one, load.five, load.fifteen]) {
                out.push(id, v);
            }
        }
    }

    fn panels(&self) -> Vec<Panel> {
        let panel = Panel::new("cpu", "CPU")
            .priority(90)
            .widget(Widget::series("total", UTIL));
        let panel = util_ids(self.cores).enumerate().fold(panel, |p, (i, id)| {
            p.widget(Widget::gauge(format!("core {i}"), id))
        });
        vec![panel]
    }
}
