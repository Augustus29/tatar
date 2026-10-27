//! Platforma özel kodlar. Her platformun kendi modülü var; diğerlerinde güvenli yedekler.

pub mod audio;
pub mod clipboard;
pub mod netinfo;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{reduce_transparency, watch_reduce_transparency};

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
pub use other::{reduce_transparency, watch_reduce_transparency};
