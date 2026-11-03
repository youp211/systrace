# Contributing to systrace

## Scope rules

- **Read-only.** No code may change system, OS, network or app state. That means no process control, no setting changes, no config edits and no packet injection. systrace only writes its own config, baselines, recordings and exports. PRs that add a system write are out of scope.
- **General computing only.** No app- or game-specific integrations.
- **No root by default.** Features that need privileges must degrade cleanly, and `probe()` must explain what's needed.

## Architecture rules

- Modules read the OS and push raw numbers. They never format values or draw UI.
- Front ends never touch the OS.
- **Second use → extract.** When a function is needed a second time, move it to `systrace_core::util` (or `systrace_core::platform` if OS-specific). Don't abstract before that.
- **One way to do each thing:** one formatter (`core::fmt`), one exporter (`core::export`), one selector matcher, one rate calculator.
- Modules are plain structs in a static registry. No dynamic loading and no async runtime.
- Adding a module should touch only its own folder, its feature in `crates/modules/Cargo.toml` and its registry lines. If it needs changes to core or a front end, fix the abstraction.

## Style

**Structure**
- Each module file opens with a `//!` comment: what it reads, followed by `Read-only.`
- OS access only happens in `probe()` and `sample()`. Parsing goes in small pure functions that are tested with fixture strings.
- Metric IDs are `const`s at the top of the file, written as dotted lowercase: `module.subject.metric`.
- Use `#[cfg]` at file or registration level. `cfg!()` is only allowed inside `probe()` for availability decisions.
- Files over about 300 lines get split (`mod.rs`, `parse.rs`, `platform_*.rs`).

**Errors**
- No `unwrap()` or `expect()` outside tests.
- `sample()` never panics and never logs every tick. Missing data means you skip the push, and core records the gap.
- Errors that need explaining belong in `probe()`, as an `Availability` with a reason and a fix hint.

**Comments**
- Doc comments are a short title only. Input formats live in test fixtures.
- No commented-out code and no `TODO` without an issue link.

**Tooling**
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` must pass.

## Example module

```rust
//! Linux pressure stall information (PSI) for memory.
//! Read-only: parses /proc/pressure/memory.

use systrace_core::platform::linux::read_to_string;
use systrace_core::prelude::*;

const PATH: &str = "/proc/pressure/memory";

const SOME: MetricId = MetricId::new("psi.memory.some");
const FULL: MetricId = MetricId::new("psi.memory.full");

#[derive(Default)]
pub struct PsiMemory;

impl Module for PsiMemory {
    fn id(&self) -> &'static str {
        "psi"
    }

    fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
        let status = match read_to_string(PATH) {
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
        // A failed read is a gap, not an error; core marks missing samples.
        let Ok(text) = read_to_string(PATH) else { return };

        for (kind, id) in [("some", SOME), ("full", FULL)] {
            if let Some(v) = parse_avg10(&text, kind) {
                out.push(id, v);
            }
        }
    }

    fn panels(&self) -> Vec<Panel> {
        vec![Panel::new("psi.memory", "Memory pressure")
            .priority(60)
            .widget(Widget::series("some", SOME))
            .widget(Widget::series("full", FULL))]
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

Registration, in `crates/modules/Cargo.toml` and `crates/modules/src/lib.rs`:

```toml
[features]
default = ["cpu", "psi"]
psi = []
```

```rust
#[cfg(all(feature = "psi", target_os = "linux"))]
mod psi;

#[cfg(all(feature = "psi", target_os = "linux"))]
reg.push(Box::new(psi::PsiMemory::default()));
```

Every module gets a feature and is listed in `default`. OS-specific modules also gate on `target_os`, so the feature is a no-op on other OSes. Their dependencies go in a `[target.'cfg(...)'.dependencies]` table as `optional = true`.
