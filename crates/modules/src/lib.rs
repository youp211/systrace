//! Built-in sysmon modules.

use sysmon_core::module::Module;

#[cfg(feature = "cpu")]
mod cpu;

/// Every module compiled into this build.
#[allow(clippy::vec_init_then_push, reason = "each push is feature/OS gated")]
pub fn all() -> Vec<Box<dyn Module>> {
    let mut reg: Vec<Box<dyn Module>> = Vec::new();

    #[cfg(feature = "cpu")]
    reg.push(Box::new(cpu::Cpu::default()));

    reg
}
