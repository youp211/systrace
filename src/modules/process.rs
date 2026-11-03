//! macOS process and thread counts.
//! Read-only: proc_listallpids and proc_pidinfo(PROC_PIDTASKINFO).

use crate::platform::macos::{self, TaskInfo};
use crate::prelude::*;

pub const HEADER: Header = Header {
    id: "process",
    name: "Processes",
    reads: "proc_listallpids, proc_pidinfo(PROC_PIDTASKINFO)",
};

const COUNT: MetricId = MetricId::new("process.count");
const THREADS: MetricId = MetricId::new("process.threads");
/// Threads running at the instant of the sample, leaving out systrace's own.
const RUNNING: MetricId = MetricId::new("process.threads.running");

#[derive(Default)]
pub struct Process;

/// Thread and running-thread totals over the processes that could be read; `own`
/// contributes threads but not running ones, since its sampling thread always runs.
/// `None` when no process could be read, so nothing is reported as zero.
fn totals(
    tasks: impl IntoIterator<Item = (i32, Option<TaskInfo>)>,
    own: i32,
) -> Option<(u64, u64)> {
    let mut any = false;
    let (mut threads, mut running) = (0u64, 0u64);
    for (pid, info) in tasks {
        let Some(info) = info else { continue };
        any = true;
        threads += u64::from(info.threads);
        if pid != own {
            running += u64::from(info.running);
        }
    }
    any.then_some((threads, running))
}

impl Module for Process {
    fn header(&self) -> &'static Header {
        &HEADER
    }

    fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
        let pids = match macos::pids() {
            Ok(pids) => pids,
            Err(e) => {
                return Capabilities::all(
                    [COUNT, THREADS, RUNNING],
                    Availability::unsupported(format!("cannot list processes: {e}")),
                );
            }
        };
        let readable = pids
            .iter()
            .filter(|&&p| macos::task_info(p).is_some())
            .count();
        let threads = if readable == 0 {
            Availability::unsupported("no process could be inspected")
        } else if readable < pids.len() {
            Availability::degraded(
                format!("{readable} of {} processes readable", pids.len()),
                "run as root to include every process",
            )
        } else {
            Availability::Available
        };
        Capabilities::all([COUNT], Availability::Available).set([THREADS, RUNNING], threads)
    }

    fn metrics(&self) -> Vec<MetricDesc> {
        vec![
            MetricDesc::gauge(COUNT, Unit::Count).detect(Detect::Shift),
            MetricDesc::gauge(THREADS, Unit::Count).detect(Detect::Shift),
            MetricDesc::gauge(RUNNING, Unit::Count),
        ]
    }

    fn sample(&mut self, _ctx: &Ctx, out: &mut Sink) {
        let Ok(pids) = macos::pids() else { return };
        out.push(COUNT, pids.len() as f64);
        let own = std::process::id() as i32;
        let tasks = pids.iter().map(|&p| (p, macos::task_info(p)));
        if let Some((threads, running)) = totals(tasks, own) {
            out.push(THREADS, threads as f64);
            out.push(RUNNING, running as f64);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn task(threads: u32, running: u32) -> Option<TaskInfo> {
        Some(TaskInfo { threads, running })
    }

    #[test]
    fn totals_skip_unreadable_and_own_running() {
        let tasks = [(1, task(10, 2)), (2, None), (7, task(5, 1))];
        assert_eq!(totals(tasks, 7), Some((15, 2)));
    }

    #[test]
    fn nothing_readable_reports_nothing() {
        assert_eq!(totals([(1, None), (2, None)], 0), None);
        assert_eq!(totals([], 0), None);
    }
}
