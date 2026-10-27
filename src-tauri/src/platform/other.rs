//! macOS ve Windows dışı yedek (Linux hedef değil ama derlenebilir kalsın).

use tauri::AppHandle;

pub fn reduce_transparency() -> bool {
    false
}

pub fn watch_reduce_transparency(_app: &AppHandle) {}

pub fn apply_window_material(_app: &AppHandle) -> bool {
    false
}

pub fn window_material_active() -> bool {
    false
}
