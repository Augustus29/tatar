//! macOS'a özel kodlar: "Saydamlığı azalt" erişilebilirlik ayarı.

use block2::RcBlock;
use objc2_app_kit::{NSWorkspace, NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification};
use objc2_foundation::NSNotification;
use std::ptr::NonNull;
use tauri::{AppHandle, Emitter};

/// Sistem ayarlarında "Saydamlığı azalt" açık mı?
pub fn reduce_transparency() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency()
}

/// Ayar değişince arayüze `reduce-transparency-changed` olayı gönderir.
pub fn watch_reduce_transparency(app: &AppHandle) {
    let app = app.clone();
    let block = RcBlock::new(move |_note: NonNull<NSNotification>| {
        let _ = app.emit("reduce-transparency-changed", reduce_transparency());
    });
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    // Gözlemci uygulama kapanana dek yaşar; dönen nesneyi bilerek bırakıyoruz.
    let token = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
            None,
            None,
            &block,
        )
    };
    std::mem::forget(token);
}

/// macOS'ta pencere malzemesi `tauri.conf.json` / popover üzerinden verilir; arayüzün ek bir şey
/// yapmasına gerek yok.
pub fn apply_window_material(_app: &AppHandle) -> bool {
    false
}

pub fn window_material_active() -> bool {
    false
}
