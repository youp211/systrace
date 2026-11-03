//! macOS process and thread counts.
//! Read-only: proc_listallpids and proc_pidinfo(PROC_PIDTASKINFO).

use crate::platform::macos;
use crate::prelude::*;

pub const HEADER: Header = Header {
    id: "process",
    name: "Processes",
    reads: "proc_listallpids, proc_pidinfo(PROC_PIDTASKINFO)",
};

const COUNT: MetricId = MetricId::new("process.count");
const THREADS: MetricId = MetricId::new("process.threads");
const RUNNING: MetricId = MetricId::new("process.threads.running");

#[derive(Default)]
pub struct Process;

impl Module for Process {
    fn header(&self) -> &'static Header {
        &HEADER
    }

    fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
        if let Err(e) = macos::pids() {
            return Capabilities::all(
                [COUNT, THREADS, RUNNING],
                Availability::unsupported(format!("cannot list processes: {e}")),
            );
        }
        let threads = if macos::is_root() {
            Availability::Available
        } else {
            Availability::degraded("only processes this user can inspect; run as root for all")
        };
        Capabilities::all([COUNT], Availability::Available).set([THREADS, RUNNING], threads)
    }

    fn metrics(&self) -> Vec<MetricDesc> {
        vec![
            MetricDesc::gauge(COUNT, Unit::Count).detect(Detect::Shift),
            MetricDesc::gauge(THREADS, Unit::Count).detect(Detect::Shift),
            MetricDesc::gauge(RUNNING, Unit::Count).detect(Detect::Spike),
        ]
    }

    fn sample(&mut self, _ctx: &Ctx, out: &mut Sink) {
        let Ok(pids) = macos::pids() else { return };
        let (threads, running) = pids
            .iter()
            .filter_map(|&pid| macos::task_info(pid))
            .fold((0u64, 0u64), |(t, r), info| {
                (t + u64::from(info.threads), r + u64::from(info.running))
            });
        out.push(COUNT, pids.len() as f64);
        out.push(THREADS, threads as f64);
        out.push(RUNNING, running as f64);
    }

    fn panels(&self) -> Vec<Panel> {
        vec![
            Panel::new("process", "Processes")
                .priority(60)
                .widget(Widget::series("processes", COUNT))
                .widget(Widget::series("threads", THREADS)),
        ]
    }
}
