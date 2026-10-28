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

#[cfg(target_os = "macos")]
pub mod notify_macos;

/// Sistem bildirimi: macOS'ta UNUserNotificationCenter, diğer platformlarda Tauri eklentisi.
pub fn notify(app: &tauri::AppHandle, title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        if !notify_macos::show(title, body) {
            // Eski NSUserNotification yolu (tauri eklentisi) macOS 26'da banner göstermez;
            // yetkilendirme yokken osascript kullanılır.
            notify_macos::osascript(title, body);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri_plugin_notification::NotificationExt;
        if let Err(e) = app.notification().builder().title(title).body(body).show() {
            log::warn!("bildirim gösterilemedi: {e}");
        }
    }
}
