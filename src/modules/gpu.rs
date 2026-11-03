//! GPU utilization and memory via sysinfo, the cross-vendor baseline.
//! Read-only.

use sysinfo::{Gpus, PCI};

use crate::prelude::*;
use crate::util::id;

pub const HEADER: Header = Header {
    id: "gpu",
    name: "GPU",
    reads: "sysinfo: utilization, dedicated memory",
};

/// Module-level entry for doctor when no GPU is found.
const GPU: MetricId = MetricId::new("gpu");

struct Device {
    /// Bus address. Devices are found again by it, since list order can change.
    pci: PCI,
    name: String,
    util: MetricId,
    mem_used: MetricId,
    mem_total: MetricId,
    /// Whether utilization is reported, so the panel only shows working rows.
    util_ok: bool,
}

#[derive(Default)]
pub struct Gpu {
    gpus: Option<Gpus>,
    devices: Vec<Device>,
}

impl Module for Gpu {
    fn header(&self) -> &'static Header {
        &HEADER
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

        // Sorted by bus address and named after the model, so ids are stable across runs.
        let mut found: Vec<_> = gpus
            .iter()
            .map(|g| {
                let name = g.model().unwrap_or("GPU").to_string();
                let mem = g.used_memory().zip(g.total_memory()).is_some();
                (g.pci().clone(), name, g.usage().is_some(), mem)
            })
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0));
        let segments = id::segments(found.iter().map(|f| f.1.as_str()));

        let mut caps = Capabilities::default();
        for ((pci, name, usage, mem), seg) in found.into_iter().zip(segments) {
            let id = |m: &str| MetricId::owned(format!("gpu.{seg}.{m}"));
            // Windows reads utilization from a rate counter, empty until a second sample.
            let util_ok = usage || cfg!(windows);
            let util = if util_ok {
                Availability::Available
            } else {
                Availability::unsupported("driver doesn't report utilization")
            };
            // Apple Silicon shares system memory, so there is no separate VRAM figure.
            let mem = if mem {
                Availability::Available
            } else {
                Availability::unsupported("driver doesn't report dedicated memory")
            };
            let dev = Device {
                pci,
                name,
                util: id("util"),
                mem_used: id("mem.used"),
                mem_total: id("mem.total"),
                util_ok,
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
        // `true` drops GPUs that are gone, so they leave gaps rather than stale values.
        gpus.refresh(true);

        for dev in &self.devices {
            let Some(g) = gpus.iter().find(|g| g.pci() == &dev.pci) else {
                continue;
            };
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
        let rows: Vec<_> = self
            .devices
            .iter()
            .filter(|d| d.util_ok)
            .map(|d| Widget::series(d.name.clone(), d.util.clone()))
            .collect();
        if rows.is_empty() {
            return vec![];
        }
        vec![
            rows.into_iter()
                .fold(Panel::new("gpu", "GPU").priority(85), Panel::widget),
        ]
    }
}
