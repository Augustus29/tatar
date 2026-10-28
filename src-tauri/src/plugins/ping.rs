//! `kdeconnect.ping` (iki yön). Gelen ping bildirim olarak gösterilir.

use std::sync::Arc;

use super::notifier::SharedNotifier;
use super::{DeviceHandle, Plugin, Shared};
use crate::i18n::{notify_text, resolve_language};
use crate::protocol::packet::NetworkPacket;
use crate::transfer::names::clean_display_text;

pub const TYPE_PING: &str = "kdeconnect.ping";

pub struct PingPlugin {
    shared: Arc<Shared>,
    notifier: SharedNotifier,
}

impl PingPlugin {
    pub fn new(shared: Arc<Shared>, notifier: SharedNotifier) -> Self {
        PingPlugin { shared, notifier }
    }
}

/// Gönderilecek ping paketi (gövde boş: telefon "Ping!" gösterir).
pub fn ping_packet() -> NetworkPacket {
    NetworkPacket::new(TYPE_PING, serde_json::json!({}))
}

impl Plugin for PingPlugin {
    fn name(&self) -> &'static str {
        "ping"
    }
    fn incoming_types(&self) -> &'static [&'static str] {
        &[TYPE_PING]
    }
    fn outgoing_types(&self) -> &'static [&'static str] {
        &[TYPE_PING]
    }
    fn handle(&self, device: &DeviceHandle, packet: &NetworkPacket) {
        log::info!("ping alındı, bildirim gösteriliyor");
        let lang = resolve_language(&self.shared.settings().language);
        let title = notify_text(lang, "pingTitle", &[]);
        // Telefon ileti de gönderebilir; yoksa varsayılan metin. İleti uzunluğu sınırlanır.
        // Denetim/biçim/bidi karakterleri atılır (bildirimde sahtecilik yapılamasın).
        let msg = packet
            .body_str("message")
            .map(|m| clean_display_text(m, 200))
            .filter(|m| !m.is_empty());
        let body = match msg {
            Some(m) => m,
            None => notify_text(lang, "pingBody", &[("device", &device.name)]),
        };
        self.notifier
            .notify(&format!("{}: {title}", device.name), &body);
    }
}
