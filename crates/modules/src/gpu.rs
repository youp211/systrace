//! GPU utilization and memory via sysinfo; vendor modules add depth later.
//! Read-only.

use sysinfo::Gpus;
use systrace_core::prelude::*;

/// Module-level entry for doctor when no GPU is found.
const GPU: MetricId = MetricId::new("gpu");

struct Device {
    util: MetricId,
    mem_used: MetricId,
    mem_total: MetricId,
}

#[derive(Default)]
pub struct Gpu {
    gpus: Option<Gpus>,
    devices: Vec<Device>,
}

impl Module for Gpu {
    fn id(&self) -> &'static str {
        "gpu"
    }

    fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
        let gpus = match Gpus::new_with_refreshed_list() {
            Ok(g) if !g.is_empty() => g,
            Ok(_) => return Capabilities::all([GPU], Availability::unsupported("no GPU found")),
            Err(e) => {
                return Capabilities::all(
                    [GPU],
                    Availability::unsupported(format!("cannot list GPUs: {e}")),
                );
            }
        };

        let mut caps = Capabilities::default();
        for (i, g) in gpus.iter().enumerate() {
            let id = |m: &str| MetricId::owned(format!("gpu.{i}.{m}"));
            let dev = Device {
                util: id("util"),
                mem_used: id("mem.used"),
                mem_total: id("mem.total"),
            };
            let util = match g.usage() {
                Some(_) => Availability::Available,
                None => Availability::unsupported("driver doesn't report utilization"),
            };
            // Apple Silicon shares system memory, so there is no separate VRAM figure.
            let mem = match g.used_memory().zip(g.total_memory()) {
                Some(_) => Availability::Available,
                None => Availability::unsupported("driver doesn't report dedicated memory"),
            };
            caps = caps
                .set([dev.util.clone()], util)
                .set([dev.mem_used.clone(), dev.mem_total.clone()], mem);
            self.devices.push(dev);
        }
        self.gpus = Some(gpus);
        caps
    }

    fn metrics(&self) -> Vec<MetricDesc> {
        self.devices
            .iter()
            .flat_map(|d| {
                [
                    MetricDesc::gauge(d.util.clone(), Unit::Percent).detect(Detect::Spike),
                    MetricDesc::gauge(d.mem_used.clone(), Unit::Bytes).detect(Detect::Shift),
                    MetricDesc::gauge(d.mem_total.clone(), Unit::Bytes),
                ]
            })
            .collect()
    }

    fn sample(&mut self, _ctx: &Ctx, out: &mut Sink) {
        let Some(gpus) = &mut self.gpus else { return };
        gpus.refresh(false);

        for (dev, g) in self.devices.iter().zip(gpus.iter()) {
            if let Some(u) = g.usage() {
                out.push(dev.util.clone(), u.into());
            }
            if let Some(m) = g.used_memory() {
                out.push(dev.mem_used.clone(), m as f64);
            }
            if let Some(m) = g.total_memory() {
                out.push(dev.mem_total.clone(), m as f64);
            }
        }
    }

    fn panels(&self) -> Vec<Panel> {
        self.devices
            .first()
            .map(|d| {
                Panel::new("gpu", "GPU")
                    .priority(85)
                    .widget(Widget::series("util", d.util.clone()))
            })
            .into_iter()
            .collect()
    }
}
