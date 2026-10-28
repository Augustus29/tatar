//! `kdeconnect.mpris` + `kdeconnect.mpris.request` (iki yön; ayrıntılar: docs/PROTOCOL_NOTES.md §6.8).
//!
//! * A yönü: Tatar telefondaki oynatıcıları (Spotify, YouTube Music...) denetler. Saf protokol;
//!   durum cihaz başına bellekte tutulur ve arayüze `media-changed` olayıyla gider.
//! * B yönü: telefon bu bilgisayardaki oynatıcıları denetler. Oynatıcılar `MediaBackend`
//!   (platform/media.rs) üzerinden okunur; telefondan gelen oynatıcı adı yalnızca kendi
//!   listemizle eşitlik karşılaştırmasıyla eşlenir, `action` sabit bir listeden seçilir.
//!
//! Ağdan gelen her alan güvenilmezdir: oynatıcı sayısı/adı, metinler ve sayılar sınırlanır.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};

use super::{DeviceHandle, Plugin, Shared};
use crate::manager::EventSink;
use crate::platform::media::{MediaAction, MediaBackend, PlayerState};
use crate::protocol::packet::{NetworkPacket, lenient_bool, lenient_i64};
use crate::transfer::names::clean_display_text;

pub const TYPE_MPRIS: &str = "kdeconnect.mpris";
pub const TYPE_MPRIS_REQUEST: &str = "kdeconnect.mpris.request";

/// Bir cihazdan kabul edilen en çok oynatıcı sayısı.
const MAX_PLAYERS: usize = 16;
const MAX_NAME_CHARS: usize = 100;
const MAX_TEXT_CHARS: usize = 300;
/// Süre/konum üst sınırı (ms): yaklaşık 12 yıl; taşma ve saçma değerleri keser.
const MAX_MS: i64 = 400_000_000_000;
/// B yönünde değişiklikleri telefona iletme aralığı (yalnızca telefon bizi sorguladıysa).
const POLL_INTERVAL: Duration = Duration::from_secs(4);

// ---------------------------------------------------------------------------------------------
// A yönü: telefondaki oynatıcılar
// ---------------------------------------------------------------------------------------------

/// Telefondaki bir oynatıcının son bilinen durumu (arayüze gider).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePlayer {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub is_playing: bool,
    /// ms; bilinmiyorsa -1.
    pub length: i64,
    /// 0-100; bilinmiyor / ayarlanamıyorsa -1.
    pub volume: i32,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
}

impl Default for RemotePlayer {
    fn default() -> Self {
        // Android da bilinmeyen yetenekleri "izinli" varsayar.
        RemotePlayer {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            is_playing: false,
            length: -1,
            volume: -1,
            can_play: true,
            can_pause: true,
            can_next: true,
            can_previous: true,
            can_seek: true,
        }
    }
}

impl RemotePlayer {
    /// Paketteki mevcut alanları uygular (eksik alan eski değeri korur). Değiştiyse `true`.
    fn update(&mut self, body: &Value) -> bool {
        let before = self.clone();
        let text = |k: &str| {
            body.get(k)
                .and_then(Value::as_str)
                .map(|s| clean_display_text(s, MAX_TEXT_CHARS))
        };
        if let Some(v) = text("title") {
            self.title = v;
        }
        if let Some(v) = text("artist") {
            self.artist = v;
        }
        if let Some(v) = text("album") {
            self.album = v;
        }
        let flag = |k: &str| body.get(k).and_then(lenient_bool);
        if let Some(v) = flag("isPlaying") {
            self.is_playing = v;
        }
        if let Some(v) = flag("canPlay") {
            self.can_play = v;
        }
        if let Some(v) = flag("canPause") {
            self.can_pause = v;
        }
        if let Some(v) = flag("canGoNext") {
            self.can_next = v;
        }
        if let Some(v) = flag("canGoPrevious") {
            self.can_previous = v;
        }
        if let Some(v) = flag("canSeek") {
            self.can_seek = v;
        }
        if let Some(v) = body.get("length").and_then(lenient_i64) {
            self.length = if v < 0 { -1 } else { v.min(MAX_MS) };
        }
        if let Some(v) = body.get("volume").and_then(lenient_i64) {
            self.volume = if (0..=100).contains(&v) { v as i32 } else { -1 };
        }
        *self != before
    }
}

/// Oynatıcı adı geçerli mi? Geri yansıtılacağı için temizlenince değişmemeli (aynen geri gider).
fn valid_player_name(s: &str) -> bool {
    !s.is_empty() && clean_display_text(s, MAX_NAME_CHARS) == s
}

/// Bir cihazın bildirdiği oynatıcılar.
#[derive(Debug, Default)]
pub struct RemoteState {
    players: BTreeMap<String, RemotePlayer>,
    /// Arayüzde son kullanılan oynatıcı.
    selected: Option<String>,
}

impl RemoteState {
    /// `kdeconnect.mpris` gövdesini işler. `(değişti mi, yeni eklenen oynatıcılar)` döner;
    /// yeni oynatıcılar için çağıran durum isteği göndermelidir.
    pub fn apply(&mut self, body: &Value) -> (bool, Vec<String>) {
        // Albüm kapağı yükü: v1'de istemiyoruz.
        if body
            .get("transferringAlbumArt")
            .and_then(lenient_bool)
            .unwrap_or(false)
        {
            return (false, Vec::new());
        }
        let mut changed = false;
        let mut added = Vec::new();
        // Bilinmeyen oynatıcıdan gelen durum yok sayılır (sınırsız büyümeyi önler).
        if let Some(p) = body
            .get("player")
            .and_then(Value::as_str)
            .and_then(|n| self.players.get_mut(n))
        {
            changed |= p.update(body);
        }
        if let Some(list) = body.get("playerList").and_then(Value::as_array) {
            let mut names: Vec<String> = Vec::new();
            for n in list.iter().filter_map(Value::as_str) {
                if names.len() >= MAX_PLAYERS {
                    break;
                }
                if valid_player_name(n) && !names.iter().any(|x| x == n) {
                    names.push(n.to_string());
                }
            }
            let before = self.players.len();
            self.players.retain(|k, _| names.contains(k));
            changed |= self.players.len() != before;
            for n in names {
                if !self.players.contains_key(&n) {
                    self.players.insert(n.clone(), RemotePlayer::default());
                    added.push(n);
                    changed = true;
                }
            }
            if self
                .selected
                .as_ref()
                .is_some_and(|s| !self.players.contains_key(s))
            {
                self.selected = None;
            }
        }
        (changed, added)
    }

    /// Arayüzde gösterilecek oynatıcı: seçilen, yoksa çalan, yoksa ilk.
    pub fn current(&self) -> Option<(&String, &RemotePlayer)> {
        self.selected
            .as_ref()
            .and_then(|s| self.players.get_key_value(s))
            .or_else(|| self.players.iter().find(|(_, p)| p.is_playing))
            .or_else(|| self.players.iter().next())
    }
}

/// Arayüze giden görünüm: bir cihazın gösterilecek oynatıcısı.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub device_id: String,
    pub player: String,
    /// Cihazdaki bütün oynatıcı adları.
    pub players: Vec<String>,
    #[serde(flatten)]
    pub state: RemotePlayer,
}

// ---------------------------------------------------------------------------------------------
// B yönü: bu bilgisayardaki oynatıcılar
// ---------------------------------------------------------------------------------------------

/// Son gönderilen durum (değişiklik farkı için).
#[derive(Default)]
pub struct LastSent {
    players: Option<Vec<String>>,
    states: HashMap<String, PlayerState>,
}

/// Bu bilgisayarın oynatıcılarını `kdeconnect.mpris` paketlerine çeviren saf mantık.
pub struct LocalMedia {
    backend: Arc<dyn MediaBackend>,
}

fn list_packet(players: &[String]) -> NetworkPacket {
    NetworkPacket::new(
        TYPE_MPRIS,
        json!({"playerList": players, "supportAlbumArtPayload": false}),
    )
}

/// Durum alanları (KDE'nin masaüstü gönderimiyle aynı adlar; süre/konum ms).
fn state_fields(player: &str, s: &PlayerState) -> serde_json::Map<String, Value> {
    let Value::Object(m) = json!({
        "player": player,
        "title": s.title,
        "artist": s.artist,
        "album": s.album,
        // Eski GSConnect sürümleri bunu okur.
        "nowPlaying": s.title,
        "isPlaying": s.is_playing,
        "pos": s.pos_ms,
        "length": if s.length_ms < 0 { -1 } else { s.length_ms },
        "canPlay": s.can_play,
        "canPause": s.can_pause,
        "canGoNext": s.can_next,
        "canGoPrevious": s.can_previous,
        "canSeek": s.can_seek,
        "volume": s.volume,
        "albumArtUrl": "",
    }) else {
        unreachable!()
    };
    m
}

/// Konum dışındaki alanlar aynı mı? (Konum zaten telefonda ilerletilir.)
fn same_ignoring_pos(a: &PlayerState, b: &PlayerState) -> bool {
    let mut a = a.clone();
    a.pos_ms = b.pos_ms;
    a == *b
}

impl LocalMedia {
    pub fn new(backend: Arc<dyn MediaBackend>) -> Arc<LocalMedia> {
        Arc::new(LocalMedia { backend })
    }

    /// Telefonun `kdeconnect.mpris.request` paketine verilecek yanıtlar. Engelleyebilir.
    /// `enabled` = false: yalnızca boş oynatıcı listesi bildirilir, hiçbir şey yapılmaz.
    pub fn handle_request(&self, body: &Value, enabled: bool) -> Vec<NetworkPacket> {
        if !enabled {
            return vec![list_packet(&[])];
        }
        // Albüm kapağı: desteklemiyoruz (`supportAlbumArtPayload: false`).
        if body.get("albumArtUrl").is_some() {
            return Vec::new();
        }
        let players = self.backend.players();
        let known = body
            .get("player")
            .and_then(Value::as_str)
            .filter(|p| players.iter().any(|x| x == p));
        let mut out = Vec::new();
        let want_list = body
            .get("requestPlayerList")
            .and_then(lenient_bool)
            .unwrap_or(false);
        if known.is_none() || want_list {
            out.push(list_packet(&players));
        }
        let Some(player) = known else {
            return out;
        };

        // Sıra KDE ile aynı: eylem, ses, sarma / konum, sonra okumalar.
        if let Some(a) = body.get("action").and_then(Value::as_str) {
            match MediaAction::parse(a) {
                Some(act) => {
                    self.backend.action(player, act);
                }
                None => log::debug!("bilinmeyen medya eylemi yok sayıldı"),
            }
        }
        if let Some(v) = body.get("setVolume").and_then(lenient_i64) {
            self.backend.set_volume(player, v.clamp(0, 100) as i32);
        }
        if let Some(off_us) = body.get("Seek").and_then(lenient_i64) {
            // MPRIS `Seek` mikrosaniye ve göreceli.
            if let Some(s) = self.backend.state(player) {
                let target = s.pos_ms.saturating_add(off_us / 1000).clamp(0, MAX_MS);
                self.backend.set_position(player, target);
            }
        }
        if let Some(ms) = body.get("SetPosition").and_then(lenient_i64) {
            self.backend.set_position(player, ms.clamp(0, MAX_MS));
        }

        let now = body
            .get("requestNowPlaying")
            .and_then(lenient_bool)
            .unwrap_or(false);
        let vol = body
            .get("requestVolume")
            .and_then(lenient_bool)
            .unwrap_or(false);
        if now || vol {
            let st = self.backend.state(player);
            let mut m = match (&st, now) {
                (Some(s), true) => state_fields(player, s),
                _ => serde_json::Map::from_iter([("player".to_string(), json!(player))]),
            };
            if vol {
                m.insert(
                    "volume".into(),
                    json!(st.as_ref().map(|s| s.volume).unwrap_or(-1)),
                );
            }
            out.push(NetworkPacket::new(TYPE_MPRIS, Value::Object(m)));
        }
        out
    }

    /// Önceki gönderimden beri değişenleri paketler. Engelleyebilir.
    pub fn diff(&self, last: &mut LastSent, enabled: bool) -> Vec<NetworkPacket> {
        let players = if enabled {
            self.backend.players()
        } else {
            Vec::new()
        };
        let mut out = Vec::new();
        if last.players.as_ref() != Some(&players) {
            out.push(list_packet(&players));
            last.players = Some(players.clone());
        }
        last.states.retain(|k, _| players.contains(k));
        for p in &players {
            let Some(s) = self.backend.state(p) else {
                last.states.remove(p);
                continue;
            };
            let same = last.states.get(p).is_some_and(|o| same_ignoring_pos(o, &s));
            if !same {
                out.push(NetworkPacket::new(
                    TYPE_MPRIS,
                    Value::Object(state_fields(p, &s)),
                ));
                last.states.insert(p.clone(), s);
            }
        }
        out
    }

    pub fn backend(&self) -> &Arc<dyn MediaBackend> {
        &self.backend
    }
}

// ---------------------------------------------------------------------------------------------
// Eklenti
// ---------------------------------------------------------------------------------------------

enum Job {
    Request(DeviceHandle, Value),
    Tick,
}

pub struct MprisPlugin {
    shared: Arc<Shared>,
    sink: Arc<dyn EventSink>,
    local: Arc<LocalMedia>,
    devices: Mutex<BTreeMap<String, DeviceHandle>>,
    remote: Mutex<BTreeMap<String, RemoteState>>,
    /// Bizi en az bir kez sorgulamış (medya denetimi yükleyen) cihazlar.
    subscribed: Mutex<HashSet<String>>,
    last: Mutex<LastSent>,
    jobs: SyncSender<Job>,
}

impl MprisPlugin {
    pub fn new(
        shared: Arc<Shared>,
        sink: Arc<dyn EventSink>,
        local: Arc<LocalMedia>,
    ) -> Arc<MprisPlugin> {
        let (tx, rx) = sync_channel::<Job>(64);
        let p = Arc::new(MprisPlugin {
            shared,
            sink,
            local,
            devices: Mutex::new(BTreeMap::new()),
            remote: Mutex::new(BTreeMap::new()),
            subscribed: Mutex::new(HashSet::new()),
            last: Mutex::new(LastSent::default()),
            jobs: tx,
        });
        // Engelleyen arka uç çağrıları (alt süreç, WinRT) tek bir işçi iş parçacığında sıralanır.
        let weak: Weak<MprisPlugin> = Arc::downgrade(&p);
        std::thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                let Some(p) = weak.upgrade() else { break };
                p.run_job(job);
            }
        });
        let weak = Arc::downgrade(&p);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(POLL_INTERVAL);
                let Some(p) = weak.upgrade() else { break };
                if !p.subscribed.lock().unwrap().is_empty() {
                    let _ = p.jobs.try_send(Job::Tick);
                }
            }
        });
        p
    }

    fn run_job(&self, job: Job) {
        let enabled = self.shared.settings().media_control;
        match job {
            Job::Request(dev, body) => {
                for pkt in self.local.handle_request(&body, enabled) {
                    dev.send(pkt);
                }
            }
            Job::Tick => {
                let packets = self.local.diff(&mut self.last.lock().unwrap(), enabled);
                if packets.is_empty() {
                    return;
                }
                let subs = self.subscribed.lock().unwrap().clone();
                let devs: Vec<DeviceHandle> = self
                    .devices
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(id, _)| subs.contains(*id))
                    .map(|(_, d)| d.clone())
                    .collect();
                for d in devs {
                    for p in &packets {
                        d.send(p.clone());
                    }
                }
            }
        }
    }

    /// Bağlı cihazların gösterilecek oynatıcıları.
    pub fn media_list(&self) -> Vec<MediaInfo> {
        let devs = self.devices.lock().unwrap();
        let remote = self.remote.lock().unwrap();
        remote
            .iter()
            .filter(|(id, _)| devs.contains_key(*id))
            .filter_map(|(id, st)| {
                let (name, p) = st.current()?;
                Some(MediaInfo {
                    device_id: id.clone(),
                    player: name.clone(),
                    players: st.players.keys().cloned().collect(),
                    state: p.clone(),
                })
            })
            .collect()
    }

    fn emit_media(&self) {
        self.sink.emit(
            "media-changed",
            serde_json::to_value(self.media_list()).unwrap_or(Value::Null),
        );
    }

    /// Arayüzden gelen istek: oynatıcı bizim bildirilen listemizde olmalı.
    fn checked_target(&self, device_id: &str, player: &str) -> Result<DeviceHandle, String> {
        let dev = self
            .devices
            .lock()
            .unwrap()
            .get(device_id)
            .cloned()
            .ok_or("cihaz bağlı değil")?;
        let mut remote = self.remote.lock().unwrap();
        let st = remote.get_mut(device_id).ok_or("oynatıcı yok")?;
        if !st.players.contains_key(player) {
            return Err("oynatıcı yok".into());
        }
        st.selected = Some(player.to_string());
        Ok(dev)
    }

    /// `action`: "play" | "pause" | "playPause" | "previous" | "next".
    pub fn remote_action(&self, device_id: &str, player: &str, action: &str) -> Result<(), String> {
        let wire = match action {
            "play" => "Play",
            "pause" => "Pause",
            "playPause" => "PlayPause",
            "previous" => "Previous",
            "next" => "Next",
            _ => return Err("geçersiz eylem".into()),
        };
        let dev = self.checked_target(device_id, player)?;
        let pkt = NetworkPacket::new(
            TYPE_MPRIS_REQUEST,
            json!({"player": player, "action": wire}),
        );
        dev.send(pkt)
            .then_some(())
            .ok_or_else(|| "gönderilemedi".to_string())
    }

    pub fn remote_volume(&self, device_id: &str, player: &str, volume: i32) -> Result<(), String> {
        let dev = self.checked_target(device_id, player)?;
        let pkt = NetworkPacket::new(
            TYPE_MPRIS_REQUEST,
            json!({"player": player, "setVolume": volume.clamp(0, 100)}),
        );
        dev.send(pkt)
            .then_some(())
            .ok_or_else(|| "gönderilemedi".to_string())
    }
}

impl Plugin for MprisPlugin {
    fn name(&self) -> &'static str {
        "mpris"
    }
    fn incoming_types(&self) -> &'static [&'static str] {
        &[TYPE_MPRIS, TYPE_MPRIS_REQUEST]
    }
    fn outgoing_types(&self) -> &'static [&'static str] {
        &[TYPE_MPRIS_REQUEST, TYPE_MPRIS]
    }

    fn handle(&self, device: &DeviceHandle, packet: &NetworkPacket) {
        if packet.packet_type == TYPE_MPRIS_REQUEST {
            self.subscribed.lock().unwrap().insert(device.id.clone());
            if self
                .jobs
                .try_send(Job::Request(device.clone(), packet.body.clone()))
                .is_err()
            {
                log::debug!("medya isteği kuyruğu dolu, istek atlandı");
            }
            return;
        }
        let (changed, added) = self
            .remote
            .lock()
            .unwrap()
            .entry(device.id.clone())
            .or_default()
            .apply(&packet.body);
        for n in added {
            device.send(NetworkPacket::new(
                TYPE_MPRIS_REQUEST,
                json!({"player": n, "requestNowPlaying": true, "requestVolume": true}),
            ));
        }
        if changed {
            self.emit_media();
        }
    }

    fn device_connected(&self, device: &DeviceHandle) {
        self.devices
            .lock()
            .unwrap()
            .insert(device.id.clone(), device.clone());
        // Telefon oynatıcı listesini kendiliğinden de yollar; yine de isteriz.
        device.send(NetworkPacket::new(
            TYPE_MPRIS_REQUEST,
            json!({"requestPlayerList": true}),
        ));
    }

    fn device_disconnected(&self, device_id: &str) {
        self.devices.lock().unwrap().remove(device_id);
        self.subscribed.lock().unwrap().remove(device_id);
        if self.remote.lock().unwrap().remove(device_id).is_some() {
            self.emit_media();
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Tauri komutları
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub fn get_media(m: tauri::State<'_, Arc<MprisPlugin>>) -> Vec<MediaInfo> {
    m.media_list()
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn media_action(
    m: tauri::State<'_, Arc<MprisPlugin>>,
    deviceId: String,
    player: String,
    action: String,
) -> Result<(), String> {
    m.remote_action(&deviceId, &player, &action)
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn media_set_volume(
    m: tauri::State<'_, Arc<MprisPlugin>>,
    deviceId: String,
    player: String,
    volume: i32,
) -> Result<(), String> {
    m.remote_volume(&deviceId, &player, volume)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[derive(Default)]
    struct FakeMedia {
        players: Mutex<Vec<(String, PlayerState)>>,
        calls: Mutex<Vec<String>>,
    }
    impl FakeMedia {
        fn with(names: &[&str]) -> Arc<FakeMedia> {
            let f = FakeMedia::default();
            for n in names {
                f.players.lock().unwrap().push((
                    n.to_string(),
                    PlayerState {
                        is_playing: true,
                        title: "T".into(),
                        artist: "A".into(),
                        length_ms: 1000,
                        pos_ms: 100,
                        volume: 50,
                        can_play: true,
                        can_pause: true,
                        can_next: true,
                        can_previous: true,
                        can_seek: true,
                        ..Default::default()
                    },
                ));
            }
            Arc::new(f)
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }
    impl MediaBackend for Arc<FakeMedia> {
        fn players(&self) -> Vec<String> {
            self.players
                .lock()
                .unwrap()
                .iter()
                .map(|p| p.0.clone())
                .collect()
        }
        fn state(&self, player: &str) -> Option<PlayerState> {
            self.players
                .lock()
                .unwrap()
                .iter()
                .find(|p| p.0 == player)
                .map(|p| p.1.clone())
        }
        fn action(&self, player: &str, a: MediaAction) -> bool {
            self.calls.lock().unwrap().push(format!("{player}:{a:?}"));
            true
        }
        fn set_volume(&self, player: &str, v: i32) -> bool {
            self.calls.lock().unwrap().push(format!("{player}:vol{v}"));
            true
        }
        fn set_position(&self, player: &str, ms: i64) -> bool {
            self.calls.lock().unwrap().push(format!("{player}:pos{ms}"));
            true
        }
    }

    fn local(f: &Arc<FakeMedia>) -> Arc<LocalMedia> {
        LocalMedia::new(Arc::new(f.clone()))
    }

    // ---- A yönü

    #[test]
    fn remote_list_adds_removes_and_asks_for_new() {
        let mut s = RemoteState::default();
        let (c, added) =
            s.apply(&json!({"playerList": ["Spotify", "YT"], "supportAlbumArtPayload": true}));
        assert!(c);
        assert_eq!(added, vec!["Spotify", "YT"]);
        let (c, added) = s.apply(&json!({"playerList": ["Spotify"]}));
        assert!(c && added.is_empty());
        assert_eq!(s.current().unwrap().0, "Spotify");
        // Aynı liste: değişiklik yok.
        assert!(!s.apply(&json!({"playerList": ["Spotify"]})).0);
        let (c, _) = s.apply(&json!({"playerList": []}));
        assert!(c && s.current().is_none());
    }

    #[test]
    fn remote_status_updates_known_player_only() {
        let mut s = RemoteState::default();
        s.apply(&json!({"playerList": ["Spotify"]}));
        let (c, _) = s.apply(&json!({
            "player": "Spotify", "title": "Şarkı", "artist": "X", "isPlaying": "true",
            "length": 200000, "volume": 70, "canGoNext": false, "pos": 5
        }));
        assert!(c);
        let (_, p) = s.current().unwrap();
        assert_eq!(p.title, "Şarkı");
        assert!(p.is_playing && !p.can_next && p.can_previous);
        assert_eq!((p.length, p.volume), (200_000, 70));
        // Eksik alan eskiyi korur.
        s.apply(&json!({"player": "Spotify", "isPlaying": false}));
        assert_eq!(s.current().unwrap().1.title, "Şarkı");
        // Bilinmeyen oynatıcı eklenmez.
        let (c, added) = s.apply(&json!({"player": "Evil", "title": "x"}));
        assert!(!c && added.is_empty());
        assert_eq!(s.players.len(), 1);
    }

    #[test]
    fn remote_validates_untrusted_input() {
        let mut s = RemoteState::default();
        let many: Vec<String> = (0..100).map(|i| format!("p{i}")).collect();
        s.apply(&json!({"playerList": many}));
        assert_eq!(s.players.len(), MAX_PLAYERS);
        let mut s = RemoteState::default();
        s.apply(&json!({"playerList": [
            "", "a\u{0}b", "line\nbreak", "\u{202e}rtl", "x".repeat(500), 5, null, "ok", "ok"
        ]}));
        assert_eq!(s.players.keys().collect::<Vec<_>>(), vec!["ok"]);
        s.apply(&json!({"player": "ok", "volume": 9999, "length": -50, "title": "a\u{202e}b\n".repeat(400)}));
        let p = &s.players["ok"];
        assert_eq!(p.volume, -1);
        assert_eq!(p.length, -1);
        assert!(p.title.chars().count() <= MAX_TEXT_CHARS);
        assert!(!p.title.contains('\u{202e}') && !p.title.contains('\n'));
        s.apply(&json!({"player": "ok", "length": i64::MAX}));
        assert_eq!(s.players["ok"].length, MAX_MS);
        // Tür karışıklığı çökertmez.
        s.apply(&json!({"player": ["x"], "playerList": "no", "title": 5, "isPlaying": {}}));
        // Albüm kapağı yükü yok sayılır.
        assert!(
            !s.apply(&json!({"player": "ok", "transferringAlbumArt": true, "title": "x"}))
                .0
        );
    }

    #[test]
    fn current_prefers_selected_then_playing() {
        let mut s = RemoteState::default();
        s.apply(&json!({"playerList": ["A", "B", "C"]}));
        assert_eq!(s.current().unwrap().0, "A");
        s.apply(&json!({"player": "B", "isPlaying": true}));
        assert_eq!(s.current().unwrap().0, "B");
        s.selected = Some("C".into());
        assert_eq!(s.current().unwrap().0, "C");
        s.apply(&json!({"playerList": ["A", "B"]}));
        assert_eq!(s.current().unwrap().0, "B");
    }

    // ---- B yönü

    #[test]
    fn local_unknown_player_gets_list_only() {
        let f = FakeMedia::with(&["Music"]);
        let out = local(&f).handle_request(&json!({"player": "../etc", "action": "Stop"}), true);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].body["playerList"], json!(["Music"]));
        assert_eq!(out[0].body["supportAlbumArtPayload"], json!(false));
        assert!(f.calls().is_empty());
    }

    #[test]
    fn local_actions_map_to_fixed_set() {
        let f = FakeMedia::with(&["Music"]);
        let l = local(&f);
        for a in ["Play", "Pause", "PlayPause", "Next", "Previous", "Stop"] {
            l.handle_request(&json!({"player": "Music", "action": a}), true);
        }
        assert_eq!(
            f.calls(),
            vec![
                "Music:Play",
                "Music:Pause",
                "Music:PlayPause",
                "Music:Next",
                "Music:Previous",
                "Music:Stop"
            ]
        );
        l.handle_request(&json!({"player": "Music", "action": "OpenUri"}), true);
        l.handle_request(
            &json!({"player": "Music", "action": "do shell script"}),
            true,
        );
        l.handle_request(&json!({"player": "Music", "action": 5}), true);
        assert_eq!(f.calls().len(), 6);
    }

    #[test]
    fn local_volume_position_seek_are_clamped() {
        let f = FakeMedia::with(&["Music"]);
        let l = local(&f);
        l.handle_request(&json!({"player": "Music", "setVolume": 500}), true);
        l.handle_request(&json!({"player": "Music", "setVolume": "-7"}), true);
        l.handle_request(&json!({"player": "Music", "SetPosition": -1}), true);
        l.handle_request(&json!({"player": "Music", "SetPosition": 4000}), true);
        // pos 100 ms + 2 s
        l.handle_request(&json!({"player": "Music", "Seek": 2_000_000}), true);
        l.handle_request(&json!({"player": "Music", "Seek": -9_000_000_000i64}), true);
        assert_eq!(
            f.calls(),
            vec![
                "Music:vol100",
                "Music:vol0",
                "Music:pos0",
                "Music:pos4000",
                "Music:pos2100",
                "Music:pos0"
            ]
        );
    }

    #[test]
    fn local_now_playing_and_volume_answer() {
        let f = FakeMedia::with(&["Music"]);
        let l = local(&f);
        let out = l.handle_request(
            &json!({"player": "Music", "requestNowPlaying": true, "requestVolume": true}),
            true,
        );
        assert_eq!(out.len(), 1);
        let b = &out[0].body;
        assert_eq!(b["player"], "Music");
        assert_eq!(b["title"], "T");
        assert_eq!(b["nowPlaying"], "T");
        assert_eq!(b["pos"], 100);
        assert_eq!(b["length"], 1000);
        assert_eq!(b["volume"], 50);
        assert_eq!(b["canGoNext"], true);
        let out = l.handle_request(&json!({"player": "Music", "requestVolume": true}), true);
        assert_eq!(out[0].body, json!({"player": "Music", "volume": 50}));
        // Liste isteği + durum isteği birlikte.
        let out = l.handle_request(
            &json!({"player": "Music", "requestPlayerList": true, "requestNowPlaying": true}),
            true,
        );
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn local_disabled_and_album_art() {
        let f = FakeMedia::with(&["Music"]);
        let l = local(&f);
        let out = l.handle_request(&json!({"player": "Music", "action": "Play"}), false);
        assert_eq!(out[0].body["playerList"], json!([]));
        assert!(f.calls().is_empty());
        assert!(
            l.handle_request(
                &json!({"player": "Music", "albumArtUrl": "file:///x"}),
                true
            )
            .is_empty()
        );
    }

    #[test]
    fn local_diff_sends_only_changes() {
        let f = FakeMedia::with(&["Music"]);
        let l = local(&f);
        let mut last = LastSent::default();
        let first = l.diff(&mut last, true);
        assert_eq!(first.len(), 2); // liste + durum
        assert!(l.diff(&mut last, true).is_empty());
        // Yalnızca konum değişirse gönderilmez.
        f.players.lock().unwrap()[0].1.pos_ms = 5000;
        assert!(l.diff(&mut last, true).is_empty());
        f.players.lock().unwrap()[0].1.is_playing = false;
        let out = l.diff(&mut last, true);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].body["isPlaying"], false);
        // Yeni oynatıcı: liste + onun durumu.
        f.players
            .lock()
            .unwrap()
            .push(("Spotify".into(), PlayerState::default()));
        assert_eq!(l.diff(&mut last, true).len(), 2);
        // Kapatılınca boş liste bir kez bildirilir.
        let out = l.diff(&mut last, false);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].body["playerList"], json!([]));
        assert!(l.diff(&mut last, false).is_empty());
    }

    // ---- Eklenti

    struct Sink(Mutex<Vec<(String, Value)>>);
    impl EventSink for Sink {
        fn emit(&self, event: &str, payload: Value) {
            self.0.lock().unwrap().push((event.to_string(), payload));
        }
    }

    fn device(id: &str, caps: &[&str]) -> (DeviceHandle, mpsc::Receiver<NetworkPacket>) {
        let (tx, rx) = mpsc::channel(32);
        (
            DeviceHandle {
                id: id.into(),
                name: "Phone".into(),
                peer_incoming: Arc::new(caps.iter().map(|s| s.to_string()).collect()),
                tx,
                peer_ip: None,
                peer_cert: Arc::new(Vec::new()),
            },
            rx,
        )
    }

    #[test]
    fn plugin_flow_a_requests_status_and_validates_ui_commands() {
        let sink = Arc::new(Sink(Mutex::new(vec![])));
        let f = FakeMedia::with(&[]);
        let p = MprisPlugin::new(Arc::new(Shared::default()), sink.clone(), local(&f));
        let (dev, mut rx) = device("ph", &[TYPE_MPRIS_REQUEST]);
        p.device_connected(&dev);
        assert_eq!(
            rx.try_recv().unwrap().body,
            json!({"requestPlayerList": true})
        );

        p.handle(
            &dev,
            &NetworkPacket::new(TYPE_MPRIS, json!({"playerList": ["Spotify"]})),
        );
        let req = rx.try_recv().unwrap();
        assert_eq!(req.packet_type, TYPE_MPRIS_REQUEST);
        assert_eq!(req.body["player"], "Spotify");
        assert_eq!(req.body["requestNowPlaying"], true);

        p.handle(
            &dev,
            &NetworkPacket::new(
                TYPE_MPRIS,
                json!({"player": "Spotify", "title": "S", "isPlaying": true}),
            ),
        );
        let list = p.media_list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title_for_test(), "S");
        assert_eq!(sink.0.lock().unwrap().last().unwrap().0, "media-changed");

        assert!(p.remote_action("ph", "Spotify", "next").is_ok());
        let sent = rx.try_recv().unwrap();
        assert_eq!(sent.body, json!({"player": "Spotify", "action": "Next"}));
        assert!(p.remote_action("ph", "Spotify", "rm -rf").is_err());
        assert!(p.remote_action("ph", "Nope", "next").is_err());
        assert!(p.remote_action("zz", "Spotify", "next").is_err());
        assert!(p.remote_volume("ph", "Spotify", 999).is_ok());
        assert_eq!(rx.try_recv().unwrap().body["setVolume"], 100);

        p.device_disconnected("ph");
        assert!(p.media_list().is_empty());
    }

    impl MediaInfo {
        fn title_for_test(&self) -> &str {
            &self.state.title
        }
    }

    #[test]
    fn plugin_flow_b_answers_via_worker() {
        let sink = Arc::new(Sink(Mutex::new(vec![])));
        let f = FakeMedia::with(&["Music"]);
        let p = MprisPlugin::new(Arc::new(Shared::default()), sink, local(&f));
        let (dev, mut rx) = device("ph", &[TYPE_MPRIS]);
        p.handle(
            &dev,
            &NetworkPacket::new(
                TYPE_MPRIS_REQUEST,
                json!({"player": "Music", "action": "Pause", "requestNowPlaying": true}),
            ),
        );
        let start = std::time::Instant::now();
        let got = loop {
            if let Ok(pkt) = rx.try_recv() {
                break pkt;
            }
            assert!(start.elapsed() < Duration::from_secs(3), "yanıt gelmedi");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(got.body["player"], "Music");
        assert_eq!(f.calls(), vec!["Music:Pause"]);
    }
}
