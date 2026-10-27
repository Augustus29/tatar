//! Rust tarafı bildirim metinleri (paylaşım). Arayüzdeki `notify.*` anahtarlarıyla aynı
//! metinler; yalnızca Rust'ın gösterdiği bildirimler için. (Diğer eklentilerin metinleriyle
//! sonradan tek bir yardımcıda birleştirilecek.)

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Tr,
    En,
}

/// Ayardaki dil tercihinden ("system" | "tr" | "en") gerçek dili çıkarır.
pub fn resolve(pref: &str) -> Lang {
    match pref {
        "tr" => Lang::Tr,
        "en" => Lang::En,
        _ => {
            let loc = sys_locale::get_locale().unwrap_or_default().to_lowercase();
            if loc.starts_with("tr") {
                Lang::Tr
            } else {
                Lang::En
            }
        }
    }
}

/// Arayüzdeki `formatSize` ile aynı biçim: 4,2 MB / 4.2 MB, küçüklerde KB.
pub fn format_size(lang: Lang, bytes: u64) -> String {
    if bytes >= 1_048_576 {
        let v = bytes as f64 / 1_048_576.0;
        let s = format!("{v:.1}");
        let s = if lang == Lang::Tr {
            s.replace('.', ",")
        } else {
            s
        };
        format!("{s} MB")
    } else {
        format!("{} KB", ((bytes as f64 / 1024.0).round() as u64).max(1))
    }
}

pub fn file_received(
    lang: Lang,
    device: &str,
    name: &str,
    size: u64,
    folder: &str,
) -> (String, String) {
    let size = format_size(lang, size);
    match lang {
        Lang::Tr => (
            "Dosya alındı".into(),
            format!("{device} cihazından {name} ({size}) {folder} klasörüne kaydedildi."),
        ),
        Lang::En => (
            "File received".into(),
            format!("{name} ({size}) from {device} was saved to {folder}."),
        ),
    }
}

pub fn text_received(lang: Lang, device: &str) -> (String, String) {
    match lang {
        Lang::Tr => (
            "Metin alındı".into(),
            format!("{device} cihazından gelen metin panoya kopyalandı."),
        ),
        Lang::En => (
            "Text received".into(),
            format!("Text from {device} was copied to the clipboard."),
        ),
    }
}

pub fn url_received(lang: Lang, device: &str, url: &str) -> (String, String) {
    match lang {
        Lang::Tr => (
            "Bağlantı alındı".into(),
            format!("{device}: {url} (panoya kopyalandı, açılmadı)"),
        ),
        Lang::En => (
            "Link received".into(),
            format!("{device}: {url} (copied to the clipboard, not opened)"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_ui_format() {
        assert_eq!(format_size(Lang::Tr, 4_404_019), "4,2 MB");
        assert_eq!(format_size(Lang::En, 4_404_019), "4.2 MB");
        assert_eq!(format_size(Lang::En, 10), "1 KB");
        assert_eq!(format_size(Lang::En, 2048), "2 KB");
    }

    #[test]
    fn explicit_language_wins() {
        assert_eq!(resolve("tr"), Lang::Tr);
        assert_eq!(resolve("en"), Lang::En);
    }
}
