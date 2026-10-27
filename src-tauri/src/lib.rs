mod diag;
mod i18n_share;
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
mod transfer;

use std::sync::Arc;

use manager::{DeviceView, EventSink, IdentityView, ManagerConfig, ManagerHandle, PairingView};
use plugins::share::{
    DEFAULT_PAYLOAD_PORTS, ShareEnv, ShareNotice, ShareNotifier, SharePlugin, ShareService,
};
use protocol::cert::KeyringStore;
use store::{Settings, SettingsPatch, SettingsStore};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WindowEvent};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;

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

// ---- Dosya aktarımı komutları (Aşama 5) ----

type Share<'a> = State<'a, Arc<ShareService>>;

#[tauri::command]
#[allow(non_snake_case)]
async fn send_files(svc: Share<'_>, deviceId: String, paths: Vec<String>) -> Result<(), String> {
    svc.send_files(&deviceId, paths).map(|_| ())
}

#[tauri::command]
async fn list_transfers(svc: Share<'_>) -> Result<Vec<transfer::Transfer>, String> {
    Ok(svc.list())
}

#[tauri::command]
async fn cancel_transfer(svc: Share<'_>, id: String) -> Result<(), String> {
    svc.cancel_transfer(&id);
    Ok(())
}

#[tauri::command]
async fn retry_transfer(svc: Share<'_>, id: String) -> Result<(), String> {
    svc.retry_transfer(&id)
}

/// Yerel dosya seçici (çoklu seçim). İptalde boş liste.
#[tauri::command]
async fn pick_files(app: AppHandle) -> Result<Vec<String>, String> {
    let picked = app.dialog().file().blocking_pick_files();
    Ok(picked
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    Ok(app
        .dialog()
        .file()
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned()))
}

/// Finder/Explorer'da dosyayı seçili gösterir.
#[tauri::command]
async fn reveal_file(svc: Share<'_>, id: String) -> Result<(), String> {
    let path = svc.file_path(&id).ok_or("dosya bulunamadı")?;
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

/// Alınan dosyayı varsayılan uygulamayla açar (yalnızca gelen ve bitmiş aktarımlar).
#[tauri::command]
async fn open_file(svc: Share<'_>, id: String) -> Result<(), String> {
    if !svc.is_incoming(&id) {
        return Err("yalnızca alınan dosyalar açılır".into());
    }
    let path = svc.file_path(&id).ok_or("dosya bulunamadı")?;
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}

/// Uygulama katmanı: paylaşım bildirimleri (yerel bildirim + pano).
struct AppNotifier {
    app: AppHandle,
    settings: Arc<SettingsStore>,
}

impl AppNotifier {
    fn show(&self, title: String, body: String) {
        if let Err(e) = self
            .app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
        {
            log::warn!("bildirim gösterilemedi: {e}");
        }
    }

    fn set_clipboard(&self, text: &str) {
        // Pano içeriği loglanmaz.
        if let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())) {
            log::warn!("pano yazılamadı: {e}");
        }
    }
}

impl ShareNotifier for AppNotifier {
    fn notify(&self, n: ShareNotice) {
        let lang = i18n_share::resolve(&self.settings.get().language);
        match n {
            ShareNotice::FileReceived {
                device,
                name,
                size,
                folder,
                ..
            } => {
                let (t, b) = i18n_share::file_received(
                    lang,
                    &device,
                    &name,
                    size,
                    &folder.to_string_lossy(),
                );
                self.show(t, b);
            }
            ShareNotice::Text { device, text } => {
                self.set_clipboard(&text);
                let (t, b) = i18n_share::text_received(lang, &device);
                self.show(t, b);
            }
            ShareNotice::Url { device, url } => {
                // Bağlantı kendiliğinden açılmaz; panoya konur ve bildirilir.
                self.set_clipboard(&url);
                let (t, b) = i18n_share::url_received(lang, &device, &url);
                self.show(t, b);
            }
        }
    }
}

/// Protokol yöneticisini Tauri'nin tokio çalışma zamanında başlatır.
fn start_protocol(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let keys = Arc::new(KeyringStore::new(KEYCHAIN_SERVICE, KEYCHAIN_USER));
    // Eklentiler burada kaydedilir.
    let share = ShareService::new();
    let mut registry = plugins::Registry::new();
    registry.register(Arc::new(SharePlugin(share.clone())));
    let cfg = ManagerConfig::production(data_dir, keys, registry);
    let sink = Arc::new(TauriSink(app.clone()));
    let handle = tauri::async_runtime::block_on(manager::start(cfg, sink.clone()))?;

    // İndirme klasörü boşsa işletim sisteminin İndirilenler klasörünü yaz (arayüz yolu gösterir).
    let default_dir = app
        .path()
        .download_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    if handle.get_settings().download_dir.trim().is_empty() {
        handle.set_settings(SettingsPatch {
            download_dir: Some(default_dir.to_string_lossy().into_owned()),
            ..Default::default()
        });
    }
    share.attach(ShareEnv {
        local: handle.local_identity(),
        settings: handle.settings_store(),
        sink,
        notifier: Arc::new(AppNotifier {
            app: app.clone(),
            settings: handle.settings_store(),
        }),
        default_download_dir: default_dir,
        payload_ports: DEFAULT_PAYLOAD_PORTS,
    });
    app.manage(share);
    app.manage(handle);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diag::init();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
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
            app_info,
            send_files,
            list_transfers,
            cancel_transfer,
            retry_transfer,
            pick_files,
            pick_folder,
            reveal_file,
            open_file
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
