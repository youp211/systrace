//! macOS memory pressure: free level, compressor, paging and swap.
//! Read-only: kern.memorystatus_level and host_statistics64.

use std::time::Instant;

use crate::platform::macos::{self, VmStats};
use crate::prelude::*;
use crate::util::rate::Rates;

pub const HEADER: Header = Header {
    id: "mem_pressure",
    name: "Memory pressure",
    reads: "kern.memorystatus_level, host_statistics64",
};

/// Percent of memory the kernel counts as free of wired and compressed pages;
/// it falls as pressure rises (what `memory_pressure` reports as "free percentage").
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
    (COMPRESSIONS, Kind::Counter, Unit::BytesPerSec, |v| {
        v.compressions
    }),
    (DECOMPRESSIONS, Kind::Counter, Unit::BytesPerSec, |v| {
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
    /// The last answer used. The kernel repeats a cached answer when asked too often.
    last: Option<VmStats>,
    /// Whether kern.memorystatus_level could be read at probe.
    level: bool,
}

impl MemPressure {
    /// Pushes one answer. A repeat of the last answer is the kernel's cache: it would
    /// read as zero activity followed by a doubled rate, so it becomes a gap instead.
    fn push_vm(&mut self, vm: VmStats, now: Instant, out: &mut Sink) {
        if self.last == Some(vm) {
            return;
        }
        self.last = Some(vm);
        for (id, kind, _, read) in VM {
            let value = read(&vm) as f64;
            let value = match kind {
                Kind::Gauge => Some(value),
                Kind::Counter => self.rates.update(&id, value, now),
            };
            if let Some(v) = value {
                out.push(id, v);
            }
        }
    }
}

impl Module for MemPressure {
    fn header(&self) -> &'static Header {
        &HEADER
    }

    fn probe(&mut self, ctx: &Ctx) -> Capabilities {
        let level = match macos::sysctl_i32(LEVEL_SYSCTL) {
            Ok(_) => Availability::Available,
            Err(e) => Availability::unsupported(format!("kern.memorystatus_level: {e}")),
        };
        self.level = level.is_usable();
        let vm = match macos::vm_stats() {
            Ok(vm) => {
                // Seed the rate baseline with this answer rather than spend another call.
                self.push_vm(vm, ctx.now, &mut Sink::default());
                Availability::Available
            }
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
        if self.level
            && let Ok(level) = macos::sysctl_i32(LEVEL_SYSCTL)
        {
            out.push(FREE, level.into());
        }
        if let Ok(vm) = macos::vm_stats() {
            self.push_vm(vm, ctx.now, out);
        }
    }

    fn panels(&self) -> Vec<Panel> {
        let mut panel = Panel::new("mem_pressure", "Memory pressure").priority(80);
        // Only when readable, so the rest of the panel still shows without it.
        if self.level {
            panel = panel.widget(Widget::gauge("free", FREE).higher_is_better());
        }
        vec![
            panel
                .widget(Widget::series("compressed", COMPRESSED))
                .widget(Widget::series("swap out", SWAPOUTS)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const PAGE: u64 = 16384;

    fn answer(faults: u64) -> VmStats {
        VmStats {
            wired: 1000 * PAGE,
            compressed: 0,
            compressor_input: 0,
            pageins: faults * PAGE,
            pageouts: 0,
            swapins: 0,
            swapouts: 0,
            faults,
            compressions: 0,
            decompressions: 0,
        }
    }

    fn pushed(m: &mut MemPressure, vm: VmStats, now: Instant) -> Vec<(MetricId, f64)> {
        let mut out = Sink::default();
        m.push_vm(vm, now, &mut out);
        out.samples().to_vec()
    }

    fn value(samples: &[(MetricId, f64)], id: &MetricId) -> Option<f64> {
        samples.iter().find(|(i, _)| i == id).map(|(_, v)| *v)
    }

    #[test]
    fn a_repeated_answer_is_a_gap_not_a_zero() {
        let mut m = MemPressure::default();
        let t0 = Instant::now();
        let first = pushed(&mut m, answer(100), t0);
        assert_eq!(first.len(), 3, "first answer: gauges only, no rates yet");

        // The kernel's cached copy: nothing pushed, and the rate baseline stays put.
        let cached = pushed(&mut m, answer(100), t0 + Duration::from_secs(1));
        assert!(cached.is_empty());

        // The next fresh answer spans both seconds: 50 faults over 2 s, not 50 in 1 s.
        let fresh = pushed(&mut m, answer(150), t0 + Duration::from_secs(2));
        assert_eq!(value(&fresh, &FAULTS), Some(25.0));
        assert_eq!(value(&fresh, &PAGEINS), Some(25.0 * PAGE as f64));
    }

    #[test]
    fn panel_keeps_working_metrics_without_the_level() {
        let without = MemPressure::default().panels();
        let metrics: Vec<_> = without[0]
            .widgets
            .iter()
            .map(|w| w.metric().clone())
            .collect();
        assert_eq!(metrics, [COMPRESSED, SWAPOUTS]);

        let with = MemPressure {
            level: true,
            ..MemPressure::default()
        }
        .panels();
        let free = &with[0].widgets[0];
        assert_eq!(free.metric(), &FREE);
        // 96% free is healthy, so it must not be colored as nearly full.
        assert_eq!(free.level(0.96), Level::Normal);
    }
}
