//! Display model shared by all front ends.

use crate::metric::{MetricId, Unit};
use crate::util::text::printable;

/// How close a bar is to full, so every front end colors it the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Normal,
    High,
    Critical,
}

impl Level {
    pub fn of(ratio: f64) -> Self {
        match ratio {
            r if r >= 0.9 => Self::Critical,
            r if r >= 0.7 => Self::High,
            _ => Self::Normal,
        }
    }
}

/// Fixed top of the scale for units that have one; other units scale to their data.
pub fn full_scale(unit: Unit) -> Option<f64> {
    (unit == Unit::Percent).then_some(100.0)
}

/// How full a bar is, 0 to 1, for units with a fixed full scale.
pub fn ratio(value: f64, unit: Unit) -> Option<f64> {
    full_scale(unit).map(|full| (value / full).clamp(0.0, 1.0))
}

/// Sparkline heights, 0 to 1: against the unit's full scale when it has one,
/// otherwise against the largest value shown. Gaps stay `None`.
pub fn spark_heights(values: &[Option<f64>], unit: Option<Unit>) -> Vec<Option<f64>> {
    let top = unit
        .and_then(full_scale)
        .unwrap_or_else(|| values.iter().flatten().fold(0.0, |m: f64, v| m.max(*v)));
    values
        .iter()
        .map(|v| {
            v.map(|v| {
                if top > 0.0 {
                    (v / top).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
pub enum Widget {
    /// A bar for units with a full scale. `higher_is_better` flips its coloring
    /// for metrics where a full bar is healthy, such as free memory.
    Gauge {
        label: String,
        metric: MetricId,
        higher_is_better: bool,
    },
    Series {
        label: String,
        metric: MetricId,
    },
}

impl Widget {
    pub fn gauge(label: impl Into<String>, metric: MetricId) -> Self {
        Self::Gauge {
            label: clean(label.into()),
            metric,
            higher_is_better: false,
        }
    }

    pub fn series(label: impl Into<String>, metric: MetricId) -> Self {
        Self::Series {
            label: clean(label.into()),
            metric,
        }
    }

    /// Marks a gauge where fuller is healthier.
    pub fn higher_is_better(self) -> Self {
        match self {
            Self::Gauge { label, metric, .. } => Self::Gauge {
                label,
                metric,
                higher_is_better: true,
            },
            series => series,
        }
    }

    /// Color level for a bar filled to `ratio`.
    pub fn level(&self, ratio: f64) -> Level {
        match self {
            Self::Gauge {
                higher_is_better: true,
                ..
            } => Level::of(1.0 - ratio),
            _ => Level::of(ratio),
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Gauge { label, .. } | Self::Series { label, .. } => label,
        }
    }

    pub fn metric(&self) -> &MetricId {
        match self {
            Self::Gauge { metric, .. } | Self::Series { metric, .. } => metric,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Panel {
    pub id: &'static str,
    pub title: String,
    pub priority: u8,
    pub widgets: Vec<Widget>,
}

impl Panel {
    pub fn new(id: &'static str, title: impl Into<String>) -> Self {
        Self {
            id,
            title: clean(title.into()),
            priority: 50,
            widgets: vec![],
        }
    }

    pub fn priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn widget(mut self, widget: Widget) -> Self {
        self.widgets.push(widget);
        self
    }
}

/// Labels and titles can carry OS-provided names; keep control characters out.
fn clean(text: String) -> String {
    match printable(&text) {
        std::borrow::Cow::Borrowed(_) => text,
        std::borrow::Cow::Owned(safe) => safe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_scale() {
        assert_eq!(Level::of(0.5), Level::Normal);
        assert_eq!(Level::of(0.7), Level::High);
        assert_eq!(Level::of(0.95), Level::Critical);
        assert_eq!(full_scale(Unit::Percent), Some(100.0));
        assert_eq!(full_scale(Unit::Bytes), None);
        assert_eq!(ratio(150.0, Unit::Percent), Some(1.0));
        assert_eq!(ratio(5.0, Unit::Bytes), None);
    }

    #[test]
    fn higher_is_better_flips_the_color() {
        const FREE: MetricId = MetricId::new("test.free");
        let used = Widget::gauge("used", FREE);
        let free = Widget::gauge("free", FREE).higher_is_better();
        assert_eq!(used.level(0.95), Level::Critical);
        assert_eq!(free.level(0.95), Level::Normal);
        assert_eq!(free.level(0.05), Level::Critical);
        assert_eq!(
            Widget::series("s", FREE).higher_is_better().level(0.95),
            Level::Critical
        );
    }

    #[test]
    fn labels_and_titles_lose_control_characters() {
        const M: MetricId = MetricId::new("test.m");
        let w = Widget::series("\x1b]0;pwned\x07disk", M);
        let p = Panel::new("p", "\x1b[2JTitle");
        assert!(!w.label().chars().any(char::is_control));
        assert!(!p.title.chars().any(char::is_control));
        assert!(w.label().ends_with("disk") && p.title.ends_with("Title"));
    }

    #[test]
    fn spark_heights_scale_and_keep_gaps() {
        let heights = spark_heights(&[Some(50.0), None, Some(100.0)], Some(Unit::Percent));
        assert_eq!(heights, [Some(0.5), None, Some(1.0)]);
        let heights = spark_heights(&[Some(2.0), Some(4.0)], Some(Unit::Bytes));
        assert_eq!(heights, [Some(0.5), Some(1.0)]);
        assert_eq!(spark_heights(&[Some(0.0)], None), [Some(0.0)]);
        assert_eq!(
            spark_heights(&[Some(3e16)], Some(Unit::BytesPerSec)),
            [Some(1.0)]
        );
    }
}
