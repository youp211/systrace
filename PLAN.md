# systrace plan

systrace is a read-only performance monitor for general computing workloads on macOS, Linux and Windows. It watches the whole system or one workload (a game, a training run, an LLM server, a compile), records it, finds bottlenecks and anomalies, and suggests OS and hardware settings worth looking at. It has a CLI, a TUI and a GUI over one shared core.

macOS on Apple Silicon is the primary platform: features land there first, and Linux and Windows follow.

## Scope

- **Read-only.** systrace observes, analyzes, reports and exports. It never changes system, OS, network or app state: no killing processes, no applying settings, no editing app configs, no sending packets.
- **Its own files only.** systrace writes only its config, baselines, recordings and exports.
- **`systrace run -- CMD`** launches the user's command so it can be tracked from t=0. Launching it is the only thing that command does.
- **General computing only.** No app-specific integrations: no frame-timing hooks, no scraping app metrics.
- **No root by default.** Elevated access unlocks extras, and `doctor` explains what each one needs.

## Stack

| Concern | Choice |
|---|---|
| Language | Rust, edition 2024, MSRV 1.85 |
| Baseline metrics | `sysinfo` |
| Deep metrics | native OS APIs behind `core::platform::<os>` |
| CLI | `clap`, built into the core |
| TUI | `ratatui` (with its re-exported `crossterm`) |
| GUI | `egui` / `eframe` |
| Serialization | `serde`, `serde_json` |
| Packet capture | libpcap / Npcap, loaded at runtime |

## Architecture

```
┌──────────────────────────── core ─────────────────────────────┐
│ Host · Store + history · Display model · layout · export · fmt│
│ util (rate, select, id) · platform (safe OS wrappers)         │
└──────▲──────────────▲──────────────▲──────────────▲───────────┘
       │ push         │ push         │ push         │ push
      cpu          memory          disk   …   modules (plugins)

core ──▶ cli   (clap)      one-shot, scripted, exports
     ──▶ tui   (ratatui)   interactive terminal
     ──▶ gui   (egui)      desktop window
```

- **Core is the host, and the CLI is part of it.** It owns module registration, sampling, the metric store and history, the display model and layout, export and the `systrace` command. Everything else builds on it. It runs on its own: `cargo run --no-default-features --example plumbing` drives a stand-in module through the whole pipeline.
- **Modules are plugins.** They read the OS, convert to the units they declare, and push values. They describe panels with core display primitives and never format values or draw UI.
- **Front ends** read from core and render its display model. They never touch the OS.

### Layout

One crate, `systrace`. The core and CLI are always built; modules and front ends are cargo features layered on top.

```
src/
  lib.rs, host.rs, store.rs, module.rs, metric.rs,     core: contract, host, store, display model,
  display.rs, layout.rs, export.rs, fmt.rs,            layout, export, formatting
  util/       rate, select, id
  platform/   safe OS wrappers (all `unsafe` lives here)
  main.rs, commands.rs                                 CLI, binary `systrace`
  modules/    one file and one feature per module      plugins
  tui/        feature `tui`                            terminal dashboard
  gui/        feature `gui`                            desktop window
examples/plumbing.rs                                   core alone, end to end
tests/conformance.rs, tests/boundaries.rs              plugin contract, layer boundaries
```

The layers inside the crate are enforced by `tests/boundaries.rs`: modules use only `prelude`, `util` and `platform`; front ends never use `platform` or `modules`; core uses neither modules nor front ends.

### Module contract

```rust
pub struct Header {
    pub id: &'static str,      // first segment of every metric the module owns
    pub name: &'static str,
    pub reads: &'static str,   // what it reads from the OS
}

pub trait Module: Send {
    fn header(&self) -> &'static Header;          // a `const HEADER` in the module file
    fn probe(&mut self, ctx: &Ctx) -> Capabilities;
    fn metrics(&self) -> Vec<MetricDesc> { vec![] }
    fn sample(&mut self, ctx: &Ctx, out: &mut Sink) {}
    fn panels(&self) -> Vec<Panel> { vec![] }
}
```

Planned additions, each with a default so existing modules keep compiling: `analyze(&self, store) -> Vec<Finding>` (phase 5) and `settings(&self, ctx) -> Vec<SettingState>` (phase 5.5).

- **Capabilities are per metric:** `Available`, `Degraded { reason }` or `Unavailable { reason, fix }`. Front ends only show usable metrics, `doctor` prints the full matrix, and the host never samples a module with nothing usable.
- **Modules convert, core stores.** A module pushes values in the unit its `MetricDesc` declares: bytes, Hz, °C, percent, and per-second rates for counters (via `util::rate::Rates`). The store keeps them as pushed, with a per-tick history in which a missed sample is a gap.
- **Display primitives:** `Gauge` and `Series` today, with `Table`, `KeyValue`, `Events` and `Hint` added when their first user lands. Widgets reference metric IDs. `core::layout::columns` places panels by priority, and every front end uses it.
- **Static registry.** Modules are compiled in through cargo features and one `modules!` line each in `src/modules/mod.rs`. No dynamic loading and no async runtime; a module uses threads plus a channel where it needs them.

## Modules

| Module | Source | Platforms | Status |
|---|---|---|---|
| `cpu` | sysinfo: utilization, load; live per-core frequency where reported (not macOS) | all | ✅ |
| `memory` | sysinfo: physical memory, swap | all | ✅ |
| `mem_pressure` | `kern.memorystatus_level`, `host_statistics64`: compressor, paging, swap, faults | macOS | ✅ |
| `disk` | sysinfo: space and I/O per device (APFS volumes de-duplicated by device) | all | ✅ |
| `net` | sysinfo: bytes, packets, errors per active interface | all | ✅ |
| `process` | `proc_listallpids`, `proc_pidinfo`: process, thread and running-thread counts | macOS | ✅ counts |
| `thermals` | sysinfo: temperature sensors | all, best-effort | ✅ |
| `gpu` | sysinfo: utilization, dedicated memory | all | ✅ |
| `process` (full) | process tree, per-process CPU/mem/IO, target tracking | all | planned |
| `memory` (per-process) | `phys_footprint` (`proc_pid_rusage`), PSS (`smaps_rollup`), private bytes | all | planned |
| `cpu_freq_apple` | IOReport: live per-cluster frequency and power | macOS | planned |
| `psi` | `/proc/pressure/{cpu,memory,io}` | Linux | planned |
| `gpu_apple` | IOReport + IORegistry: util, power, per-client GPU time | macOS | planned |
| `gpu_nvidia` | NVML (dlopen): util, VRAM, per-process, clocks, power, throttle reasons, PCIe | Linux, Windows | planned |
| `gpu_drm` | sysfs + DRM fdinfo: AMD/Intel, per-process | Linux | planned |
| `gpu_win` | PDH "GPU Engine" / "GPU Process Memory" | Windows | planned |
| `wifi` | RSSI, noise, tx rate, BSSID/roams | all | planned |
| `net_probe` | ICMP/TCP probes to gateway, DNS, anchor, user targets | all | planned |
| `net_capture` | libpcap/Npcap: per-flow TCP/UDP, RTT, retransmits, jitter, DNS | all, needs privileges | planned |

### Memory pressure sources

| | Linux | macOS | Windows |
|---|---|---|---|
| System | PSI some/full | memorystatus level, compressor, swap I/O | commit vs limit, hard faults/s, compression store |
| Process | RSS, PSS, swap, major faults | phys_footprint | working set, private bytes, page faults |

### Network

**Passive sources:**
- interface counters, errors and drops
- TCP retransmits
- per-connection RTT mapped to PIDs (netlink `INET_DIAG`, `nettop`, `GetExtendedTcpTable`)
- Wi-Fi stats
- link, IP, route and DNS change events

**Active probes** (opt-in per target):
- RTT, jitter and loss to the gateway, the DNS resolver, a public anchor and user-defined targets
- unprivileged ICMP, falling back to TCP-connect timing

**`net_capture`:**
- Loads libpcap or Npcap at runtime and takes tcpdump-compatible BPF filters.
- Captures headers only (snaplen about 128). Promiscuous mode is off, it never sends packets, and DNS parsing can be turned off.
- Live mode keeps a bounded flow table and pushes only per-flow aggregates to the store.
- Reads `.pcap`/`.pcapng` files from tcpdump or Wireshark. Raw packets are written only by the explicit `capture -o` command.
- `doctor` reports the privileges it needs: `CAP_NET_RAW`/`CAP_NET_ADMIN` on Linux, `/dev/bpf*` access on macOS, Npcap on Windows.

## Analysis

**Generic detectors** run on any metric that opts in through `MetricDesc::detect`:
- spikes: rolling median/MAD with a debounce
- sustained shifts: CUSUM
- discrete events: link up/down, IP/route/DNS change, Wi-Fi roam

Every finding carries its evidence: values, baseline, time window and the processes involved.

**Bottleneck hints:** CPU-bound on one thread, I/O- or input-pipeline-bound, PCIe transfer-bound, VRAM spill, power or thermal throttling, paging.

**Network localization:** local or Wi-Fi, Wi-Fi roam, ISP or upstream, remote host, bufferbloat, slow DNS, congestion, physical link. Baselines are kept per network, keyed by gateway MAC or SSID.

**Advisor (informational only):**
- Modules report current settings: power plan, governor, Low Power Mode, GPU power limit, Wi-Fi band, DNS resolver.
- Advice lives in a TOML knowledge base and only shows when a finding matches and the setting isn't already optimal.
- Each entry gives the title, the change, how to make it yourself, impact, risk and how to revert.

## CLI

Available:

```
systrace                          # TUI on a terminal, help when piped
systrace tui [-i 1s]
systrace gui [-i 1s]
systrace doctor [-p]              # capability matrix + fix hints; -p: problems only
systrace modules                  # loaded modules, what they read, status counts
systrace metrics [SEL...]         # catalog: id, unit, kind, detector, availability
systrace snapshot [SEL...] [-i 500ms]
```

Planned:

```
systrace tui [--pid N | --name X]
systrace watch SEL... [-i 500ms] [-n COUNT | -d 30s] [--until EXPR]
systrace top [--sort cpu|mem|gpu|io|net] [--limit 10] [--once]

systrace record -o run.smr [-d 10m] [--pid N | --name X | -- CMD] [--capture [-i IF] [-F BPF] [--pcap out.pcapng]]
systrace run -o run.smr -- CMD

systrace report [run.smr | cap.pcapng] [--fail-on warning]
systrace anomalies [run.smr] [--follow]
systrace advise [run.smr]
systrace diff a.smr b.smr

systrace capture -i IF [-F BPF] [-d 60s] -o cap.pcapng
systrace flows [-i IF | cap.pcapng] [--sort bytes|rtt|retrans|jitter] [--pid N]

systrace export run.smr -f csv|json|ndjson|perfetto [-o out] [--flows] [SEL...]
```

**Shared conventions:**
- `-f/--format table|json|ndjson|csv` works on every command: `table` on a terminal, `ndjson` when piped.
- Selectors: `cpu` matches the whole subtree, `*` matches within an ID (`cpu.core.*.util`), and `proc[name=python].mem.footprint` will target a process.
- `-i`/`-n`/`-d` (interval/count/duration) and `--pid`/`--name`/`-- CMD` (target) mean the same thing on every command.
- `-F` is a BPF filter.
- Exit codes: `0` ok, `1` error, `2` usage, `3` threshold or `--fail-on` hit.

## Exports

| Format | Use | Status |
|---|---|---|
| table / JSON / NDJSON / CSV | terminal, scripts, spreadsheets | ✅ |
| `.smr` | native, versioned recording: metrics, events, findings, capabilities, system info | planned |
| Perfetto / Chrome trace | timeline viewing | planned |
| pcapng | raw headers from `capture` / `--pcap` | planned |
| Parquet, OpenMetrics | large recordings, monitoring stacks | later |

All output goes through `core::export`. Every recording and export embeds the system info and the capability report.

## Phases

| Phase | Scope | Status |
|---|---|---|
| **0** | Core, CLI `doctor`/`modules`/`metrics`/`snapshot`, `cpu`, CI on 3 OSes | ✅ |
| **1** | `memory`, `disk`, `net`, `process`, `thermals`, history ring, TUI + shared layout | ✅ (except `watch`, `top`, full `process`) |
| **2** | Target mode (`--pid`/`--name`/`run`), per-thread CPU, per-process mem/IO/TCP, `psi`, `mem_pressure`, `wifi`; extract `util::sockets`, `util::flow` | `mem_pressure` ✅ |
| **3** | GPU vendor modules: IOReport → NVML → PDH → DRM | sysinfo `gpu` baseline ✅ |
| **4** | `record`, `.smr`, `diff`, `export` in all formats, TUI replay | |
| **5** | Analysis engine, `net_probe`, localization, `report`, `anomalies`, `--fail-on` | |
| **5.5** | Settings readers, `advise` | |
| **6** | `net_capture`, `capture`, `flows`, pcap import | |
| **7** | GUI | ✅ live view |
| Later | eBPF (off-CPU, per-process UDP), ETW, InfiniBand, Parquet/OpenMetrics | |

**Spikes** to de-risk the hard parts:
- macOS live CPU frequency and per-client GPU time from IOReport/IORegistry
- macOS per-process connections (alternatives to `nettop`)
- Windows PDH per-process GPU counters
- AMD fdinfo key names
- runtime loading of libpcap/Npcap

## Testing

- **Parsers are pure functions,** tested against real OS output in `tests/fixtures/<os>/` on every OS in CI.
- **Conformance:** `tests/conformance.rs` checks every compiled-in module: unique, complete headers, metrics under the module's ID, a capability for every metric, panels that only show declared metrics.
- **Boundaries:** `tests/boundaries.rs` checks from source that each layer only uses what it's allowed to.
- **Isolation:** CI builds and tests the core and CLI with no features (running the plumbing example), then every module and front-end feature alone.
- **CI matrix:** macOS, Windows and Linux run fmt, clippy (`-D warnings`), tests, the isolation builds and smoke tests (`doctor -f json`, `snapshot -f json`).
- **Front ends:** the TUI renders into ratatui's `TestBackend`; GUI geometry is unit-tested.
- **Hardware-specific modules** (GPU, Wi-Fi, capture) get manual testing on real machines.

## Stability

- Metric IDs are a public interface. Renaming one is a breaking change.
- `.smr` carries a format version from its first release.

## Open questions

- **License.** Undecided.
