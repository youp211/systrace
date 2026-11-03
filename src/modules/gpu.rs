//! GPU utilization and memory via sysinfo, the cross-vendor baseline.
//! Read-only.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

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

/// How long a tick waits for the reader thread; a slower answer becomes a gap.
const PATIENCE: Duration = Duration::from_millis(500);
/// The first listing can load a vendor library (NVML), so it gets longer.
const FIRST_PATIENCE: Duration = Duration::from_secs(5);

/// One GPU's figures from one refresh.
struct Reading {
    pci: PCI,
    name: String,
    usage: Option<f32>,
    used: Option<u64>,
    total: Option<u64>,
}

fn readings(gpus: &Gpus) -> Vec<Reading> {
    gpus.iter()
        .map(|g| Reading {
            pci: g.pci().clone(),
            name: g.model().unwrap_or("GPU").to_string(),
            usage: g.usage(),
            used: g.used_memory(),
            total: g.total_memory(),
        })
        .collect()
}

/// Owns sysinfo's GPU list on its own thread. On Linux that list holds the NVML
/// library handle, which can't move between threads, and modules must be `Send`.
/// The thread ends when the module is dropped.
struct Reader {
    ask: Sender<()>,
    answers: Receiver<Vec<Reading>>,
}

impl Reader {
    /// Starts the thread and returns its first listing.
    fn start() -> Result<(Self, Vec<Reading>), String> {
        let (ask, asks) = mpsc::channel::<()>();
        let (tell, answers) = mpsc::channel();
        let (first_tell, first) = mpsc::channel::<Result<Vec<Reading>, String>>();
        thread::Builder::new()
            .name("systrace-gpu".into())
            .spawn(move || {
                let mut gpus = match Gpus::new_with_refreshed_list() {
                    Ok(gpus) => gpus,
                    Err(e) => {
                        let _ = first_tell.send(Err(e.to_string()));
                        return;
                    }
                };
                if first_tell.send(Ok(readings(&gpus))).is_err() {
                    return;
                }
                while asks.recv().is_ok() {
                    // `true` drops GPUs that are gone, so they leave gaps rather than stale values.
                    gpus.refresh(true);
                    if tell.send(readings(&gpus)).is_err() {
                        return;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let listing = first
            .recv_timeout(FIRST_PATIENCE)
            .map_err(|_| "GPU listing timed out".to_string())??;
        Ok((Self { ask, answers }, listing))
    }

    /// A fresh listing, or `None` if the thread is gone or too slow this tick.
    fn read(&self) -> Option<Vec<Reading>> {
        // Drop an answer that arrived after an earlier tick gave up on it.
        while self.answers.try_recv().is_ok() {}
        self.ask.send(()).ok()?;
        self.answers.recv_timeout(PATIENCE).ok()
    }
}

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
    reader: Option<Reader>,
    devices: Vec<Device>,
}

impl Module for Gpu {
    fn header(&self) -> &'static Header {
        &HEADER
    }

    fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
        let (reader, mut found) = match Reader::start() {
            Ok((reader, found)) if !found.is_empty() => (reader, found),
            Ok(_) => return Capabilities::all([GPU], Availability::unsupported("no GPU found")),
            Err(e) => {
                return Capabilities::all(
                    [GPU],
                    Availability::unsupported(format!("cannot list GPUs: {e}")),
                );
            }
        };

        // Sorted by bus address and named after the model, so ids are stable across runs.
        found.sort_by(|a, b| a.pci.cmp(&b.pci));
        let segments = id::segments(found.iter().map(|r| r.name.as_str()));

        let mut caps = Capabilities::default();
        for (r, seg) in found.into_iter().zip(segments) {
            let id = |m: &str| MetricId::owned(format!("gpu.{seg}.{m}"));
            // Windows reads utilization from a rate counter, empty until a second sample.
            let util_ok = r.usage.is_some() || cfg!(windows);
            let util = if util_ok {
                Availability::Available
            } else {
                Availability::unsupported("driver doesn't report utilization")
            };
            // Apple Silicon shares system memory, so there is no separate VRAM figure.
            let mem = if r.used.zip(r.total).is_some() {
                Availability::Available
            } else {
                Availability::unsupported("driver doesn't report dedicated memory")
            };
            let dev = Device {
                pci: r.pci,
                name: r.name,
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
        self.reader = Some(reader);
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
        let Some(listing) = self.reader.as_ref().and_then(Reader::read) else {
            return;
        };
        for dev in &self.devices {
            let Some(r) = listing.iter().find(|r| r.pci == dev.pci) else {
                continue;
            };
            if let Some(u) = r.usage {
                out.push(dev.util.clone(), u.into());
            }
            if let Some(m) = r.used {
                out.push(dev.mem_used.clone(), m as f64);
            }
            if let Some(m) = r.total {
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
