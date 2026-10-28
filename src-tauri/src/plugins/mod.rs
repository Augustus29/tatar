//! Eklenti altyapısı. Pil, ping, pano, paylaşım ve telefon eklentileri sonraki aşamalarda
//! bu klasöre modül olarak eklenir; çekirdeğe dokunmak gerekmez: yalnızca `Registry`'ye kaydolurlar.
//!
//! Çekirdek, bir paketi eklentiye YALNIZCA eşleşmiş cihazdan geldiyse iletir.

pub mod clipboard;

use std::collections::{BTreeSet, HashMap};
use std::net::IpAddr;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{Value, json};
use tokio::sync::{Notify, mpsc};

use crate::protocol::packet::NetworkPacket;
use crate::store::{Settings, SettingsStore};

pub mod battery;
pub mod callmedia;
pub mod findmyphone;
pub mod mpris;
pub mod notifier;
pub mod ping;
pub mod telephony;

/// Eklentiler ile yönetici arasında paylaşılan durum: ayarlar, pil bilgisi, arayüz yenileme isteği.
#[derive(Default)]
pub struct Shared {
    settings: OnceLock<Arc<SettingsStore>>,
    battery: Mutex<HashMap<String, (i64, bool)>>,
    /// Değişince yönetici `devices-changed` yayar.
    pub refresh: Notify,
}

impl Shared {
    pub fn set_settings(&self, s: Arc<SettingsStore>) {
        let _ = self.settings.set(s);
    }
    /// Henüz bağlanmadıysa (yalnızca testlerde) varsayılan ayarlar.
    pub fn settings(&self) -> Settings {
        self.settings.get().map(|s| s.get()).unwrap_or_default()
    }
    pub fn set_battery(&self, device_id: &str, charge: i64, charging: bool) {
        self.battery
            .lock()
            .unwrap()
            .insert(device_id.to_string(), (charge, charging));
        self.refresh.notify_one();
    }
    pub fn clear_battery(&self, device_id: &str) {
        if self.battery.lock().unwrap().remove(device_id).is_some() {
            self.refresh.notify_one();
        }
    }
    /// Arayüzün `Battery` biçimi: `{charge, charging}`.
    pub fn battery_of(&self, device_id: &str) -> Option<Value> {
        self.battery
            .lock()
            .unwrap()
            .get(device_id)
            .map(|(c, ch)| json!({"charge": c, "charging": ch}))
    }
}

/// Bir eklentinin paket gönderebileceği, eşleşmiş ve bağlı bir cihaz.
#[derive(Clone)]
pub struct DeviceHandle {
    pub id: String,
    pub name: String,
    /// Karşı tarafın `incomingCapabilities` listesi; ilan etmediği türü göndermeyiz.
    pub peer_incoming: Arc<Vec<String>>,
    pub tx: mpsc::Sender<NetworkPacket>,
    /// Kontrol bağlantısının karşı IP'si (dosya yükü soketleri hep bu adrese bağlanır/bundan kabul eder).
    pub peer_ip: Option<IpAddr>,
    /// Eşin sabitlenmiş sertifikası (DER); yük soketlerinde TLS doğrulaması için.
    pub peer_cert: Arc<Vec<u8>>,
}

impl DeviceHandle {
    /// Paketi gönderir. Karşı taraf bu türü kabul etmiyorsa ya da kuyruk doluysa `false`.
    pub fn send(&self, packet: NetworkPacket) -> bool {
        if !self.peer_incoming.contains(&packet.packet_type) {
            log::debug!(
                "{} türü karşı tarafın yeteneklerinde yok, gönderilmedi",
                crate::diag::clean(&packet.packet_type)
            );
            return false;
        }
        self.tx.try_send(packet).is_ok()
    }
}

pub mod share;

pub trait Plugin: Send + Sync {
    fn name(&self) -> &'static str;
    /// Almayı vaat ettiğimiz paket türleri (`incomingCapabilities`).
    fn incoming_types(&self) -> &'static [&'static str];
    /// Gönderebileceğimiz paket türleri (`outgoingCapabilities`).
    fn outgoing_types(&self) -> &'static [&'static str];
    /// Engellemeden hızlı dönmeli; ağır işi kendi görevinde yapmalı. Paket içeriği loglanmaz.
    fn handle(&self, device: &DeviceHandle, packet: &NetworkPacket);
    /// Eşleşmiş bir cihazın bağlantısı kurulduğunda (ya da bağlıyken eşleşme tamamlandığında).
    fn device_connected(&self, _device: &DeviceHandle) {}
    /// Eşleşmiş cihazın bağlantısı koptuğunda ya da eşleşme kaldırıldığında.
    fn device_disconnected(&self, _device_id: &str) {}
}

#[derive(Default, Clone)]
pub struct Registry {
    plugins: Vec<Arc<dyn Plugin>>,
    shared: Arc<Shared>,
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    pub fn shared(&self) -> Arc<Shared> {
        self.shared.clone()
    }

    pub fn register(&mut self, plugin: Arc<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    /// İlan edilecek yetenekler: yalnızca kayıtlı eklentilerin gerçekten desteklediği türler.
    pub fn incoming_capabilities(&self) -> Vec<String> {
        collect(self.plugins.iter().flat_map(|p| p.incoming_types()))
    }
    pub fn outgoing_capabilities(&self) -> Vec<String> {
        collect(self.plugins.iter().flat_map(|p| p.outgoing_types()))
    }

    pub fn connected(&self, device: &DeviceHandle) {
        for p in &self.plugins {
            p.device_connected(device);
        }
    }

    pub fn disconnected(&self, device_id: &str) {
        for p in &self.plugins {
            p.device_disconnected(device_id);
        }
    }

    /// Paketi türünü işleyen eklentilere verir. Hiçbiri yoksa `false` (bilinmeyen tür).
    pub fn dispatch(&self, device: &DeviceHandle, packet: &NetworkPacket) -> bool {
        let mut handled = false;
        for p in &self.plugins {
            if p.incoming_types().contains(&packet.packet_type.as_str()) {
                p.handle(device, packet);
                handled = true;
            }
        }
        handled
    }
}

fn collect<'a>(it: impl Iterator<Item = &'a &'static str>) -> Vec<String> {
    it.map(|s| s.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Counter(AtomicUsize);
    impl Plugin for Counter {
        fn name(&self) -> &'static str {
            "test"
        }
        fn incoming_types(&self) -> &'static [&'static str] {
            &["kdeconnect.ping"]
        }
        fn outgoing_types(&self) -> &'static [&'static str] {
            &["kdeconnect.ping"]
        }
        fn handle(&self, _d: &DeviceHandle, _p: &NetworkPacket) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn registry_dispatch_and_caps() {
        assert!(Registry::new().incoming_capabilities().is_empty());
        let c = Arc::new(Counter(AtomicUsize::new(0)));
        let mut r = Registry::new();
        r.register(c.clone());
        assert_eq!(r.incoming_capabilities(), vec!["kdeconnect.ping"]);
        let (tx, mut rx) = mpsc::channel(4);
        let dev = DeviceHandle {
            id: "x".into(),
            name: "x".into(),
            peer_incoming: Arc::new(vec!["kdeconnect.ping".into()]),
            tx,
            peer_ip: None,
            peer_cert: Arc::new(Vec::new()),
        };
        let p = NetworkPacket::new("kdeconnect.ping", serde_json::json!({}));
        assert!(r.dispatch(&dev, &p));
        assert_eq!(c.0.load(Ordering::SeqCst), 1);
        assert!(!r.dispatch(
            &dev,
            &NetworkPacket::new("kdeconnect.other", serde_json::json!({}))
        ));
        assert!(dev.send(p));
        assert!(rx.try_recv().is_ok());
        assert!(!dev.send(NetworkPacket::new(
            "kdeconnect.battery",
            serde_json::json!({})
        )));
    }
}
