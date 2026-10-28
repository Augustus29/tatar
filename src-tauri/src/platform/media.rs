//! Bu bilgisayardaki medya oynatıcılarını okuma / yönetme. Küçük ve platformdan bağımsız API;
//! gerçek iş macOS'ta (Müzik ve Spotify için sabit AppleScript) ve Windows'ta
//! (`GlobalSystemMediaTransportControlsSessionManager`) yapılır. Eklenti mantığı bu arayüz
//! üzerinden çalışır; testlerde sahtesi kullanılır.
//!
//! Güvenlik: oynatıcı adları telefondan gelir. Arka uçlar adı YALNIZCA kendi listesindeki adlarla
//! eşitlik karşılaştırmasıyla eşleştirir; hiçbir telefon dizesi komuta ya da betiğe girmez.

/// Bir oynatıcının anlık durumu. Süre ve konum milisaniyedir.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerState {
    pub is_playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Bilinmiyorsa -1.
    pub length_ms: i64,
    pub pos_ms: i64,
    /// 0-100; ayarlanamıyorsa -1.
    pub volume: i32,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
}

/// Telefonun isteyebileceği (sabit) eylemler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaAction {
    Play,
    Pause,
    PlayPause,
    Next,
    Previous,
    Stop,
}

impl MediaAction {
    /// KDE Connect'in `action` dizesi (büyük/küçük harf duyarlı); bilinmeyen = `None`.
    pub fn parse(s: &str) -> Option<MediaAction> {
        Some(match s {
            "Play" => MediaAction::Play,
            "Pause" => MediaAction::Pause,
            "PlayPause" => MediaAction::PlayPause,
            "Next" => MediaAction::Next,
            "Previous" => MediaAction::Previous,
            "Stop" => MediaAction::Stop,
            _ => return None,
        })
    }
}

/// Medya arka ucu. Çağrılar engelleyebilir (alt süreç, WinRT); çağıran ayrı iş parçacığında olmalı.
pub trait MediaBackend: Send + Sync {
    /// Şu an denetleyebildiğimiz oynatıcıların adları (yinelenmeyen, sabit sıra).
    fn players(&self) -> Vec<String>;
    /// Oynatıcının durumu; yoksa / okunamıyorsa `None`.
    fn state(&self, player: &str) -> Option<PlayerState>;
    fn action(&self, player: &str, action: MediaAction) -> bool;
    /// `volume` 0-100.
    fn set_volume(&self, player: &str, volume: i32) -> bool;
    fn set_position(&self, player: &str, pos_ms: i64) -> bool;
}

/// İşletim sisteminin gerçek medya arka ucu.
pub struct SystemMedia;

impl MediaBackend for SystemMedia {
    fn players(&self) -> Vec<String> {
        imp::players()
    }
    fn state(&self, player: &str) -> Option<PlayerState> {
        imp::state(player)
    }
    fn action(&self, player: &str, action: MediaAction) -> bool {
        imp::action(player, action)
    }
    fn set_volume(&self, player: &str, volume: i32) -> bool {
        imp::set_volume(player, volume.clamp(0, 100))
    }
    fn set_position(&self, player: &str, pos_ms: i64) -> bool {
        imp::set_position(player, pos_ms.max(0))
    }
}

#[cfg(target_os = "macos")]
#[path = "media_macos.rs"]
mod imp;

#[cfg(windows)]
#[path = "media_windows.rs"]
mod imp;

#[cfg(not(any(target_os = "macos", windows)))]
mod imp {
    use super::{MediaAction, PlayerState};
    pub fn players() -> Vec<String> {
        Vec::new()
    }
    pub fn state(_player: &str) -> Option<PlayerState> {
        None
    }
    pub fn action(_player: &str, _action: MediaAction) -> bool {
        false
    }
    pub fn set_volume(_player: &str, _volume: i32) -> bool {
        false
    }
    pub fn set_position(_player: &str, _pos_ms: i64) -> bool {
        false
    }
}

/// Windows oturumunun uygulama kimliğinden (AUMID) okunur bir oynatıcı adı türetir:
/// `Spotify.exe` -> `Spotify`, `Microsoft.ZuneMusic_8wekyb3d8bbwe!App` -> `App`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn friendly_player_name(aumid: &str) -> String {
    let id = aumid.rsplit('!').next().unwrap_or(aumid);
    let id = id
        .strip_suffix(".exe")
        .or_else(|| id.strip_suffix(".EXE"))
        .unwrap_or(id);
    let id = match id.rsplit_once('.') {
        Some((_, last)) if !last.is_empty() => last,
        _ => id,
    };
    let name = crate::transfer::names::clean_display_text(id, 64);
    if name.is_empty() {
        "Player".to_string()
    } else {
        name
    }
}

/// AppleScript betiklerinin ürettiği tek satırlık durum çıktısının alan ayırıcısı (ASCII 31).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub const FIELD_SEP: char = '\u{1f}';

/// `durum<US>ad<US>sanatçı<US>albüm<US>süre<US>konum<US>ses` çıktısını ayrıştırır.
/// `duration_is_ms`: Spotify süreyi milisaniye, Müzik saniye verir; konum ve ses her ikisinde
/// saniye ve 0-100'dür. Durum `stopped` ya da çıktı bozuksa `None`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn parse_script_status(out: &str, duration_is_ms: bool) -> Option<PlayerState> {
    let line = out.trim_end_matches(['\n', '\r']);
    let f: Vec<&str> = line.split(FIELD_SEP).collect();
    if f.len() != 7 {
        return None;
    }
    let status = f[0].trim().to_ascii_lowercase();
    if status != "playing" && status != "paused" {
        return None;
    }
    // AppleScript ondalık ayırıcıyı yerel ayara göre yazabilir.
    let num = |s: &str| s.trim().replace(',', ".").parse::<f64>().ok();
    let secs_to_ms = |v: f64| {
        if v.is_finite() {
            (v * 1000.0).round().clamp(0.0, 4.0e11) as i64
        } else {
            -1
        }
    };
    let length_ms = match num(f[4]) {
        Some(d) if d > 0.0 && duration_is_ms => d.round().min(4.0e11) as i64,
        Some(d) if d > 0.0 => secs_to_ms(d),
        _ => -1,
    };
    let pos_ms = num(f[5]).map(secs_to_ms).unwrap_or(0).max(0);
    let volume = num(f[6])
        .filter(|v| v.is_finite())
        .map(|v| v.round().clamp(0.0, 100.0) as i32)
        .unwrap_or(-1);
    let clean = |s: &str| crate::transfer::names::clean_display_text(s, 300);
    Some(PlayerState {
        is_playing: status == "playing",
        title: clean(f[1]),
        artist: clean(f[2]),
        album: clean(f[3]),
        length_ms,
        pos_ms,
        volume,
        can_play: true,
        can_pause: true,
        can_next: true,
        can_previous: true,
        can_seek: length_ms > 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(parts: &[&str]) -> String {
        parts.join(&FIELD_SEP.to_string())
    }

    #[test]
    fn friendly_names() {
        assert_eq!(friendly_player_name("Spotify.exe"), "Spotify");
        assert_eq!(
            friendly_player_name("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"),
            "ZuneMusic"
        );
        assert_eq!(friendly_player_name("chrome"), "chrome");
        assert_eq!(friendly_player_name(""), "Player");
        assert_eq!(friendly_player_name("\u{202e}\n"), "Player");
    }

    #[test]
    fn action_parse_is_strict() {
        assert_eq!(
            MediaAction::parse("PlayPause"),
            Some(MediaAction::PlayPause)
        );
        assert_eq!(MediaAction::parse("Stop"), Some(MediaAction::Stop));
        assert_eq!(MediaAction::parse("playpause"), None);
        assert_eq!(MediaAction::parse("OpenUri"), None);
        assert_eq!(MediaAction::parse(""), None);
    }

    #[test]
    fn parses_music_status() {
        let s = parse_script_status(
            &line(&[
                "playing",
                "Şarkı",
                "Sanatçı",
                "Albüm",
                "245.5",
                "12,25",
                "60",
            ]),
            false,
        )
        .unwrap();
        assert!(s.is_playing);
        assert_eq!(s.title, "Şarkı");
        assert_eq!(s.length_ms, 245_500);
        assert_eq!(s.pos_ms, 12_250);
        assert_eq!(s.volume, 60);
        assert!(s.can_seek);
    }

    #[test]
    fn parses_spotify_ms_duration_and_paused() {
        let s = parse_script_status(
            &line(&["paused", "a", "b", "c", "180000", "3", "100"]),
            true,
        )
        .unwrap();
        assert!(!s.is_playing);
        assert_eq!(s.length_ms, 180_000);
        assert_eq!(s.pos_ms, 3000);
    }

    #[test]
    fn rejects_stopped_and_malformed() {
        assert!(parse_script_status("stopped", false).is_none());
        assert!(
            parse_script_status(&line(&["stopped", "", "", "", "0", "0", "0"]), false).is_none()
        );
        assert!(parse_script_status(&line(&["playing", "a"]), false).is_none());
        assert!(parse_script_status("", false).is_none());
    }

    #[test]
    fn sanitizes_text_and_clamps_numbers() {
        let s = parse_script_status(
            &line(&["playing", "a\u{202e}b\nc", "x", "y", "nan", "-5", "900"]),
            false,
        )
        .unwrap();
        assert_eq!(s.title, "ab c");
        assert_eq!(s.length_ms, -1);
        assert_eq!(s.pos_ms, 0);
        assert_eq!(s.volume, 100);
        assert!(!s.can_seek);
    }
}
