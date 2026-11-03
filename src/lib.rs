//! systrace: the core (module host, metric store, display model, export, shared
//! utilities) with the built-in modules and front ends that build on it.
//!
//! Boundaries are enforced by `tests/boundaries.rs`: modules use only `prelude`,
//! `util` and `platform`, and front ends never use `platform` or `modules`.

pub mod display;
pub mod export;
pub mod fmt;
pub mod host;
pub mod layout;
pub mod metric;
pub mod module;
pub mod pace;
pub mod platform;
pub mod store;
pub mod util;

pub mod modules;

#[cfg(feature = "gui")]
pub mod gui;
#[cfg(feature = "tui")]
pub mod tui;

pub mod prelude {
    pub use crate::display::{Level, Panel, Widget};
    pub use crate::metric::{Availability, Capabilities, Detect, Kind, MetricDesc, MetricId, Unit};
    pub use crate::module::{Ctx, Header, Module, Sink};
}
