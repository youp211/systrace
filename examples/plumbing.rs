//! Runs the core on its own, with no real modules and no front end:
//! a stand-in module goes through Host, Store, history and export.
//!
//! cargo run --no-default-features --example plumbing

use std::io::{self, Write};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::Duration;

use systrace::export::{self, Format, Record};
use systrace::host::Host;
use systrace::prelude::*;
use systrace::util::rate::Rates;

const FLOW: MetricId = MetricId::new("plumbing.flow");
const PRESSURE: MetricId = MetricId::new("plumbing.pressure");

const MESSAGE: &str = "Plumbing checked: the core runs on its own. Plug in some modules!";

/// Stand-in module: a counter that rises 1 KiB per tick and a steady gauge.
#[derive(Default)]
struct Pipe {
    total: f64,
    rates: Rates,
}

impl Module for Pipe {
    fn header(&self) -> &'static Header {
        &Header {
            id: "plumbing",
            name: "Plumbing",
            reads: "nothing; stand-in",
        }
    }

    fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
        Capabilities::all([FLOW, PRESSURE], Availability::Available)
    }

    fn metrics(&self) -> Vec<MetricDesc> {
        vec![
            MetricDesc::counter(FLOW, Unit::BytesPerSec),
            MetricDesc::gauge(PRESSURE, Unit::Percent),
        ]
    }

    fn sample(&mut self, ctx: &Ctx, out: &mut Sink) {
        self.total += 1024.0;
        if let Some(rate) = self.rates.update(&FLOW, self.total, ctx.now) {
            out.push(FLOW, rate);
        }
        out.push(PRESSURE, 42.0);
    }

    fn panels(&self) -> Vec<Panel> {
        vec![Panel::new("plumbing", "Plumbing").widget(Widget::gauge("pressure", PRESSURE))]
    }
}

#[derive(serde::Serialize)]
struct Row {
    metric: String,
    value: f64,
}

impl Record for Row {
    fn header() -> &'static [&'static str] {
        &["metric", "value"]
    }

    fn cells(&self) -> Vec<String> {
        vec![self.metric.clone(), self.value.to_string()]
    }
}

fn main() -> ExitCode {
    let mut host = Host::new(vec![Box::new(Pipe::default())]);
    host.tick();
    thread::sleep(Duration::from_millis(50));
    host.tick();

    let store = host.store();
    let checks = [
        (
            "counter became a rate",
            store.latest(&FLOW).is_some_and(|r| r > 0.0),
        ),
        ("gauge stored as-is", store.latest(&PRESSURE) == Some(42.0)),
        (
            "history kept both ticks",
            store.history(&PRESSURE).count() == 2,
        ),
        ("panel survived", host.panels().len() == 1),
    ];
    let rows: Vec<Row> = store
        .latest_all()
        .map(|(d, value)| Row {
            metric: d.id.to_string(),
            value,
        })
        .collect();
    let exported = export::write(&mut Vec::new(), &rows, Format::Csv).is_ok();

    let failed: Vec<_> = checks
        .iter()
        .chain([&("export wrote CSV", exported)])
        .filter(|(_, ok)| !ok)
        .map(|(name, _)| *name)
        .collect();
    if !failed.is_empty() {
        eprintln!("plumbing leak: {}", failed.join(", "));
        return ExitCode::FAILURE;
    }

    say(MESSAGE);
    ExitCode::SUCCESS
}

/// Through cowsay when it's installed, plain text otherwise.
/// No shell is involved: cowsay is started directly and reads the message on stdin.
fn say(message: &str) {
    let spawned = Command::new("cowsay")
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut cow) = spawned else {
        println!("{message}");
        return;
    };
    let fed = cow
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(message.as_bytes()).is_ok());
    // Dropping stdin above closes the pipe so cowsay can finish.
    let mooed = cow.wait().is_ok_and(|status| status.success());
    if !(fed && mooed) {
        let _ = writeln!(io::stdout(), "{message}");
    }
}
