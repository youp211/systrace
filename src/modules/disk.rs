//! Disk space and I/O per device via sysinfo.
//! Read-only.

use std::path::{Path, PathBuf};

use sysinfo::{DiskRefreshKind, Disks};

use crate::prelude::*;
use crate::util::id;
use crate::util::rate::Rates;

pub const HEADER: Header = Header {
    id: "disk",
    name: "Disks",
    reads: "sysinfo: per-device space and I/O",
};

/// Module-level entry for doctor when no disk can be listed.
const DISK: MetricId = MetricId::new("disk");

struct Device {
    name: String,
    mount: PathBuf,
    /// Device id at probe; a different device mounted at the same path isn't this one.
    dev: Option<u64>,
    total: MetricId,
    available: MetricId,
    used: MetricId,
    util: MetricId,
    read: MetricId,
    write: MetricId,
}

impl Device {
    fn new(segment: &str, name: String, mount: PathBuf, dev: Option<u64>) -> Self {
        let id = |m: &str| MetricId::owned(format!("disk.{segment}.{m}"));
        Self {
            name,
            mount,
            dev,
            total: id("total"),
            available: id("available"),
            used: id("used"),
            util: id("util"),
            read: id("read"),
            write: id("write"),
        }
    }

    fn descs(&self) -> [MetricDesc; 6] {
        [
            MetricDesc::gauge(self.total.clone(), Unit::Bytes),
            MetricDesc::gauge(self.available.clone(), Unit::Bytes),
            MetricDesc::gauge(self.used.clone(), Unit::Bytes),
            MetricDesc::gauge(self.util.clone(), Unit::Percent).detect(Detect::Shift),
            MetricDesc::counter(self.read.clone(), Unit::BytesPerSec).detect(Detect::Spike),
            MetricDesc::counter(self.write.clone(), Unit::BytesPerSec).detect(Detect::Spike),
        ]
    }
}

#[derive(Default)]
pub struct Disk {
    disks: Option<Disks>,
    devices: Vec<Device>,
    rates: Rates,
}

fn refresh_kind() -> DiskRefreshKind {
    DiskRefreshKind::nothing().with_storage().with_io_usage()
}

/// One mount per device id, preferring a writable one.
/// APFS lists `/` (read-only) and `/System/Volumes/Data` as the same device.
fn pick(mounts: &[(Option<u64>, bool)]) -> Vec<usize> {
    let mut picked: Vec<usize> = Vec::new();
    for (i, &(dev, read_only)) in mounts.iter().enumerate() {
        let same = dev.and_then(|d| picked.iter().position(|&j| mounts[j].0 == Some(d)));
        match same {
            None => picked.push(i),
            Some(p) if mounts[picked[p]].1 && !read_only => picked[p] = i,
            Some(_) => {}
        }
    }
    picked
}

/// Index of the entry that is still the probed device: same mount and, when the
/// device had an id, the same id. Ejected or replaced devices give `None`.
fn locate<'a>(
    entries: impl IntoIterator<Item = (&'a Path, Option<u64>)>,
    mount: &Path,
    dev: Option<u64>,
) -> Option<usize> {
    entries
        .into_iter()
        .position(|(m, id)| m == mount && (dev.is_none() || id == dev))
}

impl Module for Disk {
    fn header(&self) -> &'static Header {
        &HEADER
    }

    fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
        let disks = match Disks::new_with_refreshed_list_specifics(refresh_kind()) {
            Ok(disks) if !disks.is_empty() => disks,
            Ok(_) => return Capabilities::all([DISK], Availability::unsupported("no disks found")),
            Err(e) => {
                return Capabilities::all(
                    [DISK],
                    Availability::unsupported(format!("cannot list disks: {e}")),
                );
            }
        };

        let mounts: Vec<_> = disks.iter().map(|d| (d.id(), d.is_read_only())).collect();
        let mut chosen: Vec<_> = pick(&mounts).into_iter().map(|i| &disks[i]).collect();
        // Sorted, so repeated names get the same `_2` suffix on every run.
        chosen.sort_by(|a, b| (a.name(), a.mount_point()).cmp(&(b.name(), b.mount_point())));
        let names: Vec<_> = chosen.iter().map(|d| d.name().to_string_lossy()).collect();
        self.devices = id::segments(names.iter().map(|n| n.as_ref()))
            .iter()
            .zip(&chosen)
            .zip(&names)
            .map(|((seg, d), name)| {
                Device::new(seg, name.to_string(), d.mount_point().to_path_buf(), d.id())
            })
            .collect();
        self.disks = Some(disks);

        let ids = self.metrics().into_iter().map(|d| d.id);
        Capabilities::all(ids, Availability::Available)
    }

    fn metrics(&self) -> Vec<MetricDesc> {
        self.devices.iter().flat_map(Device::descs).collect()
    }

    fn sample(&mut self, ctx: &Ctx, out: &mut Sink) {
        let Some(disks) = &mut self.disks else { return };
        // `true` drops disks that are gone, so they leave gaps rather than stale values.
        disks.refresh_specifics(true, refresh_kind());

        for dev in &self.devices {
            let entries = disks.iter().map(|d| (d.mount_point(), d.id()));
            let Some(d) = locate(entries, &dev.mount, dev.dev).map(|i| &disks[i]) else {
                continue;
            };
            let (total, available) = (d.total_space() as f64, d.available_space() as f64);
            out.push(dev.total.clone(), total);
            out.push(dev.available.clone(), available);
            out.push(dev.used.clone(), total - available);
            if total > 0.0 {
                out.push(dev.util.clone(), (total - available) / total * 100.0);
            }
            let io = d.usage();
            for (id, total) in [
                (&dev.read, io.total_read_bytes),
                (&dev.write, io.total_written_bytes),
            ] {
                if let Some(rate) = self.rates.update(id, total as f64, ctx.now) {
                    out.push(id.clone(), rate);
                }
            }
        }
    }

    fn panels(&self) -> Vec<Panel> {
        let panel = Panel::new("disk", "Disks").priority(70);
        vec![self.devices.iter().fold(panel, |p, d| {
            p.widget(Widget::gauge(format!("{} used", d.name), d.util.clone()))
                .widget(Widget::series(format!("{} read", d.name), d.read.clone()))
                .widget(Widget::series(format!("{} write", d.name), d.write.clone()))
        })]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_one_mount_per_device() {
        // From an Apple Silicon Mac: `/` and its Data volume share device 16777234.
        let mounts = [
            (Some(16777234), true),
            (Some(16777234), false),
            (Some(16777240), false),
            (None, false),
            (None, true),
        ];
        assert_eq!(pick(&mounts), [1, 2, 3, 4]);
        assert_eq!(pick(&[(Some(1), false), (Some(1), true)]), [0]);
    }

    #[test]
    fn locates_only_the_probed_device() {
        let usb = Path::new("/Volumes/USB");
        let now = [(Path::new("/"), Some(1)), (usb, Some(7))];
        assert_eq!(locate(now, usb, Some(7)), Some(1));
        // A different stick at the same path, or no stick at all.
        assert_eq!(locate([(usb, Some(8))], usb, Some(7)), None);
        assert_eq!(locate([(Path::new("/"), Some(1))], usb, Some(7)), None);
        // Without an id, the mount alone identifies it.
        assert_eq!(locate([(usb, None)], usb, None), Some(0));
    }
}
