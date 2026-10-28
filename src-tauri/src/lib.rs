mod diag;
mod i18n;
mod i18n_share;
#[allow(dead_code)] // sonraki aşamalar kullanacak API
mod manager;
mod platform;
#[allow(dead_code)] // sonraki aşamalar kullanacak API
mod plugins;
mod popover;
#[allow(dead_code)] // sonraki aşamalar kullanacak API
mod protocol;
mod shortcuts;
#[allow(dead_code)] // sonraki aşamalar kullanacak API
mod store;
mod transfer;
mod troubleshoot;

use std::sync::Arc;

use manager::{DeviceView, EventSink, IdentityView, ManagerConfig, ManagerHandle, PairingView};
use plugins::share::{
    DEFAULT_PAYLOAD_PORTS, ShareEnv, ShareNotice, ShareNotifier, SharePlugin, ShareService,
};
use protocol::cert::KeyringStore;
use store::{Settings, SettingsPatch, SettingsStore};
#[cfg(target_os = "macos")]
use tauri::RunEvent;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_dialog::DialogExt;

/// Rust -> arayüz olayları Tauri olayı olarak gider.
struct TauriSink(AppHandle);

impl EventSink for TauriSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        if event == "pair-request" {
            self.announce_pair_request(&payload);
        }
        let _ = self.0.emit(event, payload);
    }
}

impl TauriSink {
    /// Telefondan eşleştirme isteği gelince pencere gizli olsa da kullanıcı görsün: ana pencere
    /// öne gelir ve doğrulama kodu bildirimle de gösterilir.
    fn announce_pair_request(&self, payload: &serde_json::Value) {
        let field = |k: &str| payload.get(k).and_then(|v| v.as_str()).unwrap_or_default();
        let lang = self
            .0
            .try_state::<ManagerHandle>()
            .map(|m| i18n::resolve_language(&m.get_settings().language))
            .unwrap_or_else(|| i18n::resolve_language("system"));
        let body = i18n::notify_text(
            lang,
            "pairBody",
            &[
                ("device", field("deviceName")),
                ("code", field("verificationKey")),
            ],
        );
        plugins::notifier::Notifier::notify(
            &TauriNotifier(self.0.clone()),
            &i18n::notify_text(lang, "pairTitle", &[]),
            &body,
        );
        show_main(&self.0);
    }
}

/// Sistem bildirimlerini Tauri bildirim eklentisiyle gönderir.
struct TauriNotifier(AppHandle);

impl plugins::notifier::Notifier for TauriNotifier {
    fn notify(&self, title: &str, body: &str) {
        platform::notify(&self.0, title, body);
    }
}

const KEYCHAIN_SERVICE: &str = "io.github.augustus29.tatar";
const KEYCHAIN_USER: &str = "device-private-key";

const MAIN_LABEL: &str = "main";
/// Oturum açılışında başlatılırken verilen argüman (ana pencere gizli kalır).
const AUTOSTART_ARG: &str = "--hidden";

pub(crate) fn show_main(app: &AppHandle) {
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

/// Pencerede Windows 11 Mica zemini etkin mi? (Etkinse arayüz zemini hafif saydam olur.)
#[tauri::command]
fn window_material() -> bool {
    platform::window_material_active()
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

/// Telefona ping gönderir.
#[tauri::command]
#[allow(non_snake_case)]
async fn ping_device(mgr: State<'_, ManagerHandle>, deviceId: String) -> Result<(), String> {
    mgr.send_packet(&deviceId, plugins::ping::ping_packet())
        .await
}

/// Telefonu çaldırır (`findmyphone`); tekrar çağrı da aynı paketi yollar.
#[tauri::command]
#[allow(non_snake_case)]
async fn ring_device(mgr: State<'_, ManagerHandle>, deviceId: String) -> Result<(), String> {
    mgr.send_packet(&deviceId, plugins::findmyphone::ring_packet())
        .await
}

#[tauri::command]
fn get_settings(mgr: State<'_, ManagerHandle>) -> Settings {
    mgr.get_settings()
}

/// Yamayı uygular, değişikliklerin sistem tarafındaki etkisini (açılışta başlat, Dock, tepsi)
/// uygular, birleştirilmiş ayarları döndürür ve `settings-changed` yayar.
/// Cihaz adı değişince yönetici kimliği yeniden duyurur (`ManagerHandle::set_settings`).
#[tauri::command]
fn set_settings(app: AppHandle, mgr: State<'_, ManagerHandle>, patch: SettingsPatch) -> Settings {
    let before = mgr.get_settings();
    let mut s = mgr.set_settings(patch);
    if let Some(actual) = apply_settings_effects(&app, Some(&before), &s) {
        // Açılışta başlatma ayarlanamadıysa kayıtlı değeri gerçek duruma çek.
        s = mgr.set_settings(SettingsPatch {
            launch_at_login: Some(actual),
            ..Default::default()
        });
    }
    // Komut listesi değiştiyse bağlı telefonlara yeni listeyi yolla.
    if before.commands != s.commands
        && let Some(rc) = app.try_state::<Arc<plugins::runcommand::RunCommandPlugin>>()
    {
        rc.send_to_all();
    }
    let _ = app.emit("settings-changed", &s);
    s
}

/// Dock ve menü çubuğu simgesi görünürlük planı.
#[derive(Debug, PartialEq, Eq)]
struct UiPlan {
    /// macOS'ta Dock simgesi (etkinleştirme ilkesi Regular/Accessory)
    dock: bool,
    tray: bool,
}

/// Ayarlardan görünürlük planı çıkarır. Uygulamaya ulaşmanın tek yolu kalmasın diye Dock
/// gizliyken menü çubuğu simgesi zorla görünür; Windows'ta tepsi simgesi hep görünür
/// (pencere kapatılınca uygulamaya ulaşmanın yolu o).
fn ui_plan(s: &Settings, mac: bool) -> UiPlan {
    if mac {
        UiPlan {
            dock: s.show_in_dock,
            tray: s.show_in_menu_bar || !s.show_in_dock,
        }
    } else {
        UiPlan {
            dock: true,
            tray: true,
        }
    }
}

/// Açılışta başlatmayı istenen duruma getirir. Başarısızsa gerçek durumu döndürür.
fn sync_autostart(app: &AppHandle, want: bool) -> Option<bool> {
    let manager = app.autolaunch();
    match manager.is_enabled() {
        Ok(cur) if cur == want => None,
        Ok(_) => {
            let r = if want {
                manager.enable()
            } else {
                manager.disable()
            };
            match r {
                Ok(()) => None,
                Err(e) => {
                    log::warn!("açılışta başlatma ayarlanamadı: {e}");
                    Some(manager.is_enabled().unwrap_or(false))
                }
            }
        }
        Err(e) => {
            log::warn!("açılışta başlatma durumu okunamadı: {e}");
            None
        }
    }
}

fn apply_ui_plan(app: &AppHandle, s: &Settings) {
    let plan = ui_plan(s, cfg!(target_os = "macos"));
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        let policy = if plan.dock {
            ActivationPolicy::Regular
        } else {
            ActivationPolicy::Accessory
        };
        if let Err(e) = app.set_activation_policy(policy) {
            log::warn!("Dock ayarı uygulanamadı: {e}");
        }
        // İlke değişince pencere arkada kalabilir; açıksa öne al.
        if let Some(w) = app.get_webview_window(MAIN_LABEL)
            && w.is_visible().unwrap_or(false)
        {
            let _ = w.set_focus();
        }
    }
    if let Some(tray) = app.tray_by_id("main")
        && let Err(e) = tray.set_visible(plan.tray)
    {
        log::warn!("menü çubuğu simgesi ayarlanamadı: {e}");
    }
}

/// Ayar değişikliklerinin sistemdeki karşılığını uygular. `before = None` açılıştaki tam eşitleme.
/// Açılışta başlatma ayarlanamadıysa gerçek değeri döndürür.
fn apply_settings_effects(
    app: &AppHandle,
    before: Option<&Settings>,
    now: &Settings,
) -> Option<bool> {
    let ui_changed = before.is_none_or(|b| {
        b.show_in_dock != now.show_in_dock || b.show_in_menu_bar != now.show_in_menu_bar
    });
    if ui_changed {
        apply_ui_plan(app, now);
    }
    if before.is_none_or(|b| b.launch_at_login != now.launch_at_login) {
        return sync_autostart(app, now.launch_at_login);
    }
    None
}

// ---- Sorun giderme ve sistem ayarları (Aşama 7) ----

/// 5 gerçek denetimi çalıştırır (sıra: wifi, localNetwork, firewall, discovery, phoneApp).
/// Önce kimlik yeniden yayınlanır; telefonlar buna bağlanarak yanıt verdiğinden 1,5 sn beklenir.
#[tauri::command]
async fn run_troubleshoot(
    mgr: State<'_, ManagerHandle>,
) -> Result<Vec<troubleshoot::CheckResult>, String> {
    use troubleshoot::{DeviceFacts, Firewall, Inputs, LocalNet};
    let handle = mgr.inner().clone();
    handle.reannounce();
    let probe_handle = handle.clone();
    let probe = tauri::async_runtime::spawn_blocking(move || probe_handle.probe_local_network())
        .await
        .ok()
        .flatten();
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    let wifi = tauri::async_runtime::spawn_blocking(platform::netinfo::wifi_facts)
        .await
        .map_err(|e| e.to_string())?;
    // `netsh` çok kurallı makinelerde yavaş olabilir; sınırlı bekle.
    let firewall = match tokio::time::timeout(
        std::time::Duration::from_secs(8),
        tauri::async_runtime::spawn_blocking(platform::netinfo::firewall_state),
    )
    .await
    {
        Ok(Ok(f)) => f,
        _ => Firewall::Unknown,
    };
    let local_net = if cfg!(target_os = "macos") && probe == Some(false) {
        LocalNet::Denied
    } else {
        platform::netinfo::local_network_default()
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let views = handle.list_devices().await;
    let tuples: Vec<_> = views
        .iter()
        .map(|d| (d.paired, d.reachable, d.last_seen))
        .collect();
    let inputs = Inputs {
        wifi,
        net: handle.net_stats().snapshot(),
        tcp_listening: handle.tcp_port() != 0,
        local_net,
        firewall,
        devices: DeviceFacts::from_devices(&tuples, now),
    };
    Ok(troubleshoot::assemble(&inputs, now))
}

/// Sistem ayarlarında ilgili sayfayı açar ('firewall' | 'localNetwork').
#[tauri::command]
fn open_system_settings(target: String) -> Result<(), String> {
    platform::netinfo::open_system_settings(&target)
}

/// macOS Yerel Ağ iznini tetikler: yerel ağa bir yayın gönderir ve kimliği yeniden duyurur
/// (karar verilmemişse sistem izin penceresini gösterir). Windows'ta izin gerekmez.
#[tauri::command]
async fn request_local_network(mgr: State<'_, ManagerHandle>) -> Result<bool, String> {
    let handle = mgr.inner().clone();
    handle.reannounce();
    if cfg!(target_os = "macos") {
        let _ = tauri::async_runtime::spawn_blocking(move || handle.probe_local_network()).await;
    }
    Ok(true)
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
    let _guard = popover::DialogGuard::new(&app);
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
    let _guard = popover::DialogGuard::new(&app);
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
    // GÜVENLİK: çalıştırılabilir/otomatik eylemli türler, klasörler ve sembolik bağlar
    // açılmaz; yalnızca Finder/Explorer'da gösterilir.
    if transfer::safety::should_reveal_instead(&path) {
        return tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string());
    }
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}

/// Uygulama katmanı: paylaşım bildirimleri (yerel bildirim + pano).
struct AppNotifier {
    app: AppHandle,
    settings: Arc<SettingsStore>,
    clipboard: Arc<plugins::clipboard::ClipboardPlugin>,
}

impl AppNotifier {
    fn show(&self, title: String, body: String) {
        platform::notify(&self.app, &title, &body);
    }

    fn set_clipboard(&self, text: &str) {
        // Pano eklentisi üzerinden yazılır: geçmişe girer ve telefona geri yankılanmaz.
        self.clipboard.apply_shared_text(text.to_string());
    }
}

impl ShareNotifier for AppNotifier {
    fn notify(&self, n: ShareNotice) {
        let lang = i18n::resolve_language(&self.settings.get().language);
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
                // Pano her durumda güncellenir; bildirim "alınan öğeler" anahtarına bağlıdır.
                if self.settings.get().notifications.file_received {
                    let (t, b) = i18n_share::text_received(lang, &device);
                    self.show(t, b);
                }
            }
            ShareNotice::Url { device, url } => {
                // Bağlantı kendiliğinden açılmaz; panoya konur ve bildirilir.
                self.set_clipboard(&url);
                if self.settings.get().notifications.file_received {
                    let (t, b) = i18n_share::url_received(lang, &device, &url);
                    self.show(t, b);
                }
            }
        }
    }
}

/// Protokol yöneticisini Tauri'nin tokio çalışma zamanında başlatır.
fn start_protocol(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let data_dir_for_share = data_dir.clone();
    let keys = Arc::new(KeyringStore::new(KEYCHAIN_SERVICE, KEYCHAIN_USER));
    let mut registry = plugins::Registry::new();
    let shared = registry.shared();
    let notifier: plugins::notifier::SharedNotifier = Arc::new(TauriNotifier(app.clone()));
    registry.register(Arc::new(plugins::battery::BatteryPlugin::new(
        shared.clone(),
        notifier.clone(),
    )));
    registry.register(Arc::new(plugins::ping::PingPlugin::new(
        shared.clone(),
        notifier.clone(),
    )));
    registry.register(Arc::new(plugins::findmyphone::FindMyPhonePlugin));
    let runcommand = Arc::new(plugins::runcommand::RunCommandPlugin::new(
        shared.clone(),
        notifier.clone(),
        Arc::new(plugins::runcommand::ShellRunner),
        {
            let app = app.clone();
            Arc::new(move || show_main(&app))
        },
    ));
    registry.register(runcommand.clone());
    app.manage(runcommand);
    registry.register(Arc::new(plugins::telephony::TelephonyPlugin::new(
        shared,
        notifier,
        platform::audio::SystemAudio,
    )));
    let clipboard = plugins::clipboard::ClipboardPlugin::new(Arc::new(TauriSink(app.clone())));
    registry.register(Arc::new(plugins::clipboard::ClipboardPluginHandle(
        clipboard.clone(),
    )));
    let share = ShareService::new();
    registry.register(Arc::new(SharePlugin(share.clone())));
    let cfg = ManagerConfig::production(data_dir, keys, registry);
    let sink = Arc::new(TauriSink(app.clone()));
    let handle = match tauri::async_runtime::block_on(manager::start(cfg, sink.clone())) {
        Ok(h) => h,
        Err(e) => {
            // Anahtar zinciri okunamadıysa kimliği yeniden üretip eşleşmeleri silmek yerine
            // kullanıcıyı uyar ve çık; bir sonraki açılışta izin verilince her şey yerinde kalır.
            let lang = i18n::resolve_language("system");
            let body = if e.starts_with("anahtar zincirine") {
                "startFailedKeychain"
            } else {
                "startFailedBody"
            };
            app.dialog()
                .message(i18n::notify_text(lang, body, &[]))
                .title(i18n::notify_text(lang, "startFailedTitle", &[]))
                .kind(tauri_plugin_dialog::MessageDialogKind::Error)
                .blocking_show();
            return Err(e.into());
        }
    };
    clipboard.start(handle.settings_store());
    shortcuts::register_all(app, &handle.get_settings().shortcuts);

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
    // Önceki çalışmadan kalan yarım indirmeleri (1 saatten eski, yalnızca kendi adımız) temizle.
    {
        let s = handle.get_settings();
        let dir = if s.download_dir.trim().is_empty() {
            default_dir.clone()
        } else {
            std::path::PathBuf::from(s.download_dir.trim())
        };
        std::thread::spawn(move || {
            let n =
                transfer::safety::cleanup_stale_parts(&dir, std::time::Duration::from_secs(3600));
            if n > 0 {
                log::info!("{n} eski yarım indirme silindi");
            }
        });
    }
    share.attach(ShareEnv {
        local: handle.local_identity(),
        settings: handle.settings_store(),
        sink,
        notifier: Arc::new(AppNotifier {
            app: app.clone(),
            settings: handle.settings_store(),
            clipboard: clipboard.clone(),
        }),
        default_download_dir: default_dir,
        data_dir: data_dir_for_share,
        payload_ports: DEFAULT_PAYLOAD_PORTS,
    });
    app.manage(share);
    app.manage(clipboard);
    app.manage(handle);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diag::init();
    let app = tauri::Builder::default()
        // İkinci başlatma yeni süreç açmaz; var olan ana pencereyi öne getirir. İlk kayıtlı eklenti olmalı.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_ARG]),
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            show_main_window,
            hide_popover,
            resize_popover,
            quit_app,
            reduce_transparency,
            window_material,
            list_devices,
            request_pair,
            accept_pair,
            reject_pair,
            unpair,
            add_device_by_ip,
            ping_device,
            ring_device,
            get_settings,
            set_settings,
            get_identity,
            get_diagnostic_log,
            app_info,
            run_troubleshoot,
            open_system_settings,
            request_local_network,
            plugins::clipboard::get_clipboard_history,
            plugins::clipboard::clear_clipboard_history,
            plugins::clipboard::send_clipboard,
            shortcuts::set_shortcut,
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
            #[cfg(target_os = "macos")]
            platform::notify_macos::request_authorization();
            platform::apply_window_material(app.handle());
            start_protocol(app.handle())?;
            let settings = app.state::<ManagerHandle>().get_settings();
            #[cfg(windows)]
            popover::set_tray_menu(app.handle(), &settings.language)?;
            // Açılışta ayar → sistem eşitlemesi (Dock, tepsi, açılışta başlat).
            apply_settings_effects(app.handle(), None, &settings);
            // Oturum açılışında başlatılan uygulama pencereyi göstermeden menü çubuğunda açılır.
            if settings.onboarded
                && std::env::args().any(|a| a == AUTOSTART_ARG)
                && let Some(w) = app.get_webview_window(MAIN_LABEL)
            {
                let _ = w.hide();
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_plan_keeps_an_entry_point() {
        let mut s = Settings {
            show_in_dock: false,
            show_in_menu_bar: false,
            ..Default::default()
        };
        // Dock da yok, menü çubuğu da kapalı: yine de tepsi görünür kalmalı.
        assert_eq!(
            ui_plan(&s, true),
            UiPlan {
                dock: false,
                tray: true
            }
        );
        s.show_in_dock = true;
        assert_eq!(
            ui_plan(&s, true),
            UiPlan {
                dock: true,
                tray: false
            }
        );
        // Windows'ta tepsi hep görünür.
        assert!(ui_plan(&s, false).tray);
    }
}
