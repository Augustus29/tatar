//! Windows medya denetimi: `GlobalSystemMediaTransportControlsSessionManager` (SMTC).
//! Sistemdeki her medya oturumu (Spotify, Medya Oynatıcı, tarayıcı sekmeleri...) bir oynatıcıdır.
//! Ad, oturumun uygulama kimliğinden (AUMID) türetilir; telefondan gelen ad yalnızca bu
//! listedeki adlarla eşitlik karşılaştırmasıyla eşlenir.
//! Uygulama başına ses SMTC'de yoktur: ses her zaman -1 (ayarlanamaz) bildirilir.

use std::collections::HashSet;

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

use super::{MediaAction, PlayerState, friendly_player_name};

/// 100 ns birimini milisaniyeye çevirir (negatifler 0'a).
fn ticks_to_ms(ticks: i64) -> i64 {
    (ticks / 10_000).max(0)
}

/// Şu anki oturumlar ve arayüzde görünen (yinelenmeyen) adları.
fn sessions() -> Vec<(String, Session)> {
    // SAFETY: COM'u bu iş parçacığı için başlatır; zaten başlatılmışsa hata yok sayılır.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let Ok(op) = Manager::RequestAsync() else {
        return Vec::new();
    };
    let Ok(mgr) = op.join() else {
        return Vec::new();
    };
    let Ok(list) = mgr.GetSessions() else {
        return Vec::new();
    };
    let mut used: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for s in list.into_iter() {
        let id = s
            .SourceAppUserModelId()
            .map(|h| h.to_string())
            .unwrap_or_default();
        let base = friendly_player_name(&id);
        let mut name = base.clone();
        let mut n = 2;
        while !used.insert(name.clone()) {
            name = format!("{base} ({n})");
            n += 1;
        }
        out.push((name, s));
    }
    out
}

fn find(player: &str) -> Option<Session> {
    sessions()
        .into_iter()
        .find(|(n, _)| n == player)
        .map(|(_, s)| s)
}

pub fn players() -> Vec<String> {
    sessions().into_iter().map(|(n, _)| n).collect()
}

pub fn state(player: &str) -> Option<PlayerState> {
    let s = find(player)?;
    let info = s.GetPlaybackInfo().ok()?;
    let playing = info.PlaybackStatus().ok()? == Status::Playing;
    let c = info.Controls().ok()?;
    let flag = |r: windows::core::Result<bool>| r.unwrap_or(false);
    let clean = |h: windows::core::Result<windows::core::HSTRING>| {
        crate::transfer::names::clean_display_text(
            &h.map(|h| h.to_string()).unwrap_or_default(),
            300,
        )
    };
    let props = s
        .TryGetMediaPropertiesAsync()
        .ok()
        .and_then(|o| o.join().ok());
    let (title, artist, album) = match &props {
        Some(p) => (clean(p.Title()), clean(p.Artist()), clean(p.AlbumTitle())),
        None => Default::default(),
    };
    let (mut length_ms, mut pos_ms) = (-1, 0);
    if let Ok(tl) = s.GetTimelineProperties() {
        let start = tl.StartTime().map(|t| t.Duration).unwrap_or(0);
        let end = tl.EndTime().map(|t| t.Duration).unwrap_or(0);
        if end > start {
            length_ms = ticks_to_ms(end - start);
        }
        pos_ms = tl
            .Position()
            .map(|t| ticks_to_ms(t.Duration - start))
            .unwrap_or(0);
    }
    Some(PlayerState {
        is_playing: playing,
        title,
        artist,
        album,
        length_ms,
        pos_ms,
        volume: -1,
        can_play: flag(c.IsPlayEnabled()),
        can_pause: flag(c.IsPauseEnabled()),
        can_next: flag(c.IsNextEnabled()),
        can_previous: flag(c.IsPreviousEnabled()),
        can_seek: flag(c.IsPlaybackPositionEnabled()) && length_ms > 0,
    })
}

pub fn action(player: &str, action: MediaAction) -> bool {
    let Some(s) = find(player) else {
        return false;
    };
    let r = match action {
        MediaAction::Play => s.TryPlayAsync().and_then(|o| o.join()),
        MediaAction::Pause => s.TryPauseAsync().and_then(|o| o.join()),
        MediaAction::PlayPause => s.TryTogglePlayPauseAsync().and_then(|o| o.join()),
        MediaAction::Next => s.TrySkipNextAsync().and_then(|o| o.join()),
        MediaAction::Previous => s.TrySkipPreviousAsync().and_then(|o| o.join()),
        MediaAction::Stop => s.TryStopAsync().and_then(|o| o.join()),
    };
    r.unwrap_or(false)
}

pub fn set_volume(_player: &str, _volume: i32) -> bool {
    false
}

pub fn set_position(player: &str, pos_ms: i64) -> bool {
    let Some(s) = find(player) else {
        return false;
    };
    // Oturumun başlangıç anına göre konum (100 ns birimi).
    let start = s
        .GetTimelineProperties()
        .and_then(|t| t.StartTime())
        .map(|t| t.Duration)
        .unwrap_or(0);
    s.TryChangePlaybackPositionAsync(start.saturating_add(pos_ms.saturating_mul(10_000)))
        .and_then(|o| o.join())
        .unwrap_or(false)
}
