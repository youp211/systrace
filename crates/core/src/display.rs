//! Display model shared by all front ends.

use crate::metric::MetricId;

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
