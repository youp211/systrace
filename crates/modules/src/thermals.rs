//! Temperature sensors via sysinfo, best-effort.
//! Read-only.

use sysinfo::Components;
use systrace_core::prelude::*;
use systrace_core::util::id;

/// Hottest sensor; also the doctor entry when no sensor is exposed.
const MAX: MetricId = MetricId::new("thermals.max");

struct Sensor {
    label: String,
    temp: MetricId,
}

#[derive(Default)]
pub struct Thermals {
    components: Option<Components>,
    sensors: Vec<Sensor>,
}

/// Label without a trailing "temp", since every metric already ends in `.temp`.
fn sensor_name(label: &str) -> &str {
    let trimmed = label.trim_end();
    let lower = trimmed.to_ascii_lowercase();
    let cut = ["temperature", "temp"]
        .iter()
        .find_map(|s| lower.strip_suffix(s))
        .map(|rest| rest.trim_end_matches([' ', '_', '-']).len())
        .filter(|&n| n > 0);
    cut.map_or(trimmed, |n| &trimmed[..n])
}

impl Module for Thermals {
    fn id(&self) -> &'static str {
        "thermals"
    }

    fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
        let components = match Components::new_with_refreshed_list() {
            Ok(c) => c,
            Err(e) => {
                return Capabilities::all(
                    [MAX],
                    Availability::unsupported(format!("cannot list sensors: {e}")),
                );
            }
        };
        let labels: Vec<String> = components
            .iter()
            .filter(|c| c.temperature().is_some())
            .map(|c| c.label().to_string())
            .collect();
        self.sensors = id::segments(labels.iter().map(|l| sensor_name(l)))
            .into_iter()
            .zip(labels)
            .map(|(seg, label)| Sensor {
                label,
                temp: MetricId::owned(format!("thermals.{seg}.temp")),
            })
            .collect();
        self.components = Some(components);

        if self.sensors.is_empty() {
            return Capabilities::all(
                [MAX],
                Availability::unsupported("no temperature sensors exposed"),
            );
        }
        let ids = self.metrics().into_iter().map(|d| d.id);
        Capabilities::all(ids, Availability::Available)
    }

    fn metrics(&self) -> Vec<MetricDesc> {
        let max = MetricDesc::gauge(MAX, Unit::Celsius).detect(Detect::Shift);
        let sensors = self
            .sensors
            .iter()
            .map(|s| MetricDesc::gauge(s.temp.clone(), Unit::Celsius));
        std::iter::once(max).chain(sensors).collect()
    }

    fn sample(&mut self, _ctx: &Ctx, out: &mut Sink) {
        let Some(components) = &mut self.components else {
            return;
        };
        components.refresh(false);

        let mut max = None::<f64>;
        for sensor in &self.sensors {
            let temp = components
                .iter()
                .find(|c| c.label() == sensor.label)
                .and_then(|c| c.temperature());
            if let Some(t) = temp.map(f64::from) {
                out.push(sensor.temp.clone(), t);
                max = Some(max.map_or(t, |m| m.max(t)));
            }
        }
        if let Some(m) = max {
            out.push(MAX, m);
        }
    }

    fn panels(&self) -> Vec<Panel> {
        vec![
            Panel::new("thermals", "Thermals")
                .priority(50)
                .widget(Widget::series("hottest", MAX)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_trailing_temp() {
        assert_eq!(sensor_name("NAND CH0 temp"), "NAND CH0");
        assert_eq!(sensor_name("CPU Temperature "), "CPU");
        assert_eq!(sensor_name("PMU tdie1"), "PMU tdie1");
        assert_eq!(sensor_name("gas gauge battery"), "gas gauge battery");
        assert_eq!(sensor_name("temp"), "temp");
    }
}
