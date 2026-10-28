//! Dosya aktarımı güvenliği (CLAUDE.md §4): arayüzden gelen yolların doğrulanması, alınan
//! dosyaların "internetten indirildi" işaretlenmesi, açma yerine gösterme kararı, disk alanı
//! denetimi ve artık `.part` dosyalarının temizliği. Loglara dosya adı/yolu yazılmaz.

use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Kullanıcının ev klasörü (standartlaştırılmış). Bulunamazsa `None`.
pub fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let h = std::env::var_os(var).map(PathBuf::from)?;
    if !h.is_absolute() {
        return None;
    }
    h.canonicalize().ok()
}

/// `rel` içindeki düz bileşenler (metin olarak).
fn normal_parts(rel: &Path) -> Vec<String> {
    rel.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// İndirme klasörü doğrulaması
// ---------------------------------------------------------------------------------------------

/// iCloud Drive: `~/Library` altında izin verilen tek yer.
const ICLOUD_REL: [&str; 3] = ["Library", "Mobile Documents", "com~apple~CloudDocs"];

const UNIX_SYSTEM_DIRS: [&str; 8] = [
    "/System",
    "/Library",
    "/etc",
    "/usr",
    "/bin",
    "/sbin",
    "/private",
    "/Applications",
];

/// Saf mantık: standartlaştırılmış `canon` indirme klasörü olarak yasak mı?
/// `extra` = platforma özel yasak klasörler (Windows'ta APPDATA, Program Files...).
pub fn download_dir_blocked(canon: &Path, home: Option<&Path>, extra: &[PathBuf]) -> bool {
    if UNIX_SYSTEM_DIRS.iter().any(|s| canon.starts_with(s)) {
        return true;
    }
    if extra.iter().any(|e| canon.starts_with(e)) {
        return true;
    }
    if let Some(rel) = home.and_then(|h| canon.strip_prefix(h).ok()) {
        let parts = normal_parts(rel);
        if parts.first().map(String::as_str) == Some("Library") {
            let icloud = parts.len() >= ICLOUD_REL.len()
                && parts.iter().zip(ICLOUD_REL).all(|(a, b)| a == b);
            if !icloud {
                return true;
            }
        }
        // Gizli (nokta ile başlayan) herhangi bir bileşen: .ssh, .config, .Trash ...
        if parts.iter().any(|p| p.starts_with('.')) {
            return true;
        }
    }
    false
}

/// Windows'ta yasak klasörler (ortam değişkenlerinden); diğer sistemlerde boş.
fn platform_blocked_dirs() -> Vec<PathBuf> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for var in [
        "APPDATA",
        "LOCALAPPDATA",
        "ProgramData",
        "SystemRoot",
        "windir",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
    ] {
        if let Some(v) = std::env::var_os(var) {
            let p = PathBuf::from(v);
            if p.is_absolute() {
                out.push(p.canonicalize().unwrap_or(p));
            }
        }
    }
    out
}

/// Ayarlardan/arayüzden gelen indirme klasörü kabul edilebilir mi? Mutlak, var olan bir klasör
/// olmalı (standartlaştırıldıktan sonra) ve hassas konumlarda olmamalı. Kullanıcının geçici
/// klasörü (`temp_dir`) serbesttir.
pub fn download_dir_allowed(raw: &str) -> bool {
    let p = Path::new(raw.trim());
    if raw.trim().is_empty() || !p.is_absolute() {
        return false;
    }
    let Ok(canon) = p.canonicalize() else {
        return false;
    };
    if !canon.is_dir() {
        return false;
    }
    if let Some(tmp) = std::env::temp_dir().canonicalize().ok()
        && canon.starts_with(&tmp)
    {
        return true;
    }
    !download_dir_blocked(&canon, home_dir().as_deref(), &platform_blocked_dirs())
}

// ---------------------------------------------------------------------------------------------
// Gönderilecek dosya doğrulaması
// ---------------------------------------------------------------------------------------------

/// Saf mantık: standartlaştırılmış `canon` dosyası gönderilemez mi? Ev klasörü altında gizli
/// (nokta ile başlayan) klasörler (.ssh, .gnupg, .aws ...), `~/Library/Keychains`, ev klasörünün
/// kökündeki gizli dosyalar ve `extra` (uygulamanın kendi veri klasörü) yasaktır.
pub fn send_path_blocked(canon: &Path, home: Option<&Path>, extra: &[PathBuf]) -> bool {
    if extra.iter().any(|e| canon.starts_with(e)) {
        return true;
    }
    if canon.starts_with("/etc") || canon.starts_with("/private/etc") {
        return true;
    }
    if let Some(rel) = home.and_then(|h| canon.strip_prefix(h).ok()) {
        let parts = normal_parts(rel);
        if parts.first().map(String::as_str) == Some("Library")
            && parts.get(1).map(String::as_str) == Some("Keychains")
        {
            return true;
        }
        match parts.split_last() {
            Some((last, dirs)) => {
                if dirs.iter().any(|p| p.starts_with('.')) {
                    return true;
                }
                if dirs.is_empty() && last.starts_with('.') {
                    return true;
                }
            }
            None => return true, // ev klasörünün kendisi dosya değildir
        }
    }
    false
}

/// `path` gönderilebilir mi? Sembolik bağ olmamalı, standartlaştırılmış yol hassas bir
/// konumda (bkz. `send_path_blocked`) ya da `app_data` içinde olmamalı.
pub fn send_path_allowed(path: &Path, app_data: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(m) if !m.file_type().is_symlink() => {}
        _ => return false,
    }
    let Ok(canon) = path.canonicalize() else {
        return false;
    };
    let data = app_data
        .canonicalize()
        .unwrap_or_else(|_| app_data.to_path_buf());
    !send_path_blocked(&canon, home_dir().as_deref(), &[data])
}

// ---------------------------------------------------------------------------------------------
// Açma yerine gösterme
// ---------------------------------------------------------------------------------------------

/// Çalıştırılabilir ya da otomatik eylem tetikleyebilen türler (küçük harf, noktasız).
const RISKY_EXTS: [&str; 42] = [
    "command",
    "tool",
    "terminal",
    "app",
    "pkg",
    "dmg",
    "jar",
    "sh",
    "bash",
    "zsh",
    "scpt",
    "applescript",
    "workflow",
    "action",
    "prefpane",
    "kext",
    "exe",
    "bat",
    "cmd",
    "com",
    "msi",
    "msc",
    "scr",
    "lnk",
    "pif",
    "ps1",
    "psm1",
    "vbs",
    "vbe",
    "js",
    "jse",
    "wsf",
    "wsh",
    "hta",
    "reg",
    "cpl",
    "url",
    "webloc",
    "inetloc",
    "desktop",
    "scptd",
    "mobileconfig",
];

/// Dosya, varsayılan uygulamayla AÇILMAMALI, yalnızca Finder/Explorer'da gösterilmeli mi?
/// Riskli uzantılar (büyük/küçük harf fark etmez), klasörler, sembolik bağlar ve (unix'te)
/// çalıştırma izni olan dosyalar.
pub fn should_reveal_instead(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(m) => {
            if m.file_type().is_symlink() || m.is_dir() {
                return true;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if m.permissions().mode() & 0o111 != 0 {
                    return true;
                }
            }
        }
        Err(_) => return true,
    }
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| RISKY_EXTS.contains(&e.as_str()))
}

// ---------------------------------------------------------------------------------------------
// "İnternetten indirildi" işareti
// ---------------------------------------------------------------------------------------------

/// Alınan dosyayı işletim sisteminin karantina/bölge işaretiyle damgalar, böylece ilk açılışta
/// uyarı çıkar. macOS: `com.apple.quarantine`; Windows: `Zone.Identifier` (ZoneId=3).
pub fn mark_downloaded(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let value = format!("0081;{secs:x};Tatar;");
        // Argüman vektörü: kabuk yok, yol tek bir argümandır (mutlak, `-` ile başlamaz).
        let st = std::process::Command::new("/usr/bin/xattr")
            .arg("-w")
            .arg("com.apple.quarantine")
            .arg(&value)
            .arg(path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
        if st.success() {
            Ok(())
        } else {
            Err(io::Error::other("xattr başarısız"))
        }
    }
    #[cfg(windows)]
    {
        use std::io::Write;
        let mut ads = path.as_os_str().to_os_string();
        ads.push(":Zone.Identifier");
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(PathBuf::from(ads))?;
        f.write_all(b"[ZoneTransfer]\r\nZoneId=3\r\n")
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = path;
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Disk alanı
// ---------------------------------------------------------------------------------------------

/// Birimde boş (kullanılabilir) bayt. Bilinmiyorsa `None` (denetim atlanır).
pub fn free_space(dir: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        let out = std::process::Command::new("/bin/df")
            .arg("-Pk")
            .arg(dir)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_df_available(&String::from_utf8_lossy(&out.stdout))
    }
    #[cfg(not(unix))]
    {
        // TODO(Windows): GetDiskFreeSpaceExW için `windows` crate'inde Win32_Storage_FileSystem
        // özelliği gerekir (Cargo.toml). Şimdilik denetim atlanır.
        let _ = dir;
        None
    }
}

/// `df -Pk` çıktısından "Available" (KiB) sütununu bayta çevirir. Sütun, `NN%` ("Capacity")
/// belirtecinden hemen öncedir; dosya sistemi/bağ noktası adındaki boşluklar sorun olmaz.
// Yalnızca unix'te `free_space` kullanır; Windows'ta testler için derlenir.
#[cfg_attr(not(unix), allow(dead_code))]
pub fn parse_df_available(out: &str) -> Option<u64> {
    let line = out.lines().rev().find(|l| !l.trim().is_empty())?;
    let toks: Vec<&str> = line.split_whitespace().collect();
    let i = toks.iter().position(|t| {
        t.len() > 1 && t.ends_with('%') && t[..t.len() - 1].bytes().all(|b| b.is_ascii_digit())
    })?;
    let kib: u64 = toks.get(i.checked_sub(1)?)?.parse().ok()?;
    kib.checked_mul(1024)
}

// ---------------------------------------------------------------------------------------------
// Artık `.part` temizliği
// ---------------------------------------------------------------------------------------------

/// Uygulamanın kendi geçici dosya adı mı? Tam desen: `tatar-tr-<hex>-<sayı>.part`.
pub fn is_own_part_name(name: &str) -> bool {
    let Some(mid) = name
        .strip_prefix("tatar-tr-")
        .and_then(|r| r.strip_suffix(".part"))
    else {
        return false;
    };
    let Some((hex, num)) = mid.split_once('-') else {
        return false;
    };
    !hex.is_empty()
        && !num.is_empty()
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
        && num.bytes().all(|b| b.is_ascii_digit())
}

/// `dir` içindeki, `max_age`'den eski, uygulamanın kendi `.part` dosyalarını siler. Başka hiçbir
/// şeye dokunmaz (sembolik bağ ve klasörler atlanır). Silinen sayısını döndürür.
pub fn cleanup_stale_parts(dir: &Path, max_age: Duration) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for e in rd.flatten() {
        let name = e.file_name();
        if !name.to_str().is_some_and(is_own_part_name) {
            continue;
        }
        let Ok(m) = std::fs::symlink_metadata(e.path()) else {
            continue;
        };
        if !m.is_file() {
            continue;
        }
        let old = m
            .modified()
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age >= max_age);
        if old && std::fs::remove_file(e.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn download_dir_rules_unix() {
        let home = p("/Users/ali");
        let h = Some(home.as_path());
        for bad in [
            "/System/x",
            "/Library/Foo",
            "/etc",
            "/usr/local",
            "/bin",
            "/sbin",
            "/private/var/db",
            "/Applications",
            "/Users/ali/Library",
            "/Users/ali/Library/Application Support/x",
            "/Users/ali/.ssh",
            "/Users/ali/.config/x",
            "/Users/ali/Documents/.gizli",
        ] {
            assert!(download_dir_blocked(&p(bad), h, &[]), "{bad}");
        }
        for good in [
            "/Users/ali/Downloads",
            "/Users/ali",
            "/Users/ali/Library/Mobile Documents/com~apple~CloudDocs",
            "/Users/ali/Library/Mobile Documents/com~apple~CloudDocs/Belgeler",
            "/Volumes/Disk/Indirilenler",
        ] {
            assert!(!download_dir_blocked(&p(good), h, &[]), "{good}");
        }
        // Benzer ama farklı ad: "/Users/ali/Library2" yasak değil.
        assert!(!download_dir_blocked(&p("/Users/ali/Library2"), h, &[]));
    }

    #[test]
    fn download_dir_extra_blocked() {
        let extra = [p("/c/Users/ali/AppData/Roaming"), p("/c/Windows")];
        assert!(download_dir_blocked(
            &p("/c/Users/ali/AppData/Roaming/X"),
            None,
            &extra
        ));
        assert!(download_dir_blocked(&p("/c/Windows"), None, &extra));
        assert!(!download_dir_blocked(
            &p("/c/Users/ali/Downloads"),
            None,
            &extra
        ));
    }

    #[test]
    fn download_dir_allowed_checks_fs() {
        let d = std::env::temp_dir().join(format!("tatar-safety-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let file = d.join("f.txt");
        std::fs::write(&file, b"x").unwrap();
        assert!(download_dir_allowed(d.to_str().unwrap()));
        assert!(!download_dir_allowed(file.to_str().unwrap()), "dosya");
        assert!(!download_dir_allowed(d.join("yok").to_str().unwrap()));
        assert!(!download_dir_allowed("goreli/yol"));
        assert!(!download_dir_allowed(""));
        assert!(!download_dir_allowed("/etc"));
        #[cfg(unix)]
        {
            let link = d.join("baglanti");
            std::os::unix::fs::symlink("/etc", &link).unwrap();
            assert!(!download_dir_allowed(link.to_str().unwrap()), "bağ /etc'ye");
        }
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn send_path_rules() {
        let home = p("/Users/ali");
        let h = Some(home.as_path());
        let data = [p(
            "/Users/ali/Library/Application Support/io.github.x.tatar",
        )];
        for bad in [
            "/Users/ali/.ssh/id_ed25519",
            "/Users/ali/.gnupg/pubring.kbx",
            "/Users/ali/.aws/credentials",
            "/Users/ali/.config/gh/hosts.yml",
            "/Users/ali/Documents/.git/config",
            "/Users/ali/.zsh_history",
            "/Users/ali/Library/Keychains/login.keychain-db",
            "/Users/ali/Library/Application Support/io.github.x.tatar/trusted.json",
            "/etc/passwd",
            "/private/etc/hosts",
        ] {
            assert!(send_path_blocked(&p(bad), h, &data), "{bad}");
        }
        for good in [
            "/Users/ali/Desktop/a.pdf",
            "/Users/ali/Documents/rapor.docx",
            "/Users/ali/Downloads/.hidden-ama-dosya.txt",
            "/Users/ali/Library/Mobile Documents/com~apple~CloudDocs/a.txt",
            "/Volumes/USB/foto.jpg",
            "/tmp/x.txt",
        ] {
            assert!(!send_path_blocked(&p(good), h, &data), "{good}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn send_path_rejects_symlinks_and_missing() {
        let d = std::env::temp_dir().join(format!("tatar-safety-send-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let real = d.join("a.txt");
        std::fs::write(&real, b"x").unwrap();
        let link = d.join("b.txt");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let data = d.join("veri");
        assert!(send_path_allowed(&real, &data));
        assert!(!send_path_allowed(&link, &data), "sembolik bağ");
        assert!(!send_path_allowed(&d.join("yok"), &data));
        // Uygulama veri klasörünün içi yasak.
        std::fs::create_dir_all(&data).unwrap();
        let secret = data.join("trusted.json");
        std::fs::write(&secret, b"x").unwrap();
        assert!(!send_path_allowed(&secret, &data));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn reveal_instead_of_open() {
        let d = std::env::temp_dir().join(format!("tatar-safety-open-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        for risky in [
            "a.sh",
            "a.EXE",
            "a.Command",
            "a.pkg",
            "a.webloc",
            "a.ps1",
            "a.url",
        ] {
            let f = d.join(risky);
            std::fs::write(&f, b"x").unwrap();
            assert!(should_reveal_instead(&f), "{risky}");
        }
        let ok = d.join("a.pdf");
        std::fs::write(&ok, b"x").unwrap();
        assert!(!should_reveal_instead(&ok));
        assert!(should_reveal_instead(&d), "klasör");
        assert!(should_reveal_instead(&d.join("yok.pdf")));
        #[cfg(unix)]
        {
            let link = d.join("l.pdf");
            std::os::unix::fs::symlink(&ok, &link).unwrap();
            assert!(should_reveal_instead(&link), "sembolik bağ");
        }
        let _ = std::fs::remove_dir_all(d);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn quarantine_attribute_is_set() {
        let d = std::env::temp_dir().join(format!("tatar-safety-q-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        mark_downloaded(&f).unwrap();
        let out = std::process::Command::new("/usr/bin/xattr")
            .arg("-p")
            .arg("com.apple.quarantine")
            .arg(&f)
            .output()
            .unwrap();
        let v = String::from_utf8_lossy(&out.stdout);
        assert!(v.starts_with("0081;") && v.contains(";Tatar;"), "{v}");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn df_parsing() {
        let mac = "Filesystem   1024-blocks      Used Available Capacity  Mounted on\n/dev/disk3s5   971350180 600000000 300000000    67%    /System/Volumes/Data\n";
        assert_eq!(parse_df_available(mac), Some(300_000_000 * 1024));
        let spaced = "Filesystem 1024-blocks Used Available Capacity Mounted on\nmy fs 100 40 60 40% /mnt/a b\n";
        assert_eq!(parse_df_available(spaced), Some(60 * 1024));
        assert_eq!(parse_df_available(""), None);
        assert_eq!(parse_df_available("garbage line"), None);
    }

    #[test]
    fn own_part_pattern_is_exact() {
        assert!(is_own_part_name("tatar-tr-18f3a9c2b1d-0.part"));
        assert!(is_own_part_name("tatar-tr-abc-12.part"));
        for no in [
            "tatar-tr-.part",
            "tatar-tr-abc.part",
            "tatar-tr-xyz-1.part",
            "tatar-tr-abc-1x.part",
            "tatar-tr-abc-1.part.txt",
            "tatar-abc-1.part",
            "rapor.part",
            "tatar-tr-abc-1.PART",
            "../tatar-tr-abc-1.part",
        ] {
            assert!(!is_own_part_name(no), "{no}");
        }
    }

    #[test]
    fn stale_cleanup_only_touches_old_own_parts() {
        let d = std::env::temp_dir().join(format!("tatar-safety-clean-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let own = d.join("tatar-tr-1a-0.part");
        let other = d.join("baska.part");
        let doc = d.join("belge.txt");
        for f in [&own, &other, &doc] {
            std::fs::write(f, b"x").unwrap();
        }
        // Taze dosya silinmez.
        assert_eq!(cleanup_stale_parts(&d, Duration::from_secs(3600)), 0);
        assert!(own.exists());
        // Yaşı eşik altına inince yalnızca kendi dosyamız gider.
        assert_eq!(cleanup_stale_parts(&d, Duration::ZERO), 1);
        assert!(!own.exists());
        assert!(other.exists() && doc.exists());
        let _ = std::fs::remove_dir_all(d);
    }
}
