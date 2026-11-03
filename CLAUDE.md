# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**systrace** is a read-only performance monitor: one Rust crate whose core includes the CLI, with modules (plugins), a TUI and a GUI built on top. [PLAN.md](PLAN.md) has the scope, architecture, roadmap and status; [CONTRIBUTING.md](CONTRIBUTING.md) has the hard rules, the module contract and a worked example module. Read both before adding a module or command.

**macOS first.** Primary hardware is an Apple Silicon Mac. Build and verify macOS implementations first. Linux and Windows must still compile and pass CI, and their deep-metric modules come after macOS.

## Commands

```sh
cargo build
cargo run                                   # TUI on a terminal, help when piped
cargo run -- tui -i 500ms
cargo run -- gui                            # needs a display
cargo run -- doctor -p                      # only degraded/unavailable metrics
cargo run -- snapshot cpu memory -f json
cargo test                                  # unit tests + tests/conformance.rs + tests/boundaries.rs
cargo test tracks_totals_per_metric         # one test
cargo test --test boundaries                # one integration test file
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --no-default-features --all-targets -- -D warnings      # core + CLI alone
cargo run --no-default-features --example plumbing                   # core alone, end to end (cowsay easter egg)
cargo run --no-default-features --features cpu,tui -- modules        # chosen features only
```

CI ([.github/workflows/ci.yml](.github/workflows/ci.yml)) runs on macOS, Linux and Windows. It installs egui's X11/xkb dev packages on Linux, then runs fmt, clippy and tests, then the core alone (clippy, tests, plumbing example), then every feature alone (feature list read from `cargo metadata`), then smoke tests (`doctor -f json`, `snapshot -f json`).

MSRV is 1.85 (`rust-version` in [Cargo.toml](Cargo.toml)). Clippy won't flag syntax newer than that, so don't use let-chains (`if let … && …`).

## Architecture

One package, `systrace`: library plus binary. The core and CLI are always compiled. Each module and each front end (`tui`, `gui`) is a cargo feature, all in `default`. The layers are enforced by [tests/boundaries.rs](tests/boundaries.rs), which scans `crate::` paths in the source:

- **Core** is everything in `src/` outside `modules/`, `tui/` and `gui/`, and it uses none of them.
  - `Host` ([host.rs](src/host.rs)) calls each module's `probe()` once for per-metric `Capabilities`, then registers its `MetricDesc`s. Each `tick()` sets `Ctx::now`, so every module shares one timestamp. It samples only modules with at least one usable metric, drops samples for unusable metrics, then closes the tick in the store.
  - `Store` ([store.rs](src/store.rs)) keeps values exactly as pushed. It never converts them. `latest` holds only the last tick's values, so a skipped push shows as missing, not stale. A per-tick history ring (`HISTORY` ticks) records missed samples as `None` gaps.
  - The display model ([display.rs](src/display.rs)) has `Panel` and `Widget` (`Gauge`, `Series`), plus rules both front ends share: `Level` (bar color thresholds) and `full_scale` (which units have a fixed top). `layout::columns` places panels by priority into the shortest column.
  - `export` writes every CLI format, `fmt` does all human-readable formatting, and `util` has `rate` (`Rates`), `select` (selectors) and `id` (OS names → metric ID segments).
  - `platform::macos` ([macos.rs](src/platform/macos.rs)) holds all of the project's `unsafe`, as safe wrappers over sysctl, `host_statistics64`, `proc_listallpids` and `proc_pidinfo`.
  - The CLI ([main.rs](src/main.rs), [commands.rs](src/commands.rs)) builds a `Host` from `modules::all()`, then hands it to the TUI or GUI, or prints through `export`. Each command has a row struct implementing `export::Record`. `-f` defaults to table on a TTY and NDJSON when piped, and a broken pipe exits 0. The `tui` and `gui` subcommands exist only when their features are on.
- **Modules** ([src/modules/](src/modules/)) are the plugins. They may use only `crate::prelude`, `crate::util` and `crate::platform`.
  - Each file implements `Module` ([module.rs](src/module.rs)) and declares `pub const HEADER: Header` (`id`, `name`, `reads`). `HEADER.id` is the first segment of every metric the module owns.
  - Each module has a feature in [Cargo.toml](Cargo.toml) and one line in the `modules!` macro ([mod.rs](src/modules/mod.rs)), e.g. `"process" => process::Process if "macos";`. The line expands to the gated `mod` and its registration in `all()`, so an OS-specific feature is a no-op elsewhere.
  - Modules convert to their declared unit before pushing: bytes, Hz, °C, percent, and per-second rates for counters via their own `util::rate::Rates`. They skip the push on a first total or a reset. `Kind::Counter` is only a label.
  - [tests/conformance.rs](tests/conformance.rs) checks every compiled-in module: unique, complete headers; metrics under the module's ID; a capability for each metric; panels that only show declared metrics.
  - `cpu` ([cpu.rs](src/modules/cpu.rs)) is the reference module.
- **Front ends** never use `platform` or `modules`.
  - [src/tui/](src/tui/) uses ratatui (with its re-exported crossterm) and renders `Host::panels()` and the store's history, one row per widget: label, bar or sparkline, value.
  - [src/gui/](src/gui/) uses eframe/egui and draws the same panels with the same layout. It ticks the `Host` from `update()`, and between samples `request_repaint_after` keeps it idle. eframe is about 6 MB of the binary.

Use sysinfo where it reports real values. Where it doesn't, add a safe wrapper in `platform::<os>`. Read OS APIs directly; don't shell out to tools like `vm_stat` or `powermetrics` for sampled metrics. Starting a process every tick costs too much, text formats change, and `powermetrics` needs root. Running a tool is fine for one-shot reads; start it directly, never through a shell (as `examples/plumbing.rs` does with `cowsay`).

`MetricId` is dotted lowercase (`module.subject.metric`) with natural ordering (`cpu.core.2` < `cpu.core.10`). Metric IDs are a public interface: renaming one is a breaking change.

## macOS facts the code depends on

- sysinfo reports the same fixed nominal frequency for every core, so `cpu` registers no frequency metrics on macOS. Live frequency needs IOReport.
- APFS lists `/` (read-only) and `/System/Volumes/Data` as the same device. `disk` keeps one mount per device ID and prefers the writable one.
- Without root, `proc_pidinfo(PROC_PIDTASKINFO)` only reads processes the user can inspect, about 60% of them. That's why the `process` thread metrics report `Degraded`.
- `mach_host_self` is declared locally in `platform::macos`, because libc deprecates its binding. Call it once (it's cached in a `OnceLock`), since each call adds a port reference.
- Apple Silicon GPUs report utilization but no dedicated memory (memory is unified), so `gpu.*.mem.*` are unavailable there.
- Device lists (disks, interfaces, sensors) are fixed at `probe()`. Devices that appear later need a restart.

## Rules that are easy to miss

- **Read-only.** Never change system, OS, network or app state. The only files written are systrace's own config, baselines, recordings and exports.
- No `unwrap()`/`expect()` outside tests. `sample()` never panics or logs per tick: missing data means skipping the push. Explanations belong in `probe()` as an `Availability` with a reason and a fix hint.
- OS access only in `probe()`/`sample()`. Parsing goes in small pure functions tested against fixture strings (real OS output under `tests/fixtures/<os>/`).
- `#[cfg]` at file or registration level; `cfg!()` only inside `probe()`.
- Module files start with a `//!` comment saying what they read, then `Read-only.` Doc comments are a short title. Split files past about 300 lines.
- Adding a module touches only its file in `src/modules/`, its feature in `Cargo.toml` and its `modules!` line. If core or a front end needs changing, fix the abstraction instead.
- Shared helpers: formatting in `fmt`, output in `export`, selectors in `util::select`, rates in `util::rate`, ID segments in `util::id`, bar colors and scales in `display`. A helper needed a second time moves into `util` (or `platform` if it's OS-specific).
- No async runtime, no dynamic plugins.
- Display primitives and trait methods (`analyze`, `settings`) are added only in the phase that first needs them. Check PLAN.md's Phases table before adding scope.
- The TUI and GUI can't be launched headless. Verify TUI rendering by drawing into ratatui's `TestBackend` (see the tests in [draw.rs](src/tui/draw.rs)), and keep GUI geometry in pure functions that can be unit-tested.
