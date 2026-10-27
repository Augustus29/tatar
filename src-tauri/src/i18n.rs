//! Rust tarafındaki (sistem bildirimi) metinler. Metinler arayüzle aynı olsun diye
//! `src/i18n/*.json` dosyalarındaki `notify.*` anahtarları derleme sırasında gömülür;
//! kodda ayrı bir metin tablosu tutulmaz.

use std::sync::OnceLock;

use serde_json::Value;

const TR: &str = include_str!("../../src/i18n/tr.json");
const EN: &str = include_str!("../../src/i18n/en.json");

fn parse(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or(Value::Null)
}

fn table(lang: &str) -> &'static Value {
    static TR_V: OnceLock<Value> = OnceLock::new();
    static EN_V: OnceLock<Value> = OnceLock::new();
    if lang == "tr" {
        TR_V.get_or_init(|| parse(TR))
    } else {
        EN_V.get_or_init(|| parse(EN))
    }
}

/// Ayardaki dili "tr" ya da "en"e çevirir. "system" ise işletim sistemi dili.
pub fn resolve_language(setting: &str) -> &'static str {
    match setting {
        "tr" => "tr",
        "en" => "en",
        _ => {
            let loc = sys_locale::get_locale().unwrap_or_default();
            if loc.to_ascii_lowercase().starts_with("tr") {
                "tr"
            } else {
                "en"
            }
        }
    }
}

/// `notify.<key>` metnini verir; `{ad}` yer tutucularını `args` ile doldurur.
/// Anahtar yoksa İngilizceye, o da yoksa anahtarın kendisine düşer.
pub fn notify_text(lang: &str, key: &str, args: &[(&str, &str)]) -> String {
    let find = |l: &str| {
        table(l)
            .get("notify")
            .and_then(|n| n.get(key))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let mut text = find(lang)
        .or_else(|| find("en"))
        .unwrap_or_else(|| key.to_string());
    for (k, v) in args {
        text = text.replace(&format!("{{{k}}}"), v);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_localised_and_interpolated() {
        assert_eq!(
            notify_text(
                "tr",
                "batteryLowBody",
                &[("device", "Pixel"), ("level", "12")]
            ),
            "Pixel pili %12 seviyesinde."
        );
        assert_eq!(
            notify_text("en", "batteryFullBody", &[("device", "Pixel")]),
            "Pixel is fully charged."
        );
        assert_eq!(notify_text("en", "nope", &[]), "nope");
        assert_eq!(resolve_language("tr"), "tr");
    }
}
