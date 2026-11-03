# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**systrace** is a read-only performance monitor (CLI first, then TUI, GUI later). The full design and phased roadmap is in [PLAN.md](PLAN.md); hard rules and the module style guide are in [CONTRIBUTING.md](CONTRIBUTING.md). Read both before adding a module or command.

- **macOS first.** Primary dev hardware is an Apple Silicon Mac. Build and verify macOS implementations first (e.g. `mem_pressure`, `gpu_apple` via IOReport/IORegistry, `phys_footprint`, `/dev/bpf*`). Linux and Windows still need to compile and pass CI, but their deep-metric modules come after macOS.

## Commands

```sh
cargo build
cargo run -- tui                    # interactive dashboard; also the default with no args on a TTY
cargo run -- doctor                 # binary is `systrace`; args go after `--`
cargo run -- snapshot cpu -f json
cargo test                          # whole workspace
cargo test -p systrace-core           # one crate
cargo test -p systrace-core counter_becomes_rate   # one test
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo run -p systrace-cli --no-default-features --features systrace-modules/cpu -- modules   # chosen modules only
```

CI ([.github/workflows/ci.yml](.github/workflows/ci.yml)) runs fmt, clippy with `-D warnings`, tests, and smoke tests (`doctor -f json`, `snapshot -f json`) on macOS, Windows and Linux. Code gated by `#[cfg(target_os = ...)]` must still build and lint clean on the other two.

## Architecture

Cargo workspace, `crates/*`, edition 2024. Three layers with strict one-way dependencies:

- **`crates/core`** is the host. `Host` ([host.rs](crates/core/src/host.rs)) calls each module's `probe()` once to get per-metric `Capabilities`, registers its `MetricDesc`s in the `Store`, then on each `tick()` collects samples from a `Sink` and drops any for metrics that aren't usable. `Store` ([store.rs](crates/core/src/store.rs)) keeps the latest value per metric, turns `Kind::Counter` totals into per-second rates (via `util::rate`; a counter reset or first sample yields no value), and keeps a per-tick history ring (`HISTORY` ticks) where a tick without a sample is recorded as a `None` gap. `Host::panels()` only returns panels whose widgets' metrics are all usable.
- **`crates/modules`** reads the OS. Each module is a plain struct implementing `Module` ([module.rs](crates/core/src/module.rs)), compiled in through its cargo feature (all listed in `default`) and gated with `#[cfg(all(feature = "x", target_os = "..."))]` on both its `mod` and its `reg.push(...)` in `all()` ([lib.rs](crates/modules/src/lib.rs)), so OS-specific features are no-ops elsewhere. Module selection is done on `systrace-modules` features; the CLI just forwards `default`. At runtime, `Host` doesn't sample a module whose probe found no usable metric (e.g. a missing runtime library such as NVML or libpcap, which are dlopen'd, never linked), but `doctor`/`modules` still report it with its fix hint. `cpu` ([cpu.rs](crates/modules/src/cpu.rs)) is the reference module. Modules push raw `f64`s and describe `Panel`/`Widget`s by metric ID; they never format or render.
- **`crates/tui`** (ratatui, using its re-exported crossterm) renders `Host::panels()` and the store's history. Panel placement comes from `core::layout::columns` (priority order, shortest column first), so a later GUI reuses the same arrangement; only the column count and panel height are front-end decisions. Widgets render as one row each: label, bar or sparkline, formatted value.
- **`crates/cli`** (binary `systrace`) builds a `Host` from `systrace_modules::all()` and either hands it to the TUI or prints through `core::export`. Each command defines a row struct implementing `export::Record` (`header`/`cells`, with optional `table_header`/`table_cells` overrides) and calls `export::write`, which handles table/json/ndjson/csv. `-f` defaults to table on a TTY and ndjson when piped. A broken pipe exits 0.

Modules use sysinfo where it reports real values. Where it doesn't, they call the safe wrappers in `core::platform::<os>` ([macos.rs](crates/core/src/platform/macos.rs): sysctl, `host_statistics64`, `proc_pidinfo`), which hold all of the project's `unsafe`. Read OS APIs directly; don't shell out to tools like `vm_stat` or `powermetrics` for sampled metrics (process spawn per tick, unstable text formats, and `powermetrics` needs root). Shelling out is fine for one-shot reads such as settings. Modules that enumerate devices (disk, net, thermals, gpu) fix their device list in `probe()`; when nothing is found they report a module-level ID (`disk`, `net`, ...) as unavailable so `doctor` can explain why.

Single sources of truth, reuse rather than duplicate: value formatting in `core::fmt`, all output in `core::export`, selector matching (`cpu` subtree, `cpu.core.*.util` glob) in `core::util::select`, counter→rate in `core::util::rate`, OS names → metric ID segments in `core::util::id`. A helper needed a second time moves into `core::util` (or `core::platform` for OS-specific code).

`MetricId` is a dotted lowercase name (`module.subject.metric`) with natural ordering (`cpu.core.2` < `cpu.core.10`). Metric IDs are a public interface: renaming one is a breaking change.

## Rules that are easy to miss

- **Read-only.** Never change system, OS, network or app state. The only files written are systrace's own config, baselines, recordings and exports.
- No `unwrap()`/`expect()` outside tests. `sample()` never panics or logs per tick: missing data means skipping the push. Explanations belong in `probe()` as `Availability` with a reason and a fix hint.
- OS access only in `probe()`/`sample()`; parsing goes in small pure functions tested against fixture strings (real OS output under `tests/fixtures/<os>/`).
- `#[cfg]` at file or registration level; `cfg!()` only inside `probe()`.
- Module files start with a `//!` comment saying what they read, then `Read-only.` Doc comments are a short title only. Split files past ~300 lines.
- Adding a module should touch only its own file/folder, its feature in `crates/modules/Cargo.toml` and its registry lines. If core or a front end needs changing, fix the abstraction instead.
- No async runtime, no dynamic plugins.
- Display primitives and trait methods (`analyze`, `settings`) are added only in the phase that first needs them. Check PLAN.md's Phases table before adding scope.
