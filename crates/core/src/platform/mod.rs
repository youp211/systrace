//! Safe wrappers over OS APIs that sysinfo doesn't cover. All `unsafe` lives here.

#[cfg(target_os = "macos")]
pub mod macos;
