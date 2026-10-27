//! `kdeconnect.battery` (telefon -> Tatar). Pil bilgisi arayüze gider; eşik geçişlerinde bildirim çıkar.
//! Biz kendi (masaüstü) pilimizi göndermeyiz. iOS eşleri için bağlanınca
//! `kdeconnect.battery.request` göndeririz (Android bunu ilan etmediği için orada hiç gitmez).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::json;

use super::notifier::SharedNotifier;
use super::{DeviceHandle, Plugin, Shared};
use crate::i18n::{notify_text, resolve_language};
use crate::protocol::packet::{NetworkPacket, lenient_bool, lenient_i64};

pub const TYPE_BATTERY: &str = "kdeconnect.battery";
pub const TYPE_BATTERY_REQUEST: &str = "kdeconnect.battery.request";
/// Bu yüzdenin ALTINDA ve şarj olmuyorsa "pil zayıf".
pub const LOW_THRESHOLD: i64 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryEvent {
    Low,
    Full,
}

/// Bir cihaz için "bildirim zaten verildi" bayrakları: her geçişte yalnızca bir kez bildirilir.
#[derive(Debug, Default, Clone)]
pub struct Tracker {
    low_notified: bool,
    full_notified: bool,
}

impl Tracker {
    /// Yeni okumayı işler; bildirim gerekiyorsa olayı döndürür. Saf mantık (test edilebilir).
    pub fn update(&mut self, charge: i64, charging: bool) -> Option<BatteryEvent> {
        let low = charge < LOW_THRESHOLD && !charging;
        let full = charge >= 100 && charging;
        let mut event = None;
        if low && !self.low_notified {
            event = Some(BatteryEvent::Low);
        }
        if full && !self.full_notified {
            event = Some(BatteryEvent::Full);
        }
        // Koşul ortadan kalkınca bayrak sıfırlanır (bir sonraki geçişte tekrar bildirilir).
        self.low_notified = low;
        self.full_notified = full;
        event
    }
}

pub struct BatteryPlugin {
    shared: Arc<Shared>,
    notifier: SharedNotifier,
    trackers: Mutex<HashMap<String, Tracker>>,
}

impl BatteryPlugin {
    pub fn new(shared: Arc<Shared>, notifier: SharedNotifier) -> Self {
        BatteryPlugin {
            shared,
            notifier,
            trackers: Mutex::new(HashMap::new()),
        }
    }
}

impl Plugin for BatteryPlugin {
    fn name(&self) -> &'static str {
        "battery"
    }
    fn incoming_types(&self) -> &'static [&'static str] {
        &[TYPE_BATTERY]
    }
    fn outgoing_types(&self) -> &'static [&'static str] {
        &[TYPE_BATTERY_REQUEST]
    }

    fn handle(&self, device: &DeviceHandle, packet: &NetworkPacket) {
        // Eksik / geçersiz yüzde = bilinmiyor: yok say.
        let Some(charge) = packet
            .body
            .get("currentCharge")
            .and_then(lenient_i64)
            .filter(|c| (0..=100).contains(c))
        else {
            log::debug!("pil paketinde geçerli currentCharge yok");
            return;
        };
        let charging = packet
            .body
            .get("isCharging")
            .and_then(lenient_bool)
            .unwrap_or(false);
        self.shared.set_battery(&device.id, charge, charging);

        let event = self
            .trackers
            .lock()
            .unwrap()
            .entry(device.id.clone())
            .or_default()
            .update(charge, charging);
        let Some(event) = event else { return };
        let settings = self.shared.settings();
        if !settings.notifications.battery_low {
            return;
        }
        let lang = resolve_language(&settings.language);
        let (title, body) = match event {
            BatteryEvent::Low => (
                notify_text(lang, "batteryLowTitle", &[]),
                notify_text(
                    lang,
                    "batteryLowBody",
                    &[("device", &device.name), ("level", &charge.to_string())],
                ),
            ),
            BatteryEvent::Full => (
                notify_text(lang, "batteryFullTitle", &[]),
                notify_text(lang, "batteryFullBody", &[("device", &device.name)]),
            ),
        };
        self.notifier.notify(&title, &body);
    }

    fn device_connected(&self, device: &DeviceHandle) {
        // Yalnızca iOS eşleri bu türü ilan eder; `send` diğerlerinde sessizce vazgeçer.
        device.send(NetworkPacket::new(
            TYPE_BATTERY_REQUEST,
            json!({"request": true}),
        ));
    }

    fn device_disconnected(&self, device_id: &str) {
        // Erişilemeyen cihazın eski pil değeri gösterilmez. Bildirim bayrakları korunur
        // (yeniden bağlanınca aynı uyarı tekrar tekrar çıkmasın).
        self.shared.clear_battery(device_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_fires_once_per_crossing() {
        let mut t = Tracker::default();
        assert_eq!(t.update(50, false), None);
        assert_eq!(t.update(15, false), None, "15 eşik değil, altı");
        assert_eq!(t.update(14, false), Some(BatteryEvent::Low));
        assert_eq!(t.update(13, false), None);
        assert_eq!(t.update(12, false), None);
        // Şarja takılınca sıfırlanır; çıkarınca tekrar düşükse yeniden bildirilir.
        assert_eq!(t.update(12, true), None);
        assert_eq!(t.update(12, false), Some(BatteryEvent::Low));
        // Eşiğin üstüne çıkıp tekrar inince yine bildirilir.
        assert_eq!(t.update(40, false), None);
        assert_eq!(t.update(10, false), Some(BatteryEvent::Low));
    }

    #[test]
    fn no_low_while_charging() {
        let mut t = Tracker::default();
        assert_eq!(t.update(5, true), None);
        assert_eq!(t.update(9, true), None);
    }

    #[test]
    fn full_fires_once_when_charging_at_100() {
        let mut t = Tracker::default();
        assert_eq!(t.update(99, true), None);
        assert_eq!(t.update(100, true), Some(BatteryEvent::Full));
        assert_eq!(t.update(100, true), None);
        assert_eq!(t.update(100, false), None, "şarjdan çıkınca bildirim yok");
        assert_eq!(t.update(100, true), Some(BatteryEvent::Full));
        assert_eq!(t.update(97, true), None);
        assert_eq!(t.update(100, true), Some(BatteryEvent::Full));
    }
}
