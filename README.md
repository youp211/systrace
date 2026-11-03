# systrace

A read-only performance monitor for macOS, Linux and Windows. It reads CPU, memory, disk, network, process, thermal and GPU metrics straight from OS APIs and shows them in a terminal dashboard, a desktop window, or as JSON/CSV for scripts.

systrace only observes. It never changes system settings, kills processes or sends packets, and it doesn't need root.

macOS on Apple Silicon is the primary platform. Linux and Windows build and run on the cross-platform modules.

## Build

Requires Rust 1.85 or newer.

```sh
cargo build --release
./target/release/systrace
```

For a machine without a display, leave out the GUI:

```sh
cargo build --release --no-default-features \
  --features cpu,memory,mem_pressure,disk,net,process,thermals,gpu,tui
```

On Linux, the GUI needs the X11 and xkb development packages: `libxkbcommon-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev`.

## Use

```sh
systrace                      # terminal dashboard (when run in a terminal)
systrace tui -i 500ms         # dashboard with a 500 ms sample interval
systrace gui                  # desktop window with the same panels

systrace doctor               # what this machine can measure, with fix hints
systrace doctor -p            # only metrics that are degraded or unavailable
systrace modules              # loaded modules and what each reads
systrace metrics [SEL...]     # metric catalog: unit, kind, availability
systrace snapshot [SEL...]    # one sample of current values
```

Every command takes `-f table|json|ndjson|csv`. Output is a table in a terminal and NDJSON when piped.

Selectors pick metrics by ID: `cpu` matches everything under `cpu.`, and `*` matches within IDs, as in `cpu.core.*.util`.

```sh
systrace snapshot memory disk -f json
systrace snapshot 'net.*.rx' -i 2s
```

Some metrics need more access than a normal user has. For example, macOS thread counts only include processes you own unless systrace runs as root. `doctor` lists each case and what would fix it.

## What it measures

| Module | Metrics | Platforms |
|---|---|---|
| `cpu` | total and per-core utilization, load average; per-core frequency where the OS reports live values | all |
| `memory` | total, used, available, free, swap | all |
| `mem_pressure` | memory-pressure level, wired, compressor size and contents, compressions, page-ins/outs, swap-ins/outs, faults | macOS |
| `disk` | space and read/write rate per device | all |
| `net` | bytes, packets and errors per second per active interface | all |
| `process` | process count, threads, running threads | macOS |
| `thermals` | every exposed temperature sensor, plus the hottest | all |
| `gpu` | utilization, and dedicated memory where the driver reports it | all |

Each module and each front end (`tui`, `gui`) is a cargo feature, all on by default. To build a subset:

```sh
cargo build --release --no-default-features --features cpu,memory,tui
```

## Project docs

- [PLAN.md](PLAN.md): scope, architecture and roadmap
- [CONTRIBUTING.md](CONTRIBUTING.md): rules, module contract and how to add a module

## License

Not yet chosen.
