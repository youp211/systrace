# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**systrace** is a read-only performance monitor: one Rust crate whose core includes the CLI, with modules (plugins), a TUI and a GUI built on top. [PLAN.md](PLAN.md) has the scope, architecture, roadmap and status; [CONTRIBUTING.md](CONTRIBUTING.md) has the hard rules, the module contract and a worked example module. Read both before adding a module or command.

**macOS first.** Primary hardware is an Apple Silicon Mac. Build and verify macOS implementations first. Linux and Windows must still compile and pass CI, and their deep-metric modules come after macOS.

## Commands

```sh
cargo build
cargo run                                   # TUI on a terminal; usage error (exit 2) when piped
cargo run -- tui -i 500ms                   # intervals: 10ms to 1440m
cargo run -- gui                            # needs a display
cargo run -- doctor -p                      # only degraded/unavailable metrics
cargo run -- snapshot cpu memory -f json
cargo test                                  # unit tests + tests/{conformance,boundaries,registry}.rs
cargo test tracks_totals_per_metric         # one test
cargo test --test boundaries                # one integration test file
cargo fmt --check
rustfmt --check $(git ls-files '*.rs')      # cargo fmt never reaches files declared by modules!
cargo clippy --all-targets -- -D warnings
cargo clippy --no-default-features --all-targets -- -D warnings      # core + CLI alone
cargo run --no-default-features --example plumbing                   # core alone, end to end (cowsay easter egg)
cargo run --no-default-features --features cpu,tui -- modules        # chosen features only
scripts/build-macos-cli.sh                  # release build in target/macos-cli/release/; also -tui, -all

# Lint for Linux or Windows from a Mac, without rustup: builds std from the toolchain's
# rust-src. Catches platform-only errors that macOS can't see (CI only runs on push).
RUSTC_BOOTSTRAP=1 cargo clippy -Zbuild-std=std,core,alloc,panic_abort,panic_unwind \
  --target x86_64-unknown-linux-gnu --target-dir target/linux --all-targets -- -D warnings
```

CI ([.github/workflows/ci.yml](.github/workflows/ci.yml)) runs on macOS, Linux and Windows, always under bash (PowerShell would only fail a step on its last command), with actions pinned to commits and cargo run with `--locked`:
- Linux first installs egui's X11/xkb dev packages.
- Checks: rustfmt on every file, clippy, tests, the core alone (clippy, tests, plumbing example), each feature alone (list from `cargo metadata`), and smoke tests that require `doctor`/`snapshot` JSON to have rows.
- A separate job runs `cargo check` on the MSRV.

MSRV is 1.95 (`rust-version` in [Cargo.toml](Cargo.toml)), set by sysinfo 0.40. Let-chains are fine.

The TUI and GUI can't be launched without a terminal or display. Check TUI rendering by drawing into ratatui's `TestBackend` (see the tests in [draw.rs](src/tui/draw.rs)), and keep GUI geometry in pure functions that can be unit-tested.

## Architecture

One package, `systrace`: library plus binary. The core and CLI are always compiled. Each module and each front end (`tui`, `gui`) is a cargo feature. `modules` turns on every module, and `default` is `modules`, `tui` and `gui`.

[tests/boundaries.rs](tests/boundaries.rs) enforces the layers. It parses every file with `syn` and resolves `crate::`/`super::`/`self::` paths, including in macro bodies, to the top-level item they reach. It rejects `use crate as x`, `extern crate self` and `#[path]`.

### Core

Core is everything in `src/` outside `modules/`, `tui/` and `gui/`, and it uses none of them.

- **`Host`** ([host.rs](src/host.rs)): probes each module once.
  - It keeps only capabilities and metric descriptions under the module's own `HEADER.id`; the first description of an ID wins.
  - Each `tick()` sets `Ctx::now`, samples modules with at least one usable metric, keeps only samples for usable, declared metrics, then closes the tick in the store.
- **`Store`** ([store.rs](src/store.rs)): keeps values exactly as pushed and drops non-finite ones.
  - `latest` holds only the last tick's values, so a skipped push shows as missing, not stale.
  - A per-tick history ring (`HISTORY` ticks) records missed samples as `None` gaps.
- **Display model** ([display.rs](src/display.rs)): `Panel`, and `Widget::Gauge` (with `.higher_is_better()` for metrics like free memory) or `Widget::Series`.
  - Labels and titles are cleaned of control characters on construction.
  - Front ends share `Widget::level` (bar color), `ratio` and `spark_heights` (scaling).
- **Placement and cadence:** `layout::columns` with `layout::rows` places panels by priority. `pace::Pace` is the sampling cadence: a steady grid, one tick after a stall, never a burst.
- **Output and helpers:**
  - `export` writes every CLI format; tables are padded by display width, with cleaned cells. CSV is quoted and its formulas defused.
  - `fmt` formats values: the unit is chosen after rounding, and `fmt::reading` is the dash for a missing value.
  - `util` has `rate` (`Rates`), `select` (iterative glob, `in_subtree`), `id` (OS names → unique ID segments) and `text::printable`.
- **`platform::macos`** ([macos.rs](src/platform/macos.rs)): the only code allowed `unsafe`. The crate denies `unsafe_code` and clippy denies undocumented unsafe blocks. It has safe wrappers over sysctl, `host_statistics64`, `proc_listallpids` and `proc_pidinfo`.
- **The CLI** ([main.rs](src/main.rs), [commands.rs](src/commands.rs)) is the composition root: it may use every layer but `platform`.
  - It builds a `Host` from `modules::all()`, then hands it to the TUI or GUI, or prints through `export`.
  - `-f` defaults to table on a TTY and NDJSON when piped, and a broken pipe exits 0.
  - Usage errors exit 2: bad interval, a selector that matches no metric, no command while piped or in a build without the TUI.

### Modules

[src/modules/](src/modules/) holds the plugins. They may use only `crate::prelude`, `crate::util` and `crate::platform`.

- **Header:** each file implements `Module` ([module.rs](src/module.rs)) and declares `pub const HEADER: Header` (`id`, `name`, `reads`).
- **Registration:** each has a feature in [Cargo.toml](Cargo.toml), listed in `modules`, and one line in the `modules!` macro ([mod.rs](src/modules/mod.rs)), e.g. `"process" => process::Process if "macos";`. [tests/registry.rs](tests/registry.rs) checks that every module file is registered and listed in `modules`, and that `modules` lists only modules.
- **Units:** modules convert to their declared unit before pushing: bytes, Hz, °C, percent, and per-second rates for counters via their own `util::rate::Rates`. `Kind::Counter` is only a label.
- **No fake data:** with no fresh reading, a module skips the push. The cases are a first counter total, a reset, a cached OS answer, a sysinfo refresh interval not yet up, and a device that is gone.
- **Device modules** (disk, net, thermals, gpu) fix their device list in `probe()`, sorted for stable suffixes. They re-find devices by stable identity (mount plus device ID, interface name, sensor label, PCI address) and refresh sysinfo lists with `true`. When nothing is found, they report a module-level ID (`disk`, `net`, `thermals`, `gpu`) as unavailable.
- **Conformance:** [tests/conformance.rs](tests/conformance.rs) checks headers, ID format and uniqueness, the capabilities↔metrics pairing and panel references. It also samples every module twice, checking that values are declared, owned, finite and, for percentages, between 0 and 100.
- **Reference module:** `cpu` ([cpu.rs](src/modules/cpu.rs)).

### Front ends

These never use `platform` or `modules`.

- **TUI** ([src/tui/](src/tui/), ratatui with its re-exported crossterm):
  - Panels stack by priority. Those that don't fit are counted in the footer, and a clipped panel ends with "+N more".
  - Sparklines are right-aligned, and a gap draws as `·`.
  - SIGTERM, SIGHUP and SIGINT restore the terminal (`signal-hook`, unix).
- **GUI** ([src/gui/](src/gui/), eframe/egui): draws the same panels with the same layout. It ticks the `Host` from `update()`, and between samples `request_repaint_after` keeps it idle. eframe is about 6 MB of the binary.

### Reading the OS

Use sysinfo where it reports real values; where it doesn't, add a safe wrapper in `platform::<os>`. Read OS APIs directly. Don't shell out to tools like `vm_stat` or `powermetrics` for sampled metrics: that's a process start per tick, the text formats change, and `powermetrics` needs root. Running a tool is fine for one-shot reads; start it directly, never through a shell, as `examples/plumbing.rs` does with `cowsay`.

`MetricId` is dotted lowercase (`module.subject.metric`) with natural ordering (`cpu.core.2` < `cpu.core.10`). The ordering doesn't allocate, and a tie falls back to the raw string, so `Equal` means `==`. Metric IDs are a public interface: renaming one is a breaking change.

## macOS and sysinfo facts the code depends on

- **Cached `host_statistics64` answers.** For non-Apple binaries, the kernel answers only a few `host_statistics64` calls per second (2–10, shared across processes) and otherwise repeats a cached answer. `mem_pressure` treats an answer identical to the last as no data. Without that, counters read 0/s and then double.
- **Kernel page size.** VM statistics count kernel pages: use `platform::macos::page_size()` (`vm_kernel_page_size`), not `vm_page_size`, which is 4 KiB under Rosetta.
- **CPU refresh window.** sysinfo recomputes CPU usage only after `MINIMUM_CPU_UPDATE_INTERVAL` (200 ms; 100 ms on the BSDs); asked sooner, it returns the old values. `cpu` skips its usage pushes until the window has passed. Its first tick after probe is therefore a gap.
- **No live CPU frequency.** sysinfo reports the same fixed nominal frequency for every core on macOS, so `cpu` declares the frequency metrics as unavailable there. Live frequency needs IOReport.
- **APFS mounts.** APFS lists `/` (read-only) and `/System/Volumes/Data` as one device. `disk` keeps one mount per device ID and prefers the writable one.
- **Process visibility.** Without root, `proc_pidinfo(PROC_PIDTASKINFO)` reads only some processes, about 60% here. `process` measures that at probe and reports "N of M readable" as `Degraded`, with a fix.
- **`mach_host_self`.** It and `vm_kernel_page_size` are declared locally in `platform::macos`: libc deprecates the first and doesn't bind the second. The host port is cached in a `OnceLock`, since each call adds a port reference.
- **GPU memory.** Apple Silicon GPUs report utilization but no dedicated memory (memory is unified). GPU metric IDs come from the model name, e.g. `gpu.apple_m3_max.util`.
- **sysinfo's `Gpus` isn't `Send` on Linux.** It holds the NVML library handle, while `Module: Send`, so `gpu` keeps the list on a reader thread and asks it for a listing each tick over a channel, with a timeout. macOS can't reveal this: lint for Linux before pushing (see Commands).
- **Free-memory level.** `kern.memorystatus_level` is the share of memory not wired or compressed. High is healthy, so its gauge is `higher_is_better`.
- **Device lists are fixed at `probe()`.** Devices that appear later need a restart.

## Rules that are easy to miss

- **Read-only.** Never change system, OS, network or app state. The only files written are systrace's own config, baselines, recordings and exports.
- **No panics.** No `unwrap()`/`expect()` outside tests. `sample()` never panics or logs per tick: missing data means skipping the push. Explanations belong in `probe()` as an `Availability` with a reason and a fix hint.
- **OS access** happens only in `probe()`/`sample()`. Parsing and matching go in small pure functions, tested against fixture constants of real OS output in the file's test module.
- **`cfg`:** `#[cfg]` at file or registration level; `cfg!()` only inside `probe()`.
- **File shape:**
  - Module files start with a `//!` comment saying what they read, then `Read-only.`
  - Doc comments are a short title.
  - Split files past about 300 lines.
- **Adding a module** touches only its file in `src/modules/`, its feature in `Cargo.toml` (listed in `modules`), its `modules!` line, and its README row. If core or a front end needs changing, fix the abstraction instead.
- **Shared helpers:**

  | For | Use |
  |---|---|
  | formatting | `fmt` |
  | output | `export` |
  | selectors | `util::select` |
  | rates | `util::rate` |
  | ID segments | `util::id` |
  | cleaning untrusted text | `util::text` |
  | bar colors and scales | `display` |
  | cadence | `pace` |

  A helper needed a second time moves into `util`, or into `platform` if it's OS-specific.
- **No async runtime, no dynamic plugins.**
- **Scope:** display primitives and trait methods (`analyze`, `settings`) are added only in the phase that first needs them. Check PLAN.md's Phases table before adding scope.
