//! macOS memory pressure: memorystatus level, compressor, paging and swap.
//! Read-only: kern.memorystatus_level and host_statistics64.

use crate::platform::macos::{self, VmStats};
use crate::prelude::*;
use crate::util::rate::Rates;

pub const HEADER: Header = Header {
    id: "mem_pressure",
    name: "Memory pressure",
    reads: "kern.memorystatus_level, host_statistics64",
};

const FREE: MetricId = MetricId::new("mem_pressure.free");
const WIRED: MetricId = MetricId::new("mem_pressure.wired");
const COMPRESSED: MetricId = MetricId::new("mem_pressure.compressor.size");
const COMPRESSED_INPUT: MetricId = MetricId::new("mem_pressure.compressor.stored");
const COMPRESSIONS: MetricId = MetricId::new("mem_pressure.compressor.compressions");
const DECOMPRESSIONS: MetricId = MetricId::new("mem_pressure.compressor.decompressions");
const PAGEINS: MetricId = MetricId::new("mem_pressure.pageins");
const PAGEOUTS: MetricId = MetricId::new("mem_pressure.pageouts");
const SWAPINS: MetricId = MetricId::new("mem_pressure.swapins");
const SWAPOUTS: MetricId = MetricId::new("mem_pressure.swapouts");
const FAULTS: MetricId = MetricId::new("mem_pressure.faults");

const LEVEL_SYSCTL: &std::ffi::CStr = c"kern.memorystatus_level";

type Field = fn(&VmStats) -> u64;

/// Every metric read from `VmStats`, with how to store it.
const VM: [(MetricId, Kind, Unit, Field); 10] = [
    (WIRED, Kind::Gauge, Unit::Bytes, |v| v.wired),
    (COMPRESSED, Kind::Gauge, Unit::Bytes, |v| v.compressed),
    (COMPRESSED_INPUT, Kind::Gauge, Unit::Bytes, |v| {
        v.compressor_input
    }),
    (COMPRESSIONS, Kind::Counter, Unit::PerSec, |v| {
        v.compressions
    }),
    (DECOMPRESSIONS, Kind::Counter, Unit::PerSec, |v| {
        v.decompressions
    }),
    (PAGEINS, Kind::Counter, Unit::BytesPerSec, |v| v.pageins),
    (PAGEOUTS, Kind::Counter, Unit::BytesPerSec, |v| v.pageouts),
    (SWAPINS, Kind::Counter, Unit::BytesPerSec, |v| v.swapins),
    (SWAPOUTS, Kind::Counter, Unit::BytesPerSec, |v| v.swapouts),
    (FAULTS, Kind::Counter, Unit::PerSec, |v| v.faults),
];

#[derive(Default)]
pub struct MemPressure {
    rates: Rates,
}

impl Module for MemPressure {
    fn header(&self) -> &'static Header {
        &HEADER
    }

    fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
        let level = match macos::sysctl_i32(LEVEL_SYSCTL) {
            Ok(_) => Availability::Available,
            Err(e) => Availability::unsupported(format!("kern.memorystatus_level: {e}")),
        };
        let vm = match macos::vm_stats() {
            Ok(_) => Availability::Available,
            Err(e) => Availability::unsupported(format!("host_statistics64: {e}")),
        };
        Capabilities::all([FREE], level).set(VM.map(|(id, ..)| id), vm)
    }

    fn metrics(&self) -> Vec<MetricDesc> {
        let free = MetricDesc::gauge(FREE, Unit::Percent).detect(Detect::Shift);
        let vm = VM.map(|(id, kind, unit, _)| match kind {
            Kind::Gauge => MetricDesc::gauge(id, unit),
            Kind::Counter => MetricDesc::counter(id, unit).detect(Detect::Spike),
        });
        std::iter::once(free).chain(vm).collect()
    }

    fn sample(&mut self, ctx: &Ctx, out: &mut Sink) {
        if let Ok(level) = macos::sysctl_i32(LEVEL_SYSCTL) {
            out.push(FREE, level.into());
        }
        let Ok(vm) = macos::vm_stats() else { return };
        for (id, kind, _, read) in VM {
            let value = read(&vm) as f64;
            let value = match kind {
                Kind::Gauge => Some(value),
                Kind::Counter => self.rates.update(&id, value, ctx.now),
            };
            if let Some(v) = value {
                out.push(id, v);
            }
        }
    }

    fn panels(&self) -> Vec<Panel> {
        vec![
            Panel::new("mem_pressure", "Memory pressure")
                .priority(80)
                .widget(Widget::gauge("free", FREE))
                .widget(Widget::series("compressed", COMPRESSED))
                .widget(Widget::series("swap out", SWAPOUTS)),
        ]
    }
}
