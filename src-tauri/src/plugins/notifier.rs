//! Sistem bildirimi gönderme soyutlaması. Üretimde Tauri bildirim eklentisi kullanılır;
//! testlerde sahte bir uygulama.

use std::sync::Arc;

pub trait Notifier: Send + Sync {
    fn notify(&self, title: &str, body: &str);
}

pub type SharedNotifier = Arc<dyn Notifier>;
