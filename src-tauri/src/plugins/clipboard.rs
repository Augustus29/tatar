//! Pano eklentisi: `kdeconnect.clipboard` ve `kdeconnect.clipboard.connect`.
//!
//! Kipler (`settings.clipboardMode`):
//! - `auto`   : yerel pano izlenir, değişiklikler bağlı cihazlara gider; gelen içerik panoya yazılır;
//!   bağlanınca `clipboard.connect` gönderilir ve karşıdakinden gelen (daha yeni ise) uygulanır.
//! - `manual` : yerel pano yalnızca geçmiş için izlenir, kendiliğinden GİTMEZ. Yalnızca kullanıcı
//!   "Panoyu gönder" ya da kısayol ile gönderir. Karşı tarafın bilerek gönderdiği `clipboard`
//!   paketi uygulanır (Android zaten yalnızca elle gönderir); `clipboard.connect` (sessiz
//!   eşitleme) yok sayılır ve biz de göndermeyiz.
//! - `off`    : hiçbiri; pano izlenmez, gelen paketler yok sayılır.
//!
//! GÜVENLİK: gizli içerik (şifre yöneticileri) asla gönderilmez; geçmişe boş metinli
//! `concealed` öğe olarak yazılır. Pano içeriği asla loglanmaz (yalnızca uzunluk).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};

use super::{DeviceHandle, Plugin};
use crate::manager::EventSink;
use crate::platform::clipboard as native;
use crate::protocol::packet::NetworkPacket;
use crate::store::SettingsStore;

pub const TYPE_CLIPBOARD: &str = "kdeconnect.clipboard";
pub const TYPE_CLIPBOARD_CONNECT: &str = "kdeconnect.clipboard.connect";

/// Geçmişte tutulacak en çok öğe (CLAUDE.md §4.5).
pub const MAX_HISTORY: usize = 20;
/// Bundan büyük metin ne gönderilir ne uygulanır.
const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(500);

// ---------------------------------------------------------------------------------------------
// Saf mantık (test edilebilir)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Manual,
    Off,
}

impl Mode {
    pub fn parse(s: &str) -> Mode {
        match s {
            "manual" => Mode::Manual,
            "off" => Mode::Off,
            _ => Mode::Auto,
        }
    }
    /// Yerel pano izlensin mi?
    pub fn watches(self) -> bool {
        self != Mode::Off
    }
    /// Yerel değişiklik kendiliğinden gönderilsin mi?
    pub fn auto_sends(self) -> bool {
        self == Mode::Auto
    }
    /// Gelen `kdeconnect.clipboard` uygulansın mı?
    pub fn applies_incoming(self) -> bool {
        self != Mode::Off
    }
    /// `clipboard.connect` gönderilsin/uygulansın mı?
    pub fn uses_connect(self) -> bool {
        self == Mode::Auto
    }
}

/// `clipboard.connect` kuralı (Android ve KDE ile aynı): zaman damgası 0 ya da bizimkinden eskiyse yok say.
pub fn connect_should_apply(remote_ts: i64, own_ts: i64) -> bool {
    remote_ts != 0 && remote_ts >= own_ts
}

/// Karşıdan gelen `clipboard.connect` zaman damgasını (ms) şimdi + 60 sn ile sınırlar:
/// geleceğe uzanan bir damga, sonraki meşru güncellemeleri sonsuza dek "eski" gösterirdi.
pub fn clamp_remote_ts(remote_ts: i64, now: i64) -> i64 {
    remote_ts.min(now.saturating_add(60_000))
}

/// Yerel pano değişince ne yapılacağı.
#[derive(Debug, PartialEq, Eq)]
pub enum LocalAction {
    /// Hiçbir şey yapma (boş, çok büyük ya da az önce ağdan aldığımız içerik).
    Ignore,
    /// Gizli içerik: geçmişe boş metinle yaz, GÖNDERME.
    Concealed,
    /// Geçmişe yaz; `send` doğruysa bağlı cihazlara da gönder.
    Record { send: bool },
}

pub fn decide_local(mode: Mode, concealed: bool, text: &str, echo: &mut EchoGuard) -> LocalAction {
    if !mode.watches() {
        return LocalAction::Ignore;
    }
    if concealed {
        return LocalAction::Concealed;
    }
    if text.is_empty() || text.len() > MAX_TEXT_BYTES {
        return LocalAction::Ignore;
    }
    if echo.consume(text) {
        return LocalAction::Ignore;
    }
    LocalAction::Record {
        send: mode.auto_sends(),
    }
}

/// Ağdan alıp panoya yazdığımız metni, izleyici tekrar "yerel değişiklik" sanıp geri göndermesin.
#[derive(Default)]
pub struct EchoGuard {
    pending: Option<String>,
}

impl EchoGuard {
    pub fn expect(&mut self, text: &str) {
        self.pending = Some(text.to_string());
    }
    /// Beklenen metinse bir kez tüketir ve `true` döner; farklıysa bekleneni de siler.
    pub fn consume(&mut self, text: &str) -> bool {
        match self.pending.take() {
            Some(p) => p == text,
            None => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipKind {
    Text,
    Link,
    Concealed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClipItem {
    pub id: String,
    pub kind: ClipKind,
    /// `concealed` için HER ZAMAN boş.
    pub text: String,
    /// epoch ms
    pub time: i64,
}

pub fn classify(text: &str) -> ClipKind {
    let t = text.trim();
    let lower = t.to_ascii_lowercase();
    if (lower.starts_with("http://") || lower.starts_with("https://"))
        && !t.chars().any(char::is_whitespace)
    {
        ClipKind::Link
    } else {
        ClipKind::Text
    }
}

/// Bellekte, en çok 20 öğelik geçmiş; en yeni başta.
#[derive(Default)]
pub struct History {
    items: VecDeque<ClipItem>,
    next_id: u64,
}

impl History {
    fn push_item(&mut self, kind: ClipKind, text: String, now: i64) {
        self.next_id += 1;
        self.items.push_front(ClipItem {
            id: format!("clip-{}", self.next_id),
            kind,
            text,
            time: now,
        });
        self.items.truncate(MAX_HISTORY);
    }

    /// Aynı metin varsa eskisini kaldırıp en başa taşır (geçmişten yeniden kopyalama çift kayıt yapmasın).
    pub fn push_text(&mut self, text: &str, now: i64) {
        self.items
            .retain(|i| i.kind == ClipKind::Concealed || i.text != text);
        self.push_item(classify(text), text.to_string(), now);
    }

    /// Gizli öğe: metin boş.
    pub fn push_concealed(&mut self, now: i64) {
        self.push_item(ClipKind::Concealed, String::new(), now);
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn snapshot(&self) -> Vec<ClipItem> {
        self.items.iter().cloned().collect()
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------
// Eklenti
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct State {
    history: History,
    echo: EchoGuard,
    /// Yerel panonun son değiştiği an (ms); 0 = bilinmiyor.
    update_ts: i64,
}

pub struct ClipboardPlugin {
    state: Mutex<State>,
    /// Bağlı, eşleşmiş cihazlar (kimliğe göre sıralı, "ilk cihaz" belirgin olsun).
    devices: Mutex<std::collections::BTreeMap<String, DeviceHandle>>,
    settings: OnceLock<Arc<SettingsStore>>,
    sink: Arc<dyn EventSink>,
}

impl ClipboardPlugin {
    pub fn new(sink: Arc<dyn EventSink>) -> Arc<ClipboardPlugin> {
        Arc::new(ClipboardPlugin {
            state: Mutex::new(State::default()),
            devices: Mutex::new(Default::default()),
            settings: OnceLock::new(),
            sink,
        })
    }

    /// Yönetici başladıktan sonra ayar deposunu bağlar ve pano izleyicisini başlatır.
    pub fn start(self: &Arc<Self>, settings: Arc<SettingsStore>) {
        let _ = self.settings.set(settings);
        let me = self.clone();
        tauri::async_runtime::spawn(async move { me.watch_loop().await });
    }

    fn mode(&self) -> Mode {
        self.settings
            .get()
            .map(|s| Mode::parse(&s.get().clipboard_mode))
            .unwrap_or(Mode::Off)
    }

    fn emit_history(&self) {
        let snap = self.history();
        self.sink.emit(
            "clipboard-history-changed",
            serde_json::to_value(snap).unwrap_or(Value::Null),
        );
    }

    pub fn history(&self) -> Vec<ClipItem> {
        self.state.lock().unwrap().history.snapshot()
    }

    pub fn clear_history(&self) {
        self.state.lock().unwrap().history.clear();
        self.emit_history();
    }

    /// Eşleşmiş ve bağlı ilk cihazın kimliği (kısayollar için).
    pub fn first_device(&self) -> Option<String> {
        self.devices.lock().unwrap().keys().next().cloned()
    }

    async fn watch_loop(self: Arc<Self>) {
        let mut last = native::change_token();
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            let token = native::change_token();
            if !self.mode().watches() {
                // İzleme kapalıyken biriken değişiklik, açılınca gönderilmesin.
                last = token;
                // Kapalıyken bekleyen yankı kaydı bayatlamasın.
                self.state.lock().unwrap().echo = EchoGuard::default();
                continue;
            }
            if token == last {
                continue;
            }
            last = token;
            // Yarış: gizli denetimi ile okuma arasında pano değişebilir (ör. şifre kopyalanır).
            // Okumadan sonra jeton değiştiyse sonucu at; sonraki tur yeni değişikliği işler.
            let concealed = native::is_concealed();
            let text = if concealed { None } else { read_text() };
            if native::change_token() != token {
                log::debug!("pano okunurken değişti, atlandı");
                continue;
            }
            self.on_local_change(concealed, text);
        }
    }

    fn on_local_change(&self, concealed: bool, text: Option<String>) {
        let mode = self.mode();
        let now = now_ms();
        let action = {
            let mut st = self.state.lock().unwrap();
            let t = text.as_deref().unwrap_or("");
            let action = decide_local(mode, concealed, t, &mut st.echo);
            match &action {
                LocalAction::Concealed => st.history.push_concealed(now),
                LocalAction::Record { .. } => {
                    st.history.push_text(t, now);
                    st.update_ts = now;
                }
                LocalAction::Ignore => {}
            }
            action
        };
        match action {
            LocalAction::Ignore => {}
            LocalAction::Concealed => {
                log::debug!("gizli pano içeriği: gönderilmedi, geçmişe kilitli olarak yazıldı");
                self.emit_history();
            }
            LocalAction::Record { send } => {
                if send && let Some(t) = text {
                    log::debug!("yerel pano değişti ({} bayt), gönderiliyor", t.len());
                    self.broadcast(&t);
                }
                self.emit_history();
            }
        }
    }

    fn broadcast(&self, text: &str) {
        let pkt = NetworkPacket::new(TYPE_CLIPBOARD, json!({ "content": text }));
        for d in self.devices.lock().unwrap().values() {
            d.send(pkt.clone());
        }
    }

    /// Komut/kısayol: metni (yoksa şu anki panoyu) tek bir cihaza gönderir.
    pub fn send_to(&self, device_id: &str, text: Option<String>) -> Result<(), String> {
        if self.mode() == Mode::Off {
            return Err("pano eşitleme kapalı".into());
        }
        let text = match text {
            Some(t) => t,
            None => {
                let token = native::change_token();
                if native::is_concealed() {
                    return Err("pano içeriği gizli işaretli, gönderilmez".into());
                }
                let t = read_text().ok_or("pano boş ya da metin değil")?;
                if native::change_token() != token {
                    return Err("pano okunurken değişti, tekrar deneyin".into());
                }
                t
            }
        };
        if text.is_empty() || text.len() > MAX_TEXT_BYTES {
            return Err("pano içeriği gönderilemez (boş ya da çok büyük)".into());
        }
        let dev = self
            .devices
            .lock()
            .unwrap()
            .get(device_id)
            .cloned()
            .ok_or("cihaz bağlı değil")?;
        if !dev.send(NetworkPacket::new(
            TYPE_CLIPBOARD,
            json!({ "content": text }),
        )) {
            return Err("cihaz panoyu kabul etmiyor ya da kuyruk dolu".into());
        }
        {
            let mut st = self.state.lock().unwrap();
            st.history.push_text(&text, now_ms());
        }
        self.emit_history();
        Ok(())
    }

    /// Kısayol: ilk bağlı cihaza gönder.
    pub fn send_to_first(&self) -> Result<(), String> {
        let id = self.first_device().ok_or("bağlı cihaz yok")?;
        self.send_to(&id, None)
    }

    /// Ağdan gelen metni yerel panoya yazar (ayrı iş parçacığında; pano meşgul olabilir).
    /// Paylaşımla gelen metin/bağlantıyı panoya yazar (yankı korumasıyla).
    pub fn apply_shared_text(self: &Arc<Self>, text: String) {
        self.apply_remote(text, now_ms());
    }

    fn apply_remote(self: &Arc<Self>, text: String, ts: i64) {
        // Kapalıyken pano ve geçmişe dokunulmaz; yankı kaydı da bırakılmaz.
        if self.mode() == Mode::Off {
            return;
        }
        if text.is_empty() || text.len() > MAX_TEXT_BYTES {
            log::debug!("gelen pano içeriği yok sayıldı (boş ya da çok büyük)");
            return;
        }
        {
            let mut st = self.state.lock().unwrap();
            st.echo.expect(&text);
            st.update_ts = ts;
            st.history.push_text(&text, now_ms());
        }
        self.emit_history();
        let me = self.clone();
        std::thread::spawn(move || {
            if write_text(&text).is_err() {
                log::warn!("pano yazılamadı");
                // Yazılamadıysa bekleyen yankı kaydı kalmasın.
                me.state.lock().unwrap().echo = EchoGuard::default();
            }
        });
    }
}

fn read_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

fn write_text(text: &str) -> Result<(), arboard::Error> {
    arboard::Clipboard::new()?.set_text(text.to_string())
}

/// `Plugin` arayüzü `Arc<Self>` vermediği için ince bir sarmalayıcı.
pub struct ClipboardPluginHandle(pub Arc<ClipboardPlugin>);

impl Plugin for ClipboardPluginHandle {
    fn name(&self) -> &'static str {
        "clipboard"
    }
    fn incoming_types(&self) -> &'static [&'static str] {
        &[TYPE_CLIPBOARD, TYPE_CLIPBOARD_CONNECT]
    }
    fn outgoing_types(&self) -> &'static [&'static str] {
        &[TYPE_CLIPBOARD, TYPE_CLIPBOARD_CONNECT]
    }

    fn handle(&self, _device: &DeviceHandle, packet: &NetworkPacket) {
        let p = &self.0;
        let mode = p.mode();
        match packet.packet_type.as_str() {
            TYPE_CLIPBOARD if mode.applies_incoming() => {
                if let Some(c) = packet.body_str("content") {
                    p.apply_remote(c.to_string(), now_ms());
                }
            }
            TYPE_CLIPBOARD_CONNECT if mode.uses_connect() => {
                let ts = clamp_remote_ts(packet.body_i64("timestamp").unwrap_or(0), now_ms());
                let own = p.state.lock().unwrap().update_ts;
                if connect_should_apply(ts, own)
                    && let Some(c) = packet.body_str("content")
                {
                    p.apply_remote(c.to_string(), ts);
                }
            }
            _ => {}
        }
    }

    fn device_connected(&self, device: &DeviceHandle) {
        let p = &self.0;
        p.devices
            .lock()
            .unwrap()
            .insert(device.id.clone(), device.clone());
        if !p.mode().uses_connect() {
            return;
        }
        let ts = p.state.lock().unwrap().update_ts;
        let token = native::change_token();
        if ts == 0 || native::is_concealed() {
            return;
        }
        let text = read_text().filter(|t| !t.is_empty() && t.len() <= MAX_TEXT_BYTES);
        if native::change_token() != token {
            return; // okurken değişti: gizli olabilir, gönderme
        }
        if let Some(text) = text {
            device.send(NetworkPacket::new(
                TYPE_CLIPBOARD_CONNECT,
                json!({ "timestamp": ts, "content": text }),
            ));
        }
    }

    fn device_disconnected(&self, device_id: &str) {
        self.0.devices.lock().unwrap().remove(device_id);
    }
}

// ---------------------------------------------------------------------------------------------
// Tauri komutları
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub fn get_clipboard_history(clip: tauri::State<'_, Arc<ClipboardPlugin>>) -> Vec<ClipItem> {
    clip.history()
}

#[tauri::command]
pub fn clear_clipboard_history(clip: tauri::State<'_, Arc<ClipboardPlugin>>) {
    clip.clear_history();
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn send_clipboard(
    clip: tauri::State<'_, Arc<ClipboardPlugin>>,
    deviceId: String,
    text: Option<String>,
) -> Result<(), String> {
    clip.send_to(&deviceId, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_capped_newest_first() {
        let mut h = History::default();
        for i in 0..30 {
            h.push_text(&format!("m{i}"), i);
        }
        let s = h.snapshot();
        assert_eq!(s.len(), MAX_HISTORY);
        assert_eq!(s[0].text, "m29");
        assert_eq!(s[MAX_HISTORY - 1].text, "m10");
    }

    #[test]
    fn history_clear_wipes_everything() {
        let mut h = History::default();
        h.push_text("a", 1);
        h.push_concealed(2);
        h.clear();
        assert!(h.snapshot().is_empty());
    }

    #[test]
    fn concealed_item_has_empty_text() {
        let mut h = History::default();
        h.push_concealed(5);
        let s = h.snapshot();
        assert_eq!(s[0].kind, ClipKind::Concealed);
        assert!(s[0].text.is_empty());
        let v = serde_json::to_value(&s[0]).unwrap();
        assert_eq!(v["kind"], "concealed");
        assert_eq!(v["text"], "");
    }

    #[test]
    fn duplicate_text_moves_to_top() {
        let mut h = History::default();
        h.push_text("a", 1);
        h.push_text("b", 2);
        h.push_text("a", 3);
        let s = h.snapshot();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].text, "a");
    }

    #[test]
    fn classify_links() {
        assert_eq!(classify("https://example.com/x"), ClipKind::Link);
        assert_eq!(classify("HTTP://a.b"), ClipKind::Link);
        assert_eq!(classify("see https://example.com"), ClipKind::Text);
        assert_eq!(classify("merhaba"), ClipKind::Text);
    }

    #[test]
    fn remote_timestamp_is_clamped() {
        assert_eq!(clamp_remote_ts(1_000, 5_000), 1_000);
        assert_eq!(clamp_remote_ts(i64::MAX, 5_000), 65_000);
        assert_eq!(clamp_remote_ts(0, 5_000), 0);
        assert_eq!(clamp_remote_ts(i64::MAX, i64::MAX), i64::MAX);
    }

    #[test]
    fn echo_is_suppressed_once() {
        let mut e = EchoGuard::default();
        e.expect("gelen");
        assert_eq!(
            decide_local(Mode::Auto, false, "gelen", &mut e),
            LocalAction::Ignore
        );
        // Aynı metin sonradan bilerek kopyalanırsa gönderilir.
        assert_eq!(
            decide_local(Mode::Auto, false, "gelen", &mut e),
            LocalAction::Record { send: true }
        );
    }

    #[test]
    fn stale_echo_is_dropped_on_different_text() {
        let mut e = EchoGuard::default();
        e.expect("x");
        assert_eq!(
            decide_local(Mode::Auto, false, "y", &mut e),
            LocalAction::Record { send: true }
        );
        assert!(!e.consume("x"));
    }

    #[test]
    fn mode_logic() {
        let mut e = EchoGuard::default();
        assert_eq!(
            decide_local(Mode::Auto, false, "a", &mut e),
            LocalAction::Record { send: true }
        );
        assert_eq!(
            decide_local(Mode::Manual, false, "a", &mut e),
            LocalAction::Record { send: false }
        );
        assert_eq!(
            decide_local(Mode::Off, false, "a", &mut e),
            LocalAction::Ignore
        );
        assert_eq!(
            decide_local(Mode::Auto, true, "", &mut e),
            LocalAction::Concealed
        );
        assert_eq!(
            decide_local(Mode::Manual, true, "", &mut e),
            LocalAction::Concealed
        );
        assert_eq!(
            decide_local(Mode::Off, true, "", &mut e),
            LocalAction::Ignore
        );
        assert_eq!(
            decide_local(Mode::Auto, false, "", &mut e),
            LocalAction::Ignore
        );
        assert!(Mode::Manual.applies_incoming() && !Mode::Manual.uses_connect());
        assert!(!Mode::Off.applies_incoming());
        assert!(Mode::Auto.uses_connect());
        assert_eq!(Mode::parse("bogus"), Mode::Auto);
        assert_eq!(Mode::parse("manual"), Mode::Manual);
    }

    #[test]
    fn connect_timestamp_rule() {
        assert!(!connect_should_apply(0, 0));
        assert!(!connect_should_apply(5, 10));
        assert!(connect_should_apply(10, 10));
        assert!(connect_should_apply(11, 0));
    }
}
