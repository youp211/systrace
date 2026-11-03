# systrace

A read-only performance monitor for macOS, Linux and Windows. It reads CPU, memory, disk, network, process, thermal and GPU metrics straight from OS APIs and shows them in a terminal dashboard, a desktop window, or as JSON/CSV for scripts.

systrace only observes. It never changes system settings, kills processes or sends packets, and it doesn't need root.

macOS on Apple Silicon is the primary platform. Linux and Windows build and run on the cross-platform modules.

## Build

Requires Rust 1.95 or newer.

```sh
cargo build --release
./target/release/systrace
```

For a machine without a display, leave out the GUI, or both front ends. The `modules` feature turns on every module:

```sh
cargo build --release --no-default-features --features modules,tui   # CLI and TUI
cargo build --release --no-default-features --features modules       # CLI only
```

On a Mac, these scripts run the three builds, each in its own target directory so that one never replaces another:

```sh
scripts/build-macos-all.sh   # target/macos-all/release/systrace: CLI, TUI and GUI
scripts/build-macos-tui.sh   # target/macos-tui/release/systrace: CLI and TUI
scripts/build-macos-cli.sh   # target/macos-cli/release/systrace: CLI only
```

On Linux, the GUI needs the X11 and xkb development packages: `libxkbcommon-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev`.

## Use

```sh
systrace                      # terminal dashboard (in a terminal, in builds with the TUI)
systrace tui -i 500ms         # dashboard with a 500 ms sample interval
systrace gui                  # desktop window with the same panels

systrace doctor               # what this machine can measure, with fix hints
systrace doctor -p            # only metrics that are degraded or unavailable
systrace modules              # loaded modules and what each reads
systrace metrics [SEL...]     # metric catalog: unit, kind, availability
systrace snapshot [SEL...]    # one sample of current values
```

Every command takes `-f table|json|ndjson|csv`. Output is a table in a terminal and NDJSON when piped. Intervals (`-i`) take `ms`, `s` or `m`, from 10 ms to 1440 m.

Selectors pick metrics by ID: `cpu` matches everything under `cpu.`, and `*` matches any run of characters, as in `cpu.core.*.util`.

```sh
systrace snapshot memory disk -f json
systrace snapshot 'net.*.rx' -i 2s
```

A metric with no fresh reading this sample is left out of `snapshot` and drawn as a gap (`·` in the TUI), never as a made-up zero. For example, CPU usage needs 200 ms between readings, so `snapshot -i 100ms` has no CPU rows.

Some metrics need more access than a normal user has. For example, macOS thread counts only include processes you can inspect unless systrace runs as root. `doctor` lists each case and what would fix it.

Exit codes: `0` ok, `1` error, `2` usage error, such as an unknown flag, an interval out of range, a selector that matches no metric, or no command when output is piped or the build has no TUI.

## What it measures

| Module | Metrics | Platforms |
|---|---|---|
| `cpu` | total and per-core utilization; load average (not Windows); per-core frequency where the OS reports live values (not macOS) | all |
| `memory` | total, used, available, free, swap | all |
| `mem_pressure` | free-memory level (`kern.memorystatus_level`), wired, compressor size and contents, compression and decompression rates, page-ins/outs, swap-ins/outs, faults | macOS |
| `disk` | space and read/write rate per device | all |
| `net` | bytes, packets and errors per second per active interface | all |
| `process` | process count, threads, running threads | macOS |
| `thermals` | every exposed temperature sensor, plus the hottest | all |
| `gpu` | utilization per GPU, and dedicated memory where the driver reports it | all |

On macOS, `memory.available` counts inactive pages too (sysinfo's definition), so `used` and `available` can add up to more than `total`.

Each module and each front end (`tui`, `gui`) is a cargo feature, all on by default. To build a subset:

```sh
cargo build --release --no-default-features --features cpu,memory,tui
```

## Project docs

- [PLAN.md](PLAN.md): scope, architecture and roadmap
- [CONTRIBUTING.md](CONTRIBUTING.md): rules, module contract and how to add a module

## License

Not yet chosen.
