//! Safe Rust entry points for the Apple Core Image RAW decoder.
#![cfg_attr(not(target_os = "macos"), forbid(unsafe_code))]

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod native;

#[cfg(target_os = "macos")]
pub use native::{available, decode};
