//! Safe wrappers over OS APIs that sysinfo doesn't cover. All `unsafe` lives here:
//! the crate denies `unsafe_code`, and only these modules allow it.

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub mod macos;
