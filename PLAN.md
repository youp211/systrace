# systrace plan

A read-only, cross-platform (macOS, Windows, Linux) performance monitor for general computing workloads. It ships as a CLI and a TUI first, with a GUI later. It can watch the whole system or one workload: a game, a training run, an LLM server, a compile. It measures the workload, records it, finds bottlenecks and anomalies, and suggests OS and hardware settings worth looking at.

## Scope

- **Read-only.** systrace observes, analyzes, reports and exports. It never changes system, OS, network or app state. It never kills processes, applies settings, edits app or game configs, or sends packets.
- **The only files systrace writes** are its own config, baselines, recordings and exports.
- **`systrace run -- CMD`** launches the user's command so it can be tracked from t=0. Launching it is the only thing that command does.
- **General computing only.** systrace has no app-specific integrations: no frame-timing hooks and no app metric scraping.
- **No root needed by default.** Elevated access unlocks extras, and `doctor` explains what each extra needs.

## Stack

| Concern | Choice |
|---|---|
| Language | Rust (edition 2024) |
| Basic metrics fallback | `sysinfo` |
| Deep metrics | native APIs per OS (see Modules) |
| CLI | `clap` |
| TUI | `ratatui` + `crossterm` |
| GUI (later) | `egui` / `eframe` |
| Serialization | `serde`, `serde_json` |
| Packet capture | libpcap / Npcap, loaded at runtime |

## Architecture

```
┌────────────────────────── core ───────────────────────────┐
│  Host (registry + scheduler) · Store · Display model      │
│  Analysis engine · Recording · Export · fmt · util        │
└───────▲──────────────▲──────────────▲─────────────▲───────┘
        │ feeds        │ feeds        │ feeds       │ feeds
      cpu          gpu_nvidia     net_probe     net_capture   … modules

core ──▶ cli (clap)      scripted, one-shot, streaming, exports
     ──▶ tui (ratatui)   interactive
     ──▶ gui (egui)      later
```

- **Core is the host.** It owns module registration, sampling, the metric store, history, the display model and layout, analysis, recording and export.
- **Modules** read the OS. They push raw numbers to the store and describe panels using core display primitives. They never format values or draw UI.
- **Front ends** (CLI, TUI, GUI) read from core and translate display primitives into output. They never touch the OS.

### Crates

```
crates/
  core/      systrace-core      traits, store, display model, export, fmt, util
  modules/   systrace-modules   one folder per module, cargo feature per module
  cli/       systrace-cli       binary `systrace`
  tui/       systrace-tui       Phase 1
  gui/       systrace-gui       Phase 7
```

### Module contract

```rust
pub trait Module: Send {
    fn id(&self) -> &'static str;
    fn probe(&mut self, ctx: &Ctx) -> Capabilities;
    fn metrics(&self) -> Vec<MetricDesc> { vec![] }
    fn sample(&mut self, ctx: &Ctx, out: &mut Sink) {}
    fn panels(&self) -> Vec<Panel> { vec![] }
    // Added in their phases, each with a default:
    // fn analyze(&self, store: &Store) -> Vec<Finding>       (Phase 5)
    // fn settings(&self, ctx: &Ctx) -> Vec<SettingState>     (Phase 5.5)
}
```

- **Capabilities are per metric:** `Available`, `Degraded { reason }` or `Unavailable { reason, fix }`. Front ends only show what's usable, and `doctor` prints the full matrix with fix hints.
- **The store is central.** Modules declare metrics up front (unit, gauge/counter, detector), and core provides history, counter-to-rate conversion, recording, export and anomaly detection for every metric.
- **Display primitives:** `Gauge`, `Series`, `Table`, `KeyValue`, `Events`, `Hint`. Each is added when its first user lands. Widgets reference metric IDs, and renderers pull the values. `core::layout` places panels by priority, and every front end uses the same layout.
- **Modules are compiled in.** They're a static registry with cargo features: no dynamic plugins and no async runtime. Each module is threads plus a channel where needed.

## Modules

| Module | Source | Platforms |
|---|---|---|
| `cpu` | sysinfo; per-thread via OS APIs (Phase 2) | all |
| `memory` | sysinfo; phys_footprint (`proc_pid_rusage`), PSS (`smaps_rollup`), private bytes | all |
| `psi` | `/proc/pressure/{cpu,memory,io}` | Linux |
| `mem_pressure` | `kern.memorystatus_level`, compressor, swap-ins/outs | macOS |
| `disk` | per-mount usage, per-disk and per-process I/O | all |
| `net` | interface counters, errors, drops, TCP stats, connections → PID | all |
| `process` | process tree, per-process CPU/mem/IO, target tracking | all |
| `thermals` | sensors where exposed | all, best-effort |
| `gpu` | sysinfo fallback: utilization, dedicated memory | all |
| `gpu_nvidia` | NVML (dlopen): util, VRAM, per-process, clocks, power, throttle reasons, PCIe | Linux, Windows |
| `gpu_drm` | sysfs + DRM fdinfo (AMD/Intel, per-process) | Linux |
| `gpu_win` | PDH "GPU Engine" / "GPU Process Memory" (any vendor) | Windows |
| `gpu_apple` | IOReport + IORegistry (util, power, per-client GPU time) | macOS |
| `wifi` | RSSI, noise, tx rate, BSSID/roams | all |
| `net_probe` | ICMP/TCP probes to gateway, DNS, anchor, user targets | all |
| `net_capture` | libpcap/Npcap: per-flow TCP/UDP, RTT, retrans, jitter, DNS | all (needs privileges) |

### Memory pressure sources

| | Linux | macOS | Windows |
|---|---|---|---|
| System | PSI some/full | memorystatus level, compressed, swap I/O | commit vs limit, hard faults/s, compression store |
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
- Loads libpcap or Npcap at runtime and uses tcpdump-compatible BPF filters.
- Captures headers only (snaplen ~128). Promiscuous mode is off, it never sends packets, and DNS parsing can be turned off.
- Live mode keeps a bounded flow table and pushes only per-flow aggregates to the store.
- Reads `.pcap`/`.pcapng` files from tcpdump or Wireshark. Raw packets are written only by the explicit `capture -o` command.
- Privileges are reported by `doctor`: `CAP_NET_RAW`/`CAP_NET_ADMIN` on Linux, `/dev/bpf*` access on macOS, Npcap on Windows.

## Analysis

**Generic detectors** run on any metric that opts in through `MetricDesc`:
- spikes: rolling median/MAD with a debounce
- sustained shifts: CUSUM
- discrete events: link up/down, IP/route/DNS change, Wi-Fi roam

Findings always carry their evidence: values, baseline, time window and the processes involved.

**Bottleneck hints:**
- CPU-bound on a specific thread
- I/O- or input-pipeline-bound
- PCIe transfer-bound
- VRAM spill
- power or thermal throttling
- paging

**Network localization:**
- local or Wi-Fi
- Wi-Fi roam
- ISP or upstream
- remote host
- bufferbloat
- slow DNS
- congestion
- physical link

Network baselines are kept per network, keyed by gateway MAC or SSID.

**Advisor (informational only):**
- Modules report current settings: power plan, governor, Low Power Mode, GPU power limit, Wi-Fi band, DNS resolver.
- Advice lives in a TOML knowledge base and only shows when a finding matches and the setting isn't already optimal.
- Each entry gives the title, the change, how to make it yourself, impact, risk and how to revert.

## CLI

```
systrace                          # TUI (Phase 1; prints help until then)
systrace tui [--pid N | --name X]

systrace doctor [-p]              # capability matrix + fix hints; -p: problems only
systrace modules                  # loaded modules + status
systrace metrics [SEL...]         # catalog: id, unit, kind, availability

systrace snapshot [SEL...]        # one sample
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
- `-f/--format table|json|ndjson|csv` works on every command. The default is `table` on a terminal and `ndjson` when piped.
- Selectors: `cpu` (matches the whole subtree), `cpu.core.*.util`, `proc[name=python].mem.footprint` (Phase 2).
- `-i`/`-n`/`-d` (interval/count/duration) and `--pid`/`--name`/`-- CMD` (target) mean the same thing everywhere.
- `-F` is the BPF filter.
- Exit codes: `0` ok, `1` error, `2` usage, `3` threshold or `--fail-on` hit.

## Exports

| Format | Use |
|---|---|
| `.smr` | native, versioned: metrics, events, findings, capabilities, system info |
| JSON / NDJSON / CSV | reports, streaming, spreadsheets |
| Perfetto / Chrome trace | timeline viewing |
| pcapng | raw headers from `capture` / `--pcap` |
| Later: Parquet, OpenMetrics | large recordings, monitoring stacks |

All exports go through `core::export`. Every export embeds the system info and the capability report.

## Phases

| Phase | Scope |
|---|---|
| **0** | Core (trait, host, store, display types, fmt, export JSON/NDJSON/CSV/table). CLI `doctor`, `modules`, `metrics`, `snapshot`. `cpu` reference module. CI on all 3 OSes |
| **1** | `memory`, `disk`, `net`, `process`, `thermals`. History ring. CLI `watch`, `top`. TUI v1 + layout |
| **2** | Target mode (`--pid`/`--name`/`run`), per-thread CPU, per-process mem/IO/TCP, `psi`, `mem_pressure`, `wifi`. Extract `util::sockets`, `util::flow` |
| **3** | GPU modules: NVML → PDH → DRM → IOReport |
| **4** | `record`, `.smr`, `diff`, `export` (all formats), TUI replay |
| **5** | Analysis engine, `net_probe`, localization, `report`, `anomalies`, `--fail-on` |
| **5.5** | Settings readers + `advise` |
| **6** | `net_capture`, `capture`, `flows`, pcap import |
| **7** | GUI |
| Later | eBPF (off-CPU, per-process UDP), ETW, InfiniBand, Parquet/OpenMetrics |

**Early spikes** to de-risk the hard parts:
- Windows PDH per-process GPU counters
- macOS per-client GPU time from IORegistry
- AMD fdinfo key names
- macOS per-process connections (alternatives to `nettop`)
- runtime loading of libpcap/Npcap

## Testing

- **Parsers are pure functions,** tested against real OS output saved in `tests/fixtures/<os>/`. They run on every OS in CI.
- **CI matrix:** macOS, Windows and Linux running fmt, clippy (`-D warnings`), tests, and smoke tests (`systrace doctor`, `systrace snapshot -f json`).
- **Hardware-specific modules** (GPU, Wi-Fi, capture) get manual testing on real machines. Primary dev hardware is an Apple Silicon Mac.

## Stability

- Metric IDs are a public interface. Renaming one is a breaking change.
- `.smr` carries a format version from its first release.

## Open questions

- **License.** Undecided.
