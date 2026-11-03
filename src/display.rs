//! Display model shared by all front ends.

use crate::metric::{MetricId, Unit};

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

#[derive(Clone, Debug)]
pub enum Widget {
    Gauge { label: String, metric: MetricId },
    Series { label: String, metric: MetricId },
}

impl Widget {
    pub fn gauge(label: impl Into<String>, metric: MetricId) -> Self {
        Self::Gauge {
            label: label.into(),
            metric,
        }
    }

    pub fn series(label: impl Into<String>, metric: MetricId) -> Self {
        Self::Series {
            label: label.into(),
            metric,
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
            title: title.into(),
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
    }
}
