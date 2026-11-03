//! Per-interface network throughput, packets and errors via sysinfo.
//! Read-only.

use sysinfo::{InterfaceOperationalState, Networks};
use crate::prelude::*;
use crate::util::id;
use crate::util::rate::Rates;

pub const HEADER: Header = Header {
    id: "net",
    name: "Network",
    reads: "sysinfo: per-interface bytes, packets, errors",
};

/// Module-level entry for doctor when no interface is tracked.
const NET: MetricId = MetricId::new("net");

struct Interface {
    name: String,
    /// rx bytes, tx bytes, rx packets, tx packets, rx errors, tx errors.
    ids: [MetricId; 6],
}

const SUFFIXES: [(&str, Unit); 6] = [
    ("rx", Unit::BytesPerSec),
    ("tx", Unit::BytesPerSec),
    ("rx_packets", Unit::PerSec),
    ("tx_packets", Unit::PerSec),
    ("rx_errors", Unit::PerSec),
    ("tx_errors", Unit::PerSec),
];

#[derive(Default)]
pub struct Net {
    networks: Option<Networks>,
    interfaces: Vec<Interface>,
    rates: Rates,
}

/// Up, not loopback, and has carried traffic since boot; skips idle virtual interfaces.
fn tracked(name: &str, up: bool, total_bytes: u64) -> bool {
    up && !name.starts_with("lo") && total_bytes > 0
}

impl Module for Net {
    fn header(&self) -> &'static Header {
        &HEADER
    }

    fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
        let networks = match Networks::new_with_refreshed_list() {
            Ok(n) => n,
            Err(e) => {
                return Capabilities::all(
                    [NET],
                    Availability::unsupported(format!("cannot list interfaces: {e}")),
                );
            }
        };

        let mut names: Vec<String> = networks
            .iter()
            .filter(|(name, data)| {
                let up = data.operational_state() == InterfaceOperationalState::Up;
                let bytes = data.total_received() + data.total_transmitted();
                tracked(&name.to_string_lossy(), up, bytes)
            })
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect();
        names.sort();
        self.interfaces = id::segments(names.iter().map(String::as_str))
            .into_iter()
            .zip(names)
            .map(|(seg, name)| Interface {
                name,
                ids: SUFFIXES.map(|(s, _)| MetricId::owned(format!("net.{seg}.{s}"))),
            })
            .collect();
        self.networks = Some(networks);

        if self.interfaces.is_empty() {
            return Capabilities::all(
                [NET],
                Availability::unsupported("no active interface has carried traffic"),
            );
        }
        let ids = self.metrics().into_iter().map(|d| d.id);
        Capabilities::all(ids, Availability::Available)
    }

    fn metrics(&self) -> Vec<MetricDesc> {
        self.interfaces
            .iter()
            .flat_map(|i| {
                i.ids.iter().zip(SUFFIXES).map(|(id, (_, unit))| {
                    MetricDesc::counter(id.clone(), unit).detect(Detect::Spike)
                })
            })
            .collect()
    }

    fn sample(&mut self, ctx: &Ctx, out: &mut Sink) {
        let Some(networks) = &mut self.networks else {
            return;
        };
        networks.refresh(false);

        for iface in &self.interfaces {
            let Some(d) = networks.get(std::ffi::OsStr::new(&iface.name)) else {
                continue;
            };
            let totals = [
                d.total_received(),
                d.total_transmitted(),
                d.total_packets_received(),
                d.total_packets_transmitted(),
                d.total_errors_on_received(),
                d.total_errors_on_transmitted(),
            ];
            for (id, total) in iface.ids.iter().zip(totals) {
                if let Some(rate) = self.rates.update(id, total as f64, ctx.now) {
                    out.push(id.clone(), rate);
                }
            }
        }
    }

    fn panels(&self) -> Vec<Panel> {
        let panel = Panel::new("net", "Network").priority(75);
        vec![self.interfaces.iter().fold(panel, |p, i| {
            p.widget(Widget::series(format!("{} rx", i.name), i.ids[0].clone()))
                .widget(Widget::series(format!("{} tx", i.name), i.ids[1].clone()))
        })]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_active_non_loopback() {
        assert!(tracked("en0", true, 1));
        assert!(!tracked("en0", false, 1));
        assert!(!tracked("lo0", true, 1));
        assert!(!tracked("awdl0", true, 0));
    }
}
