//! sysmon command-line interface.

mod commands;

use std::io::{self, IsTerminal};
use std::process::ExitCode;
use std::time::Duration;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use sysmon_core::export::Format;

#[derive(Parser)]
#[command(
    name = "sysmon",
    version,
    about = "Read-only system performance monitor"
)]
struct Cli {
    /// Output format (default: table on a terminal, ndjson when piped)
    #[arg(short = 'f', long, global = true, value_enum)]
    format: Option<FormatArg>,

    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show what this machine can measure, with fix hints
    Doctor,
    /// List loaded modules and their status
    Modules,
    /// List the metric catalog
    Metrics {
        /// Selectors: `cpu`, `cpu.core.*.util`
        selectors: Vec<String>,
    },
    /// Take one sample and print it
    Snapshot {
        /// Selectors: `cpu`, `cpu.core.*.util`
        selectors: Vec<String>,
        /// Time between the warm-up and measured sample
        #[arg(short, long, default_value = "500ms", value_parser = parse_duration)]
        interval: Duration,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum FormatArg {
    Table,
    Json,
    Ndjson,
    Csv,
}

impl From<FormatArg> for Format {
    fn from(f: FormatArg) -> Self {
        match f {
            FormatArg::Table => Format::Table,
            FormatArg::Json => Format::Json,
            FormatArg::Ndjson => Format::Ndjson,
            FormatArg::Csv => Format::Csv,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let format = cli.format.map(Format::from).unwrap_or_else(default_format);

    let result = match cli.cmd {
        // The TUI takes over the no-argument case in Phase 1.
        None => Cli::command().print_help(),
        Some(Cmd::Doctor) => commands::doctor(format),
        Some(Cmd::Modules) => commands::modules(format),
        Some(Cmd::Metrics { selectors }) => commands::metrics(&selectors, format),
        Some(Cmd::Snapshot {
            selectors,
            interval,
        }) => commands::snapshot(&selectors, interval, format),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sysmon: error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn default_format() -> Format {
    if io::stdout().is_terminal() {
        Format::Table
    } else {
        Format::Ndjson
    }
}

/// Duration parser: `250ms`, `2s`, `5m`.
fn parse_duration(s: &str) -> Result<Duration, String> {
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: u64 = num.parse().map_err(|_| format!("invalid duration `{s}`"))?;
    match unit {
        "ms" => Ok(Duration::from_millis(n)),
        "s" => Ok(Duration::from_secs(n)),
        "m" => Ok(Duration::from_secs(n * 60)),
        _ => Err(format!("invalid duration `{s}` (use ms, s or m)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_durations() {
        assert_eq!(parse_duration("250ms"), Ok(Duration::from_millis(250)));
        assert_eq!(parse_duration("2s"), Ok(Duration::from_secs(2)));
        assert_eq!(parse_duration("5m"), Ok(Duration::from_secs(300)));
        assert!(parse_duration("5").is_err());
        assert!(parse_duration("ms").is_err());
    }

    #[test]
    fn cli_is_valid() {
        Cli::command().debug_assert();
    }
}
