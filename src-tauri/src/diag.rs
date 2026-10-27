//! Basit günlükleyici: stderr'e yazar ve son satırları bellekte halka arabelleğinde tutar
//! ("Tanılama günlüğünü kopyala" için). Paket gövdesi, pano, dosya içeriği ya da anahtar
//! günlüğe ASLA yazılmaz; çağıranlar yalnızca tür/kimlik/sayı loglar.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

const CAPACITY: usize = 500;

static BUFFER: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
static LOGGER: Logger = Logger;

struct Logger;

fn buffer() -> &'static Mutex<VecDeque<String>> {
    BUFFER.get_or_init(|| Mutex::new(VecDeque::with_capacity(CAPACITY)))
}

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        // Üçüncü taraf kütüphanelerin ayrıntılı çıktısı elenir.
        m.target().starts_with("tatar") || m.level() <= log::Level::Warn
    }
    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        // Satır başı/sonu enjeksiyonuna karşı tek satıra indir.
        let msg = r.args().to_string().replace(['\n', '\r'], " ");
        let line = format!("{ms} {:<5} {}", r.level(), msg);
        eprintln!("{line}");
        let mut b = buffer().lock().unwrap();
        if b.len() == CAPACITY {
            b.pop_front();
        }
        b.push_back(line);
    }
    fn flush(&self) {}
}

/// Ağdan gelen, saldırgan denetiminde olabilecek kısa bir dizeyi (ör. paket türü) loglamak için
/// güvenli hale getirir: en çok 48 karakter, kontrol karakterleri `?` olur.
pub fn clean(s: &str) -> String {
    s.chars()
        .take(48)
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

pub fn init() {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(if cfg!(debug_assertions) {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        });
    }
}

pub fn recent() -> String {
    buffer()
        .lock()
        .unwrap()
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    #[test]
    fn clean_truncates_and_strips_controls() {
        let long = format!("a\u{7}{}", "b".repeat(200));
        let c = super::clean(&long);
        assert_eq!(c.chars().count(), 48);
        assert!(c.starts_with("a?"));
    }

    #[test]
    fn newline_is_flattened() {
        super::init();
        log::warn!("a\nb");
        assert!(!super::recent().contains("a\nb"));
    }
}
