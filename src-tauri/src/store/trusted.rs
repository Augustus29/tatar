//! Güvenilir (eşleşmiş) cihazlar. Sertifika (genel) burada saklanır; özel anahtar burada DEĞİL.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrustedDevice {
    pub device_id: String,
    pub name: String,
    pub device_type: String,
    /// Eşin sertifikası (DER, onaltılık).
    #[serde(with = "hex_bytes")]
    pub cert: Vec<u8>,
    pub protocol_version: i64,
    #[serde(default)]
    pub last_ip: Option<String>,
    #[serde(default)]
    pub last_port: Option<u16>,
    /// Son görülme, epoch ms.
    #[serde(default)]
    pub last_seen: Option<i64>,
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(v))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        hex::decode(s).map_err(serde::de::Error::custom)
    }
}

pub struct TrustedStore {
    path: Option<PathBuf>,
    devices: Mutex<BTreeMap<String, TrustedDevice>>,
}

impl TrustedStore {
    /// `path = None` -> yalnızca bellekte (testler).
    pub fn load(path: Option<PathBuf>) -> TrustedStore {
        let devices = path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<Vec<TrustedDevice>>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            // Sertifikasız "güvenilir" cihaz olamaz.
            .filter(|d| !d.cert.is_empty())
            .map(|d| (d.device_id.clone(), d))
            .collect();
        TrustedStore {
            path,
            devices: Mutex::new(devices),
        }
    }

    fn save(&self, map: &BTreeMap<String, TrustedDevice>) {
        let Some(path) = &self.path else { return };
        let list: Vec<&TrustedDevice> = map.values().collect();
        match serde_json::to_vec_pretty(&list) {
            Ok(data) => {
                if let Err(e) = super::write_atomic(path, &data) {
                    log::warn!("güvenilir cihazlar kaydedilemedi: {e}");
                }
            }
            Err(e) => log::warn!("güvenilir cihazlar serileştirilemedi: {e}"),
        }
    }

    pub fn get(&self, id: &str) -> Option<TrustedDevice> {
        self.devices.lock().unwrap().get(id).cloned()
    }
    pub fn is_trusted(&self, id: &str) -> bool {
        self.devices.lock().unwrap().contains_key(id)
    }
    pub fn list(&self) -> Vec<TrustedDevice> {
        self.devices.lock().unwrap().values().cloned().collect()
    }
    pub fn upsert(&self, d: TrustedDevice) {
        let mut m = self.devices.lock().unwrap();
        m.insert(d.device_id.clone(), d);
        self.save(&m);
    }
    pub fn remove(&self, id: &str) -> bool {
        let mut m = self.devices.lock().unwrap();
        let had = m.remove(id).is_some();
        if had {
            self.save(&m);
        }
        had
    }
    pub fn clear(&self) {
        let mut m = self.devices.lock().unwrap();
        m.clear();
        self.save(&m);
    }
    /// Son IP/port/görülme ve (varsa) ad bilgisini günceller. Yalnızca DOĞRULANMIŞ bir bağlantıdan
    /// (TLS + sertifika sabitleme geçmiş) çağrılmalı; UDP/mDNS keşfinden asla (sahte duyuru
    /// kayıtlı adresi bozabilir). `port` boşsa eski port korunur.
    pub fn touch(
        &self,
        id: &str,
        ip: Option<String>,
        port: Option<u16>,
        name: Option<&str>,
        seen: i64,
    ) {
        let mut m = self.devices.lock().unwrap();
        if let Some(d) = m.get_mut(id) {
            if ip.is_some() {
                d.last_ip = ip;
                if port.is_some() {
                    d.last_port = port;
                }
            }
            if let Some(n) = name {
                d.name = n.to_string();
            }
            d.last_seen = Some(seen);
            self.save(&m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(id: &str) -> TrustedDevice {
        TrustedDevice {
            device_id: id.into(),
            name: "Telefon".into(),
            device_type: "phone".into(),
            cert: vec![1, 2, 3],
            protocol_version: 8,
            last_ip: Some("192.168.1.5".into()),
            last_port: Some(1716),
            last_seen: None,
        }
    }

    #[test]
    fn persists_and_reloads() {
        let p = std::env::temp_dir().join(format!(
            "tatar-trusted-{}.json",
            crate::protocol::identity::generate_device_id()
        ));
        let s = TrustedStore::load(Some(p.clone()));
        s.upsert(dev("abc"));
        assert!(s.is_trusted("abc"));
        let s2 = TrustedStore::load(Some(p.clone()));
        assert_eq!(s2.get("abc").unwrap(), dev("abc"));
        s2.touch("abc", Some("10.0.0.2".into()), Some(1720), None, 5);
        assert_eq!(
            TrustedStore::load(Some(p.clone()))
                .get("abc")
                .unwrap()
                .last_ip
                .as_deref(),
            Some("10.0.0.2")
        );
        // Port verilmezse eski port korunur.
        s2.touch("abc", Some("10.0.0.3".into()), None, None, 6);
        assert_eq!(s2.get("abc").unwrap().last_port, Some(1720));
        assert!(s2.remove("abc"));
        assert!(TrustedStore::load(Some(p.clone())).list().is_empty());
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn corrupt_file_is_empty() {
        let p = std::env::temp_dir().join("tatar-trusted-corrupt.json");
        std::fs::write(&p, b"{bozuk").unwrap();
        assert!(TrustedStore::load(Some(p.clone())).list().is_empty());
        let _ = std::fs::remove_file(p);
    }
}
