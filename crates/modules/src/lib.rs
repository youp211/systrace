//! Built-in systrace modules.
//!
//! Each module is gated by its cargo feature and, when OS-specific, its target:
//! `#[cfg(all(feature = "x", target_os = "macos"))]` on both the `mod` and the push.

use systrace_core::module::Module;

#[cfg(feature = "cpu")]
mod cpu;
#[cfg(feature = "disk")]
mod disk;
#[cfg(feature = "gpu")]
mod gpu;
#[cfg(all(feature = "mem_pressure", target_os = "macos"))]
mod mem_pressure;
#[cfg(feature = "memory")]
mod memory;
#[cfg(feature = "net")]
mod net;
#[cfg(all(feature = "process", target_os = "macos"))]
mod process;
#[cfg(feature = "thermals")]
mod thermals;

/// Every module compiled into this build.
#[allow(clippy::vec_init_then_push, reason = "each push is feature/OS gated")]
#[allow(unused_mut, reason = "a build may compile in no modules")]
pub fn all() -> Vec<Box<dyn Module>> {
    let mut reg: Vec<Box<dyn Module>> = Vec::new();

    #[cfg(feature = "cpu")]
    reg.push(Box::new(cpu::Cpu::default()));
    #[cfg(feature = "memory")]
    reg.push(Box::new(memory::Memory::default()));
    #[cfg(all(feature = "mem_pressure", target_os = "macos"))]
    reg.push(Box::new(mem_pressure::MemPressure));
    #[cfg(feature = "disk")]
    reg.push(Box::new(disk::Disk::default()));
    #[cfg(feature = "net")]
    reg.push(Box::new(net::Net::default()));
    #[cfg(all(feature = "process", target_os = "macos"))]
    reg.push(Box::new(process::Process));
    #[cfg(feature = "thermals")]
    reg.push(Box::new(thermals::Thermals::default()));
    #[cfg(feature = "gpu")]
    reg.push(Box::new(gpu::Gpu::default()));

    reg
}
