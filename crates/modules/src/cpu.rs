//! CPU utilization, frequency and load average via sysinfo.
//! Read-only.

use sysinfo::System;
use sysmon_core::prelude::*;

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
}

/// Per-core (util, freq) metric ids.
fn core_ids(cores: usize) -> impl Iterator<Item = (MetricId, MetricId)> {
    (0..cores).map(|i| {
        (
            MetricId::owned(format!("cpu.core.{i}.util")),
            MetricId::owned(format!("cpu.core.{i}.freq")),
        )
    })
}

impl Module for Cpu {
    fn id(&self) -> &'static str {
        "cpu"
    }

    fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
        let mut sys = match System::new() {
            Ok(sys) => sys,
            Err(e) => {
                return Capabilities::all(
                    [UTIL],
                    Availability::unsupported(format!("cannot read CPU stats: {e}")),
                );
            }
        };
        sys.refresh_cpu_all();
        self.cores = sys.cpus().len();
        self.sys = Some(sys);

        let load = match System::load_average() {
            Ok(_) => Availability::Available,
            Err(e) => Availability::unsupported(format!("load average: {e}")),
        };
        let cores = core_ids(self.cores).flat_map(|(u, f)| [u, f]);
        Capabilities::all(std::iter::once(UTIL).chain(cores), Availability::Available)
            .set(LOAD, load)
    }

    fn metrics(&self) -> Vec<MetricDesc> {
        let mut out = vec![MetricDesc::gauge(UTIL, Unit::Percent).detect(Detect::Spike)];
        for (util, freq) in core_ids(self.cores) {
            out.push(MetricDesc::gauge(util, Unit::Percent));
            out.push(MetricDesc::gauge(freq, Unit::Hertz));
        }
        out.extend(LOAD.map(|id| MetricDesc::gauge(id, Unit::Number)));
        out
    }

    fn sample(&mut self, _ctx: &Ctx, out: &mut Sink) {
        let Some(sys) = &mut self.sys else { return };
        sys.refresh_cpu_all();
        out.push(UTIL, sys.global_cpu_usage().into());

        for ((util, freq), cpu) in core_ids(self.cores).zip(sys.cpus()) {
            out.push(util, cpu.usage().into());
            out.push(freq, cpu.frequency() as f64 * 1e6);
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
        let panel = core_ids(self.cores)
            .enumerate()
            .fold(panel, |p, (i, (util, _))| {
                p.widget(Widget::gauge(format!("core {i}"), util))
            });
        vec![panel]
    }
}
