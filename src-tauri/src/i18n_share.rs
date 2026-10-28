//! Paylaşım bildirimleri: metinler `src/i18n/*.json` içindeki `notify.*` anahtarlarından gelir
//! (`crate::i18n`); burada yalnızca boyut biçimi ve yer tutucu doldurma vardır.

use crate::i18n::notify_text;
use crate::transfer::names::clean_display_text;

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

/// Bildirimde gösterilecek en çok bağlantı uzunluğu (karakter).
const MAX_URL_DISPLAY: usize = 200;

pub fn url_received(lang: &str, device: &str, url: &str) -> (String, String) {
    // Yalnızca GÖSTERİM için temizlenir ve kısaltılır; panoya giden bağlantı değişmez.
    let mut shown = clean_display_text(url, MAX_URL_DISPLAY);
    if url.chars().count() > MAX_URL_DISPLAY {
        shown.push('…');
    }
    let url = shown.as_str();
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
        let long = format!("https://x.y/{}", "a".repeat(400));
        let (_, b) = url_received("en", "P", &long);
        assert!(b.contains('…') && !b.contains(&"a".repeat(250)));
    }
}
