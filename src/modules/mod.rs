//! Built-in modules (plugins).
//!
//! To add a module: write `src/modules/<name>.rs`, add a `<name>` feature in
//! Cargo.toml (and to `modules`), then add one line to `modules!` below.

use crate::prelude::Module;

/// Declares each module once: feature, file, type and, if OS-specific, its target.
/// Expands to the gated `mod` and its registration in `all()`.
macro_rules! modules {
    ($($feature:literal => $file:ident::$ty:ident $(if $os:literal)?;)*) => {
        $(
            #[cfg(all(feature = $feature $(, target_os = $os)?))]
            mod $file;
        )*

        /// Every module compiled into this build.
        pub fn all() -> Vec<Box<dyn Module>> {
            #[allow(unused_mut, reason = "a build may compile in no modules")]
            let mut reg: Vec<Box<dyn Module>> = Vec::new();
            $(
                #[cfg(all(feature = $feature $(, target_os = $os)?))]
                reg.push(Box::new($file::$ty::default()));
            )*
            reg
        }
    };
}

modules! {
    "cpu" => cpu::Cpu;
    "memory" => memory::Memory;
    "mem_pressure" => mem_pressure::MemPressure if "macos";
    "disk" => disk::Disk;
    "net" => net::Net;
    "process" => process::Process if "macos";
    "thermals" => thermals::Thermals;
    "gpu" => gpu::Gpu;
}
