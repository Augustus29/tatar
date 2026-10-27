//! Gelen dosya adı güvenliği (CLAUDE.md §4 kural 3). Ağdan gelen ad güvenilmezdir:
//! yol parçaları, `..`, NUL/denetim karakterleri, Windows ayrılmış adları, gizli dosya ve
//! aşırı uzunluk temizlenir; var olan dosyanın üzerine asla yazılmaz; yalnızca seçili indirme
//! klasörünün içine yazılır.

use std::io;
use std::path::{Component, Path, PathBuf};

/// Dosya adı için bayt sınırı (çoğu dosya sistemi 255 bayt).
pub const MAX_NAME_BYTES: usize = 255;
/// Uzantı olarak sayılacak en uzun kuyruk (bayt, noktayla birlikte).
const MAX_EXT_BYTES: usize = 17;

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn is_reserved(stem: &str) -> bool {
    let s = stem.trim_end_matches([' ', '.']);
    RESERVED.iter().any(|r| r.eq_ignore_ascii_case(s))
}

/// Çift yönlü metin denetim karakterleri (uzantıyı tersine çevirip gizleyen sahtecilik için).
fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}')
}

/// Sondaki boşluk ve noktaları (Windows bunları sessizce atar) ile baştaki boşlukları keser.
fn trim_edges(s: &str) -> &str {
    s.trim_start_matches([' ', '\t'])
        .trim_end_matches([' ', '.'])
}

/// `ad.uzantı` -> (`ad`, `.uzantı`). Uzantı yoksa ya da çok uzunsa tamamı gövdedir.
fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && name.len() - i <= MAX_EXT_BYTES => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// `s`'yi en çok `max` bayta, karakter sınırında keser.
fn truncate_bytes(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Ağdan gelen adı yerel dosya sistemi için güvenli, düz bir ada çevirir. Sonuç hiçbir zaman
/// boş değildir, yol ayırıcı içermez, gizli (`.` ile başlayan) değildir ve en çok 255 bayttır.
pub fn sanitize_filename(raw: &str) -> String {
    // 1) Yol bileşenlerini at: son `/` ya da `\` sonrasını al.
    let last = raw.rsplit(['/', '\\']).next().unwrap_or("");
    // 2) Denetim karakterleri (NUL dahil) ve çift yönlü işaretleri at; Windows'ta geçersiz
    //    karakterleri `_` yap.
    let cleaned: String = last
        .chars()
        .filter(|c| !c.is_control() && !is_bidi_control(*c))
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    // 3) Gizli dosya: baştaki noktaları at. Sondaki nokta/boşlukları kes.
    let mut name = trim_edges(cleaned.trim_start_matches([' ', '\t', '.'])).to_string();
    if name.is_empty() {
        return "file".to_string();
    }
    // 4) Windows ayrılmış adları: gövde ayrılmışsa önüne `_` koy ("CON.txt" -> "_CON.txt").
    let stem_end = name.find('.').unwrap_or(name.len());
    if is_reserved(&name[..stem_end]) {
        name.insert(0, '_');
    }
    // 5) Uzunluk: uzantıyı koruyup gövdeyi kısalt.
    if name.len() > MAX_NAME_BYTES {
        let (stem, ext) = split_ext(&name);
        let keep = MAX_NAME_BYTES.saturating_sub(ext.len());
        let stem = truncate_bytes(stem, keep).trim_end_matches([' ', '.']);
        name = format!("{stem}{ext}");
        if name.len() > MAX_NAME_BYTES {
            name = truncate_bytes(&name, MAX_NAME_BYTES).to_string();
        }
    }
    let name = trim_edges(&name).to_string();
    if name.is_empty() {
        "file".to_string()
    } else {
        name
    }
}

/// `n`. aday ad: 0 -> `ad.ext`, 1 -> `ad (1).ext`... Toplam 255 baytı aşmaz (gövde kısalır).
pub fn candidate_name(name: &str, n: u32) -> String {
    if n == 0 {
        return name.to_string();
    }
    let (stem, ext) = split_ext(name);
    let suffix = format!(" ({n})");
    let keep = MAX_NAME_BYTES.saturating_sub(ext.len() + suffix.len());
    let stem = truncate_bytes(stem, keep);
    format!("{stem}{suffix}{ext}")
}

/// `dir` içinde, `name`'in henüz var olmayan ilk adayını döndürür (yalnızca bilgi amaçlı;
/// gerçek yazma `finalize_no_clobber` ile atomik yapılır).
#[cfg(test)]
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let mut n = 0;
    loop {
        let p = dir.join(candidate_name(name, n));
        if std::fs::symlink_metadata(&p).is_err() {
            return p;
        }
        n += 1;
    }
}

/// `path`'in doğrudan `dir`'in içinde (kardeş değil, alt klasör değil, `..` yok) olduğunu
/// doğrular. `dir` var olmalıdır; `path`'in ana klasörü de. Standartlaştırılmış yolu döndürür.
pub fn ensure_inside(dir: &Path, path: &Path) -> io::Result<PathBuf> {
    let bad = |m: &str| io::Error::new(io::ErrorKind::PermissionDenied, m.to_string());
    let canon_dir = dir.canonicalize()?;
    let file_name = path.file_name().ok_or_else(|| bad("dosya adı yok"))?;
    let mut comps = Path::new(file_name).components();
    if !matches!(comps.next(), Some(Component::Normal(_))) || comps.next().is_some() {
        return Err(bad("dosya adı düz bir ad değil"));
    }
    let parent = path.parent().ok_or_else(|| bad("ana klasör yok"))?;
    let canon_parent = parent.canonicalize()?;
    if canon_parent != canon_dir {
        return Err(bad("yol indirme klasörünün dışında"));
    }
    Ok(canon_dir.join(file_name))
}

/// Tamamlanmış `.part` dosyasını `dir` içinde, var olan hiçbir dosyanın üzerine yazmadan
/// son adına taşır (`ad (1).ext`...). Önce sabit bağ (atomik "yoksa oluştur"), bağ desteklenmiyorsa
/// (ör. FAT) var olmayan ada `rename`. Son yolu döndürür.
pub fn finalize_no_clobber(part: &Path, dir: &Path, name: &str) -> io::Result<PathBuf> {
    for n in 0..10_000u32 {
        let target = ensure_inside(dir, &dir.join(candidate_name(name, n)))?;
        match std::fs::hard_link(part, &target) {
            Ok(()) => {
                let _ = std::fs::remove_file(part);
                return Ok(target);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => {
                // Sabit bağ yok: yine de üzerine yazma.
                if std::fs::symlink_metadata(&target).is_ok() {
                    continue;
                }
                std::fs::rename(part, &target)?;
                return Ok(target);
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "uygun bir dosya adı bulunamadı",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "tatar-names-{tag}-{}",
            crate::protocol::identity::generate_device_id()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn strips_path_components() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("..\\x"), "x");
        assert_eq!(
            sanitize_filename("C:\\Windows\\system32\\evil.exe"),
            "evil.exe"
        );
        assert_eq!(sanitize_filename("/etc/hosts"), "hosts");
        assert_eq!(sanitize_filename("a/b/c.txt"), "c.txt");
    }

    #[test]
    fn rejects_dot_names_and_empty() {
        for s in ["", ".", "..", "...", "/", "\\", "a/", " ", "  . ", "\u{0}"] {
            assert_eq!(sanitize_filename(s), "file", "girdi: {s:?}");
        }
    }

    #[test]
    fn hidden_files_lose_the_dot() {
        assert_eq!(sanitize_filename(".hidden"), "hidden");
        assert_eq!(sanitize_filename("..bashrc"), "bashrc");
        assert_eq!(
            sanitize_filename(".ssh/.authorized_keys"),
            "authorized_keys"
        );
    }

    #[test]
    fn removes_control_and_nul() {
        assert_eq!(sanitize_filename("a\0b.txt"), "ab.txt");
        assert_eq!(sanitize_filename("a\nb\r\tc.txt"), "abc.txt");
        assert_eq!(sanitize_filename("x\u{7f}y"), "xy");
    }

    #[test]
    fn bidi_overrides_removed() {
        assert_eq!(sanitize_filename("fatura\u{202E}fdp.exe"), "faturafdp.exe");
    }

    #[test]
    fn windows_reserved_names() {
        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_filename("con"), "_con");
        assert_eq!(sanitize_filename("NUL"), "_NUL");
        assert_eq!(sanitize_filename("com1.log"), "_com1.log");
        assert_eq!(sanitize_filename("LPT9.tar.gz"), "_LPT9.tar.gz");
        assert_eq!(sanitize_filename("AUX "), "_AUX");
        // Ayrılmış ada benzeyen ama farklı adlar dokunulmaz.
        assert_eq!(sanitize_filename("CONSOLE.txt"), "CONSOLE.txt");
        assert_eq!(sanitize_filename("COM10.txt"), "COM10.txt");
    }

    #[test]
    fn trailing_dots_and_spaces() {
        assert_eq!(sanitize_filename("rapor.txt. . "), "rapor.txt");
        assert_eq!(sanitize_filename("  boşluk.txt"), "boşluk.txt");
    }

    #[test]
    fn invalid_windows_chars_replaced() {
        assert_eq!(
            sanitize_filename("a<b>c:d\"e|f?g*h.txt"),
            "a_b_c_d_e_f_g_h.txt"
        );
    }

    #[test]
    fn unicode_turkish_names_kept() {
        for n in [
            "Şartname_Rev2.docx",
            "çğıöşü İĞÜŞÖÇ.pdf",
            "Toplantı notları (son).txt",
            "日本語.png",
        ] {
            assert_eq!(sanitize_filename(n), n);
        }
    }

    #[test]
    fn long_names_capped_keep_extension() {
        let long = format!("{}.docx", "ş".repeat(300));
        let out = sanitize_filename(&long);
        assert!(out.len() <= MAX_NAME_BYTES, "{}", out.len());
        assert!(out.ends_with(".docx"));
        assert!(out.starts_with('ş'));
        // Uzantısız uzun ad.
        let out = sanitize_filename(&"a".repeat(1000));
        assert_eq!(out.len(), MAX_NAME_BYTES);
        // Çok uzun "uzantı" uzantı sayılmaz ama yine sınırlanır.
        let out = sanitize_filename(&format!("a.{}", "b".repeat(400)));
        assert!(out.len() <= MAX_NAME_BYTES);
    }

    #[test]
    fn sanitized_is_always_plain() {
        let nasty = [
            "../..",
            "..\\..\\..\\Windows\\win.ini",
            "a/../../b",
            "%2e%2e/x",
            ".\u{0}.",
            "COM1",
            "\u{202E}",
        ];
        for n in nasty {
            let s = sanitize_filename(n);
            assert!(!s.is_empty());
            assert!(!s.contains(['/', '\\', '\0']), "{s:?}");
            assert!(!s.starts_with('.'), "{s:?}");
        }
    }

    #[test]
    fn candidate_names() {
        assert_eq!(candidate_name("a.txt", 0), "a.txt");
        assert_eq!(candidate_name("a.txt", 1), "a (1).txt");
        assert_eq!(candidate_name("a.txt", 12), "a (12).txt");
        assert_eq!(candidate_name("arşiv.tar.gz", 2), "arşiv.tar (2).gz");
        assert_eq!(candidate_name("noext", 3), "noext (3)");
        let long = sanitize_filename(&format!("{}.txt", "x".repeat(400)));
        assert!(candidate_name(&long, 123).len() <= MAX_NAME_BYTES);
    }

    #[test]
    fn unique_path_never_overwrites() {
        let d = tmp("uniq");
        assert_eq!(unique_path(&d, "a.txt"), d.join("a.txt"));
        std::fs::write(d.join("a.txt"), b"1").unwrap();
        assert_eq!(unique_path(&d, "a.txt"), d.join("a (1).txt"));
        std::fs::write(d.join("a (1).txt"), b"2").unwrap();
        assert_eq!(unique_path(&d, "a.txt"), d.join("a (2).txt"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn inside_dir_check() {
        let d = tmp("inside");
        let sub = d.join("alt");
        std::fs::create_dir_all(&sub).unwrap();
        assert!(ensure_inside(&d, &d.join("ok.txt")).is_ok());
        assert!(ensure_inside(&d, &d.join("alt").join("x.txt")).is_err());
        assert!(ensure_inside(&d, &d.join("..").join("x.txt")).is_err());
        assert!(ensure_inside(&d, &d.join("a/../../x")).is_err());
        assert!(ensure_inside(&d, Path::new("/etc/passwd")).is_err());
        assert!(ensure_inside(&sub, &d.join("x.txt")).is_err());
        assert!(ensure_inside(&d, &d.join("..")).is_err());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn finalize_does_not_overwrite() {
        let d = tmp("fin");
        std::fs::write(d.join("a.txt"), b"eski").unwrap();
        let part = d.join("x.part");
        std::fs::write(&part, b"yeni").unwrap();
        let out = finalize_no_clobber(&part, &d, "a.txt").unwrap();
        assert_eq!(out, d.canonicalize().unwrap().join("a (1).txt"));
        assert_eq!(std::fs::read(d.join("a.txt")).unwrap(), b"eski");
        assert_eq!(std::fs::read(&out).unwrap(), b"yeni");
        assert!(!part.exists());
        let _ = std::fs::remove_dir_all(d);
    }
}
