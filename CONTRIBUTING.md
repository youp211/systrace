# Contributing to systrace

Read [PLAN.md](PLAN.md) first for scope and architecture.

## Scope rules

- **Read-only.** No code may change system, OS, network or app state: no process control, no setting changes, no config edits, no packet injection. systrace writes only its own config, baselines, recordings and exports. PRs that add a system write are out of scope.
- **General computing only.** No app- or game-specific integrations.
- **No root by default.** Features that need privileges degrade cleanly, and `probe()` explains what's needed.

## Architecture rules

- **Modules read the OS and push values.** They never format values or draw UI.
- **Front ends never touch the OS.**
- **Modules convert, core stores.** A module pushes values in the unit its `MetricDesc` declares. Core stores them exactly as pushed.
- **Layers inside the crate.** Core (everything in `src/` outside `modules/`, `tui/` and `gui/`) uses no module or front end. Modules use only `crate::prelude`, `crate::util` and `crate::platform`. Front ends use core but never `platform` or `modules`. The CLI binary (`main.rs`, `commands.rs`) is the composition root: it may use every layer except `platform`. `tests/boundaries.rs` parses the source and enforces this, including `super::` paths and macro bodies. Aliasing the crate (`use crate as x`, `extern crate self`) and `#[path]` are rejected.
- **Second use → extract.** When a function is needed a second time, move it to `crate::util`, or to `crate::platform::<os>` if it is OS-specific. Don't abstract before that.
- **One way to do each thing:** one formatter (`crate::fmt`), one exporter (`crate::export`), one selector matcher (`util::select`), one rate calculator (`util::rate`), one panel layout (`crate::layout`), one sampling cadence (`crate::pace`), one text cleaner (`util::text`).
- **All `unsafe` lives in `crate::platform`,** behind safe functions with a `SAFETY:` comment on every block. The crate denies `unsafe_code` and only `platform` allows it; clippy denies undocumented unsafe blocks.
- **The host trusts nothing.** It ignores capabilities and metrics outside a module's own ID, a second description of the same ID, and samples for metrics the module can't use or never declared. The store turns a non-finite value into a gap.
- **Text from the OS is untrusted.** Labels and titles lose control characters when a `Widget` or `Panel` is built, and table and CSV output clean their cells too, so a device name can't send terminal escape sequences or spreadsheet formulas.
- **Read APIs directly.** Don't shell out to tools like `vm_stat` or `powermetrics` for sampled metrics. Starting a process every tick costs too much, text formats change between OS releases, and some tools need root. Running a tool is fine for one-shot reads such as settings; start it directly, never through a shell.
- **Static registry.** Modules are plain structs compiled in through cargo features. No dynamic loading and no async runtime.
- **Plug and play.** Adding a module touches only its own file in `src/modules/`, its feature in `Cargo.toml` and its line in `modules!`. CI builds every module alone, so a module must not depend on another being compiled in. If a module needs a change to core or a front end, fix the abstraction.

## Module rules

**Structure**
- The file opens with a `//!` comment saying what it reads, then `Read-only.`
- `pub const HEADER: Header` follows the imports: `id`, `name` and what it `reads`. `header()` returns `&HEADER`.
- Metric IDs are `const`s near the top, dotted lowercase `module.subject.metric`, where `module` is `HEADER.id`. Build IDs from OS-provided names with `util::id::segments`.
- OS access happens only in `probe()` and `sample()`. Parsing and matching go in small pure functions, tested against real OS output kept as string or struct constants in the file's test module. Tests of an OS-gated module run only on that OS's CI runner.
- Use `#[cfg]` at file or registration level. `cfg!()` is only for availability decisions inside `probe()`.
- Files over about 300 lines get split (`mod.rs`, `parse.rs`, `platform_*.rs`).

**Values**
- Push base units: bytes (not pages or KiB), Hz (not MHz), °C, percent 0–100.
- Counters: keep a `util::rate::Rates` in the module and push `rates.update(&id, total, ctx.now)` when it returns a rate. On the first total or a counter reset it returns `None`, so push nothing.
- A module that lists devices (disks, interfaces, sensors, GPUs) fixes the list in `probe()`, sorted so repeated names get the same `_2` suffix on every run. If it finds none, it reports its module-level ID (for example `disk`) as unavailable, so `doctor` can explain why.
- Re-find each device by a stable identity (device id, bus address, name), never by list position, and refresh sysinfo lists with `true` so a device that disappears leaves gaps rather than stale values.
- When the OS can repeat a cached answer (macOS `host_statistics64`), treat an identical answer as no new data: skip the push and don't feed it to `Rates`.
- A source with a minimum refresh interval (sysinfo CPU usage, `MINIMUM_CPU_UPDATE_INTERVAL`) gets no push until the interval has passed.

**Errors**
- No `unwrap()` or `expect()` outside tests.
- `sample()` never panics and never logs per tick. Missing data means skipping the push; the store records the gap.
- Anything that needs explaining belongs in `probe()`, as an `Availability` with a reason and, where one exists, a fix. Measure a degraded state rather than assume it, e.g. "514 of 818 processes readable".

**Comments**
- Doc comments are a short title. Input formats live in the tests' fixture constants.
- No commented-out code and no `TODO` without an issue link.

## Checks

These must pass; CI runs them on macOS, Linux and Windows:

```sh
cargo fmt --check
rustfmt --check $(git ls-files '*.rs')                                    # cargo fmt misses files declared by modules!
cargo clippy --all-targets -- -D warnings
cargo test                                                                # includes conformance, boundaries and registry
cargo clippy --no-default-features --all-targets -- -D warnings           # core + CLI alone
cargo run --no-default-features --example plumbing                        # core alone, end to end
cargo clippy --no-default-features --features <feature> --all-targets -- -D warnings   # each module or front end alone
```

The MSRV is 1.95, set by sysinfo; CI checks it with `cargo check --locked --all-targets` on that toolchain.

## Example module

```rust
//! Linux pressure stall information (PSI) for memory.
//! Read-only: parses /proc/pressure/memory.

use crate::prelude::*;

pub const HEADER: Header = Header {
    id: "psi",
    name: "Pressure stall",
    reads: "/proc/pressure/memory",
};

const PATH: &str = "/proc/pressure/memory";

const SOME: MetricId = MetricId::new("psi.memory.some");
const FULL: MetricId = MetricId::new("psi.memory.full");

#[derive(Default)]
pub struct PsiMemory;

impl Module for PsiMemory {
    fn header(&self) -> &'static Header {
        &HEADER
    }

    fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
        let status = match std::fs::read_to_string(PATH) {
            Ok(_) => Availability::Available,
            Err(e) => Availability::unavailable(
                format!("cannot read {PATH}: {e}"),
                "Requires Linux 4.20+ with PSI enabled (some distros need boot flag psi=1)",
            ),
        };
        Capabilities::all([SOME, FULL], status)
    }

    fn metrics(&self) -> Vec<MetricDesc> {
        vec![
            MetricDesc::gauge(SOME, Unit::Percent).detect(Detect::Spike),
            MetricDesc::gauge(FULL, Unit::Percent).detect(Detect::Spike),
        ]
    }

    fn sample(&mut self, _ctx: &Ctx, out: &mut Sink) {
        // A failed read is a gap, not an error; the store records it.
        let Ok(text) = std::fs::read_to_string(PATH) else {
            return;
        };
        for (kind, id) in [("some", SOME), ("full", FULL)] {
            if let Some(v) = parse_avg10(&text, kind) {
                out.push(id, v);
            }
        }
    }

    fn panels(&self) -> Vec<Panel> {
        vec![
            Panel::new("psi", "Pressure stall")
                .priority(60)
                .widget(Widget::series("memory some", SOME))
                .widget(Widget::series("memory full", FULL)),
        ]
    }
}

/// PSI avg10 parser.
fn parse_avg10(text: &str, kind: &str) -> Option<f64> {
    text.lines()
        .find(|l| l.starts_with(kind))?
        .split_whitespace()
        .find_map(|field| field.strip_prefix("avg10="))?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "some avg10=1.50 avg60=0.80 avg300=0.20 total=12345\n\
                          full avg10=0.25 avg60=0.10 avg300=0.00 total=678\n";

    #[test]
    fn parses_avg10() {
        assert_eq!(parse_avg10(SAMPLE, "some"), Some(1.50));
        assert_eq!(parse_avg10(SAMPLE, "full"), Some(0.25));
        assert_eq!(parse_avg10("", "some"), None);
    }
}
```

## Registering a module

Save the module as `src/modules/psi.rs`. In `Cargo.toml`, add a feature and list it in `modules`, which `default` turns on. Dependencies only one OS needs go in a `[target.'cfg(...)'.dependencies]` table with `optional = true`:

```toml
[features]
modules = ["cpu", "psi"]
psi = []
```

In `src/modules/mod.rs`, add one line to `modules!`. The type must implement `Default`; `if "<os>"` makes the feature a no-op on other OSes:

```rust
modules! {
    "cpu" => cpu::Cpu;
    "psi" => psi::PsiMemory if "linux";
}
```

The line declares the file and registers the module under the same `cfg`. `tests/registry.rs` fails if a module file has no line or its feature is missing from `modules`; `tests/conformance.rs` samples it twice and checks every value it pushes; `tests/boundaries.rs` checks what it imports.

Finally, add the module's row to "What it measures" in README.md. Builds that use `modules`, such as the `scripts/build-macos-*.sh` builds, pick the module up on their own.
