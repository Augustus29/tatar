//! Platforma özel kodlar. Her platformun kendi modülü var; diğerlerinde güvenli yedekler.

pub mod audio;
pub mod clipboard;
pub mod netinfo;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{
    apply_window_material, reduce_transparency, watch_reduce_transparency, window_material_active,
};

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::{
    apply_window_material, reduce_transparency, watch_reduce_transparency, window_material_active,
};

#[cfg(not(any(target_os = "macos", windows)))]
mod other;
#[cfg(not(any(target_os = "macos", windows)))]
pub use other::{
    apply_window_material, reduce_transparency, watch_reduce_transparency, window_material_active,
};
