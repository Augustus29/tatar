//! Paylaşım bildirimleri: metinler `src/i18n/*.json` içindeki `notify.*` anahtarlarından gelir
//! (`crate::i18n`); burada yalnızca boyut biçimi ve yer tutucu doldurma vardır.

use crate::i18n::notify_text;

/// Arayüzdeki `formatSize` ile aynı biçim: 4,2 MB / 4.2 MB, küçüklerde KB.
pub fn format_size(lang: &str, bytes: u64) -> String {
    if bytes >= 1_048_576 {
        let v = bytes as f64 / 1_048_576.0;
        let s = format!("{v:.1}");
        let s = if lang == "tr" { s.replace('.', ",") } else { s };
        format!("{s} MB")
    } else {
        format!("{} KB", ((bytes as f64 / 1024.0).round() as u64).max(1))
    }
}

pub fn file_received(
    lang: &str,
    device: &str,
    name: &str,
    size: u64,
    folder: &str,
) -> (String, String) {
    let size = format_size(lang, size);
    (
        notify_text(lang, "fileReceivedTitle", &[]),
        notify_text(
            lang,
            "fileReceivedBody",
            &[
                ("device", device),
                ("name", name),
                ("size", &size),
                ("folder", folder),
            ],
        ),
    )
}

pub fn text_received(lang: &str, device: &str) -> (String, String) {
    (
        notify_text(lang, "textReceivedTitle", &[]),
        notify_text(lang, "textReceivedBody", &[("device", device)]),
    )
}

pub fn url_received(lang: &str, device: &str, url: &str) -> (String, String) {
    (
        notify_text(lang, "urlReceivedTitle", &[]),
        notify_text(lang, "urlReceivedBody", &[("device", device), ("url", url)]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_ui_format() {
        assert_eq!(format_size("tr", 4_404_019), "4,2 MB");
        assert_eq!(format_size("en", 4_404_019), "4.2 MB");
        assert_eq!(format_size("en", 10), "1 KB");
        assert_eq!(format_size("en", 2048), "2 KB");
    }

    #[test]
    fn texts_come_from_json_tables() {
        let (t, b) = file_received("tr", "Pixel", "a.pdf", 2048, "/İndirilenler");
        assert_eq!(t, "Dosya alındı");
        assert_eq!(
            b,
            "Pixel cihazından a.pdf (2 KB) /İndirilenler klasörüne kaydedildi."
        );
        let (t, b) = url_received("en", "Pixel", "https://x.y");
        assert_eq!(t, "Link received");
        assert!(b.contains("https://x.y") && b.contains("Pixel"));
        assert_eq!(text_received("en", "P").0, "Text received");
    }
}
