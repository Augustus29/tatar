mod platform;
mod popover;

use tauri::{AppHandle, Manager, RunEvent, WindowEvent};

const MAIN_LABEL: &str = "main";

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Ana pencereyi öne getirir (popover'daki "Tatar'ı aç").
#[tauri::command]
fn show_main_window(app: AppHandle) {
    popover::hide(&app);
    show_main(&app);
}

#[tauri::command]
fn hide_popover(app: AppHandle) {
    popover::hide(&app);
}

/// Popover içeriğinin yüksekliği değişince pencereyi uyarlar.
#[tauri::command]
fn resize_popover(app: AppHandle, height: f64) {
    popover::resize(&app, height);
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Sistemde "Saydamlığı azalt" açık mı?
#[tauri::command]
fn reduce_transparency() -> bool {
    platform::reduce_transparency()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            show_main_window,
            hide_popover,
            resize_popover,
            quit_app,
            reduce_transparency
        ])
        .setup(|app| {
            popover::setup(app.handle())?;
            platform::watch_reduce_transparency(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Ana pencere kapatılınca uygulama arka planda çalışmaya devam eder.
            if window.label() == MAIN_LABEL
                && let WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("Tatar başlatılamadı");

    app.run(|app, event| {
        // Dock simgesine tıklanınca pencere yeniden açılsın.
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen { .. } = event {
            show_main(app);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app, event);
    });
}
