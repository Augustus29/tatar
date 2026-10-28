//! macOS medya denetimi: Müzik (Music) ve Spotify için SABİT AppleScript, `osascript` ile.
//!
//! Neden bu yol? macOS 15.4'ten beri özel `MediaRemote` çatısı, Apple'ın verdiği özel yetkisi
//! (entitlement) olmayan uygulamalara "şu an çalan" bilgisini vermiyor; imzasız Tatar için
//! güvenilmez. AppleScript ise belgelenmiş, kararlı bir arayüzdür; yalnızca ilk kullanımda
//! macOS'un "Otomasyon" izin penceresini çıkarır. Çalışmayan uygulamayı başlatmamak için önce
//! `NSRunningApplication` ile çalışıp çalışmadığına bakılır (çalışmıyorsa betik hiç çalıştırılmaz).
//!
//! Sınır: yalnızca Müzik ve Spotify. Tarayıcı, VLC vb. okunamaz.
//! Güvenlik: betikler derleme zamanında sabittir; yalnızca bizim doğruladığımız tamsayılar
//! `argv` ile geçer. Telefondan gelen hiçbir dize betiğe ya da komut satırına girmez.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use objc2_app_kit::NSRunningApplication;
use objc2_foundation::NSString;

use super::{FIELD_SEP, MediaAction, PlayerState, parse_script_status};

const OSASCRIPT: &str = "/usr/bin/osascript";
/// İzin penceresi açıkken asılı kalmasın.
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(6);
const MAX_OUTPUT: u64 = 16 * 1024;

/// Desteklenen oynatıcı: arayüzde görünen ad, paket kimliği, süre birimi ve sabit betikler.
struct App {
    name: &'static str,
    bundle: &'static str,
    /// Spotify süreyi milisaniye, Müzik saniye verir.
    duration_is_ms: bool,
    status: &'static str,
    play: &'static str,
    pause: &'static str,
    playpause: &'static str,
    next: &'static str,
    previous: &'static str,
    stop: &'static str,
    volume: &'static str,
    position: &'static str,
}

const APPS: [App; 2] = [
    App {
        name: "Music",
        bundle: "com.apple.Music",
        duration_is_ms: false,
        status: concat!(
            "tell application \"Music\"\n",
            "if player state is stopped then return \"stopped\"\n",
            "set sep to (ASCII character 31)\n",
            "set stText to (player state as text)\n",
            "set vol to sound volume\n",
            "set posn to player position\n",
            "try\n",
            "set trk to current track\n",
            "return stText & sep & (name of trk) & sep & (artist of trk) & sep & (album of trk) & sep & ((duration of trk) as text) & sep & (posn as text) & sep & (vol as text)\n",
            "on error\n",
            "return stText & sep & \"\" & sep & \"\" & sep & \"\" & sep & \"0\" & sep & (posn as text) & sep & (vol as text)\n",
            "end try\n",
            "end tell"
        ),
        play: "tell application \"Music\" to play",
        pause: "tell application \"Music\" to pause",
        playpause: "tell application \"Music\" to playpause",
        next: "tell application \"Music\" to next track",
        previous: "tell application \"Music\" to previous track",
        stop: "tell application \"Music\" to stop",
        volume: "on run argv\ntell application \"Music\" to set sound volume to (item 1 of argv as integer)\nend run",
        position: "on run argv\ntell application \"Music\" to set player position to ((item 1 of argv as integer) / 1000)\nend run",
    },
    App {
        name: "Spotify",
        bundle: "com.spotify.client",
        duration_is_ms: true,
        status: concat!(
            "tell application \"Spotify\"\n",
            "if player state is stopped then return \"stopped\"\n",
            "set sep to (ASCII character 31)\n",
            "set stText to (player state as text)\n",
            "set vol to sound volume\n",
            "set posn to player position\n",
            "try\n",
            "set trk to current track\n",
            "return stText & sep & (name of trk) & sep & (artist of trk) & sep & (album of trk) & sep & ((duration of trk) as text) & sep & (posn as text) & sep & (vol as text)\n",
            "on error\n",
            "return stText & sep & \"\" & sep & \"\" & sep & \"\" & sep & \"0\" & sep & (posn as text) & sep & (vol as text)\n",
            "end try\n",
            "end tell"
        ),
        play: "tell application \"Spotify\" to play",
        pause: "tell application \"Spotify\" to pause",
        playpause: "tell application \"Spotify\" to playpause",
        next: "tell application \"Spotify\" to next track",
        previous: "tell application \"Spotify\" to previous track",
        // Spotify'ın sözlüğünde `stop` yok.
        stop: "tell application \"Spotify\" to pause",
        volume: "on run argv\ntell application \"Spotify\" to set sound volume to (item 1 of argv as integer)\nend run",
        position: "on run argv\ntell application \"Spotify\" to set player position to ((item 1 of argv as integer) / 1000)\nend run",
    },
];

// Ayırıcı değişirse ayrıştırma sessizce bozulmasın.
const _: () = assert!(FIELD_SEP as u32 == 31);

/// Telefondan gelen adı yalnızca kendi tablomuzla eşitlik karşılaştırmasıyla eşler.
fn app_for(player: &str) -> Option<&'static App> {
    APPS.iter().find(|a| a.name == player)
}

fn is_running(app: &App) -> bool {
    let id = NSString::from_str(app.bundle);
    !NSRunningApplication::runningApplicationsWithBundleIdentifier(&id).is_empty()
}

/// Sabit betiği çalıştırır (isteğe bağlı tamsayı argümanıyla). Başarılıysa çıktıyı döndürür.
fn run_script(script: &str, arg: Option<i64>) -> Option<String> {
    let mut cmd = Command::new(OSASCRIPT);
    cmd.arg("-e").arg(script);
    if let Some(n) = arg {
        cmd.arg(n.to_string());
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    // Çıktıyı ayrı iş parçacığında topla: boru dolup betik asılmasın.
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.take(MAX_OUTPUT).read_to_end(&mut buf);
        buf
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() < SCRIPT_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(25))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let out = reader.join().ok()?;
    status
        .success()
        .then(|| String::from_utf8_lossy(&out).into_owned())
}

pub fn players() -> Vec<String> {
    APPS.iter()
        .filter(|a| is_running(a))
        .map(|a| a.name.to_string())
        .collect()
}

pub fn state(player: &str) -> Option<PlayerState> {
    let app = app_for(player)?;
    if !is_running(app) {
        return None;
    }
    parse_script_status(&run_script(app.status, None)?, app.duration_is_ms)
}

pub fn action(player: &str, action: MediaAction) -> bool {
    let Some(app) = app_for(player) else {
        return false;
    };
    if !is_running(app) {
        return false;
    }
    let script = match action {
        MediaAction::Play => app.play,
        MediaAction::Pause => app.pause,
        MediaAction::PlayPause => app.playpause,
        MediaAction::Next => app.next,
        MediaAction::Previous => app.previous,
        MediaAction::Stop => app.stop,
    };
    run_script(script, None).is_some()
}

pub fn set_volume(player: &str, volume: i32) -> bool {
    match app_for(player) {
        Some(app) if is_running(app) => run_script(app.volume, Some(volume.into())).is_some(),
        _ => false,
    }
}

pub fn set_position(player: &str, pos_ms: i64) -> bool {
    match app_for(player) {
        Some(app) if is_running(app) => run_script(app.position, Some(pos_ms)).is_some(),
        _ => false,
    }
}
