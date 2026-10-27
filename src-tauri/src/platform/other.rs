//! macOS dışı yedek. Windows'ta "Saydamlık efektleri" kaydı Aşama 8'de okunacak.

use tauri::AppHandle;

pub fn reduce_transparency() -> bool {
    false
}

pub fn watch_reduce_transparency(_app: &AppHandle) {}
