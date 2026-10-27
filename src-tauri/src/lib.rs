mod diag;
#[allow(dead_code)] // sonraki aşamalar kullanacak API
mod manager;
mod platform;
#[allow(dead_code)] // sonraki aşamalar kullanacak API
mod plugins;
mod popover;
#[allow(dead_code)] // sonraki aşamalar kullanacak API
mod protocol;
#[allow(dead_code)] // sonraki aşamalar kullanacak API
mod store;

use std::sync::Arc;

use manager::{DeviceView, EventSink, IdentityView, ManagerConfig, ManagerHandle, PairingView};
use protocol::cert::KeyringStore;
use store::{Settings, SettingsPatch};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WindowEvent};

/// Rust -> arayüz olayları Tauri olayı olarak gider.
struct TauriSink(AppHandle);

impl EventSink for TauriSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        let _ = self.0.emit(event, payload);
    }
}

const KEYCHAIN_SERVICE: &str = "io.github.augustus29.tatar";
const KEYCHAIN_USER: &str = "device-private-key";

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

// ---- Protokol komutları (sözleşme: src/lib/api.ts) ----

#[tauri::command]
async fn list_devices(mgr: State<'_, ManagerHandle>) -> Result<Vec<DeviceView>, String> {
    Ok(mgr.list_devices().await)
}

#[tauri::command]
#[allow(non_snake_case)]
async fn request_pair(
    mgr: State<'_, ManagerHandle>,
    deviceId: String,
) -> Result<PairingView, String> {
    mgr.request_pair(&deviceId).await
}

#[tauri::command]
#[allow(non_snake_case)]
fn accept_pair(mgr: State<'_, ManagerHandle>, deviceId: String) {
    mgr.accept_pair(&deviceId);
}

/// Gelen isteği reddeder ya da giden isteği iptal eder.
#[tauri::command]
#[allow(non_snake_case)]
fn reject_pair(mgr: State<'_, ManagerHandle>, deviceId: String) {
    mgr.reject_pair(&deviceId);
}

#[tauri::command]
#[allow(non_snake_case)]
fn unpair(mgr: State<'_, ManagerHandle>, deviceId: String) {
    mgr.unpair(&deviceId);
}

#[tauri::command]
fn add_device_by_ip(mgr: State<'_, ManagerHandle>, ip: String) -> Result<(), String> {
    mgr.add_device_by_ip(&ip)
}

#[tauri::command]
fn get_settings(mgr: State<'_, ManagerHandle>) -> Settings {
    mgr.get_settings()
}

/// Yamayı uygular, birleştirilmiş ayarları döndürür ve `settings-changed` yayar.
#[tauri::command]
fn set_settings(app: AppHandle, mgr: State<'_, ManagerHandle>, patch: SettingsPatch) -> Settings {
    let s = mgr.set_settings(patch);
    let _ = app.emit("settings-changed", &s);
    s
}

#[tauri::command]
fn get_identity(mgr: State<'_, ManagerHandle>) -> IdentityView {
    mgr.get_identity()
}

#[tauri::command]
fn get_diagnostic_log() -> String {
    diag::recent()
}

#[tauri::command]
fn app_info(app: AppHandle) -> serde_json::Value {
    serde_json::json!({ "version": app.package_info().version.to_string() })
}

/// Protokol yöneticisini Tauri'nin tokio çalışma zamanında başlatır.
fn start_protocol(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let keys = Arc::new(KeyringStore::new(KEYCHAIN_SERVICE, KEYCHAIN_USER));
    // Eklentiler sonraki aşamalarda burada kaydedilir.
    let registry = plugins::Registry::new();
    let cfg = ManagerConfig::production(data_dir, keys, registry);
    let handle =
        tauri::async_runtime::block_on(manager::start(cfg, Arc::new(TauriSink(app.clone()))))?;
    app.manage(handle);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diag::init();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            show_main_window,
            hide_popover,
            resize_popover,
            quit_app,
            reduce_transparency,
            list_devices,
            request_pair,
            accept_pair,
            reject_pair,
            unpair,
            add_device_by_ip,
            get_settings,
            set_settings,
            get_identity,
            get_diagnostic_log,
            app_info
        ])
        .setup(|app| {
            popover::setup(app.handle())?;
            platform::watch_reduce_transparency(app.handle());
            start_protocol(app.handle())?;
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
