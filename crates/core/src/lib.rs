//! systrace core: module host, metric store, display model, export and shared utilities.

pub mod display;
pub mod export;
pub mod fmt;
pub mod host;
pub mod layout;
pub mod metric;
pub mod module;
pub mod platform;
pub mod store;
pub mod util;

pub mod prelude {
    pub use crate::display::{Panel, Widget};
    pub use crate::metric::{Availability, Capabilities, Detect, Kind, MetricDesc, MetricId, Unit};
    pub use crate::module::{Ctx, Module, Sink};
}
