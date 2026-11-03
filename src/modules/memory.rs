//! Physical memory and swap via sysinfo.
//! Read-only.

use sysinfo::System;
use crate::prelude::*;

pub const HEADER: Header = Header {
    id: "memory",
    name: "Memory",
    reads: "sysinfo: physical memory and swap",
};

const TOTAL: MetricId = MetricId::new("memory.total");
const USED: MetricId = MetricId::new("memory.used");
const AVAILABLE: MetricId = MetricId::new("memory.available");
const FREE: MetricId = MetricId::new("memory.free");
const UTIL: MetricId = MetricId::new("memory.util");
const SWAP_TOTAL: MetricId = MetricId::new("memory.swap.total");
const SWAP_USED: MetricId = MetricId::new("memory.swap.used");

#[derive(Default)]
pub struct Memory {
    sys: Option<System>,
}

impl Module for Memory {
    fn header(&self) -> &'static Header {
        &HEADER
    }

    fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
        let status = match System::new() {
            Ok(sys) => {
                self.sys = Some(sys);
                Availability::Available
            }
            Err(e) => Availability::unsupported(format!("cannot read memory stats: {e}")),
        };
        let ids = self.metrics().into_iter().map(|d| d.id);
        Capabilities::all(ids, status)
    }

    fn metrics(&self) -> Vec<MetricDesc> {
        vec![
            MetricDesc::gauge(TOTAL, Unit::Bytes),
            MetricDesc::gauge(USED, Unit::Bytes),
            MetricDesc::gauge(AVAILABLE, Unit::Bytes),
            MetricDesc::gauge(FREE, Unit::Bytes),
            MetricDesc::gauge(UTIL, Unit::Percent).detect(Detect::Shift),
            MetricDesc::gauge(SWAP_TOTAL, Unit::Bytes),
            MetricDesc::gauge(SWAP_USED, Unit::Bytes).detect(Detect::Shift),
        ]
    }

    fn sample(&mut self, _ctx: &Ctx, out: &mut Sink) {
        let Some(sys) = &mut self.sys else { return };
        sys.refresh_memory();

        let total = sys.total_memory() as f64;
        let used = sys.used_memory() as f64;
        out.push(TOTAL, total);
        out.push(USED, used);
        out.push(AVAILABLE, sys.available_memory() as f64);
        out.push(FREE, sys.free_memory() as f64);
        if total > 0.0 {
            out.push(UTIL, used / total * 100.0);
        }
        out.push(SWAP_TOTAL, sys.total_swap() as f64);
        out.push(SWAP_USED, sys.used_swap() as f64);
    }

    fn panels(&self) -> Vec<Panel> {
        vec![
            Panel::new("memory", "Memory")
                .priority(85)
                .widget(Widget::gauge("used", UTIL))
                .widget(Widget::series("swap", SWAP_USED)),
        ]
    }
}
