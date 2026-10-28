//! Kullanıcı ayarları. Alan adları ve varsayılanlar `src/lib/types.ts` ile birebir aynıdır.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Notifications {
    pub file_received: bool,
    pub battery_low: bool,
}

impl Default for Notifications {
    fn default() -> Self {
        Notifications {
            file_received: true,
            battery_low: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// "system" | "tr" | "en"
    pub language: String,
    /// "auto" | "manual" | "off"
    pub clipboard_mode: String,
    pub mute_on_call: bool,
    /// "mute" | "lower"
    pub mute_mode: String,
    pub download_dir: String,
    pub launch_at_login: bool,
    pub show_in_menu_bar: bool,
    pub show_in_dock: bool,
    pub shortcuts: BTreeMap<String, Vec<String>>,
    pub notifications: Notifications,
    /// Boş = otomatik ("Tatar / <bilgisayar adı>").
    pub device_name: String,
    pub share_battery: bool,
    pub auto_connect: bool,
    pub onboarded: bool,
    /// Telefon bildirimlerini sistem bildirimi olarak göster.
    pub notification_mirror: bool,
    /// Yansıtılan bildirimde yalnızca uygulama adı gösterilir.
    pub notification_hide_content: bool,
}

pub fn default_shortcuts() -> BTreeMap<String, Vec<String>> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    BTreeMap::from([
        ("sendClip".to_string(), s(&["⇧", "⌘", "C"])),
        ("sendFile".to_string(), s(&["⇧", "⌘", "F"])),
        ("findDev".to_string(), s(&["⌥", "⌘", "R"])),
        ("openWin".to_string(), s(&["⌥", "⌘", "T"])),
        ("showHist".to_string(), s(&["⇧", "⌘", "V"])),
    ])
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: "system".into(),
            clipboard_mode: "auto".into(),
            mute_on_call: true,
            mute_mode: "mute".into(),
            download_dir: String::new(),
            launch_at_login: false,
            show_in_menu_bar: true,
            show_in_dock: false,
            shortcuts: default_shortcuts(),
            notifications: Notifications::default(),
            device_name: String::new(),
            share_battery: true,
            auto_connect: true,
            // Taze kurulumda ilk açılış akışı gösterilir (arayüz, yüklenmeden önce `true` varsayar).
            onboarded: false,
            notification_mirror: true,
            notification_hide_content: false,
        }
    }
}

/// `SettingsPatch` = kısmi ayar (kısayollar hariç; onlar `set_shortcut` ile değişir).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub language: Option<String>,
    pub clipboard_mode: Option<String>,
    pub mute_on_call: Option<bool>,
    pub mute_mode: Option<String>,
    pub download_dir: Option<String>,
    pub launch_at_login: Option<bool>,
    pub show_in_menu_bar: Option<bool>,
    pub show_in_dock: Option<bool>,
    pub notifications: Option<Notifications>,
    pub device_name: Option<String>,
    pub share_battery: Option<bool>,
    pub auto_connect: Option<bool>,
    pub onboarded: Option<bool>,
    pub notification_mirror: Option<bool>,
    pub notification_hide_content: Option<bool>,
}

fn one_of(v: &str, allowed: &[&str]) -> bool {
    allowed.contains(&v)
}

impl Settings {
    /// Geçersiz değerleri varsayılana çevirir (bozuk dosya / kötü yama).
    pub fn sanitized(mut self) -> Settings {
        let d = Settings::default();
        if !one_of(&self.language, &["system", "tr", "en"]) {
            self.language = d.language;
        }
        if !one_of(&self.clipboard_mode, &["auto", "manual", "off"]) {
            self.clipboard_mode = d.clipboard_mode;
        }
        if !one_of(&self.mute_mode, &["mute", "lower"]) {
            self.mute_mode = d.mute_mode;
        }
        // İndirme klasörü webview'dan gelebilir: yalnızca var olan, hassas olmayan bir klasör.
        // Geçersizse varsayılan İndirilenler'e (o da uygun değilse boşa = uygulama varsayılanı) döner.
        if !self.download_dir.trim().is_empty()
            && !crate::transfer::safety::download_dir_allowed(&self.download_dir)
        {
            log::warn!("geçersiz indirme klasörü reddedildi, varsayılana dönüldü");
            self.download_dir = crate::transfer::safety::home_dir()
                .map(|h| h.join("Downloads"))
                .filter(|d| crate::transfer::safety::download_dir_allowed(&d.to_string_lossy()))
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        // Kayıp kısayol kimliklerini varsayılanlarla tamamla.
        for (k, v) in default_shortcuts() {
            self.shortcuts.entry(k).or_insert(v);
        }
        self.device_name = crate::protocol::identity::filter_device_name(&self.device_name);
        self
    }

    pub fn apply(&mut self, p: SettingsPatch) {
        macro_rules! set {
            ($($f:ident),*) => { $( if let Some(v) = p.$f { self.$f = v; } )* };
        }
        set!(
            language,
            clipboard_mode,
            mute_on_call,
            mute_mode,
            download_dir,
            launch_at_login,
            show_in_menu_bar,
            show_in_dock,
            notifications,
            device_name,
            share_battery,
            auto_connect,
            onboarded,
            notification_mirror,
            notification_hide_content
        );
        *self = std::mem::take(self).sanitized();
    }
}

pub struct SettingsStore {
    path: Option<PathBuf>,
    inner: Mutex<Settings>,
}

impl SettingsStore {
    pub fn load(path: Option<PathBuf>) -> SettingsStore {
        let s = path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .unwrap_or_default()
            .sanitized();
        SettingsStore {
            path,
            inner: Mutex::new(s),
        }
    }

    pub fn get(&self) -> Settings {
        self.inner.lock().unwrap().clone()
    }

    pub fn patch(&self, p: SettingsPatch) -> Settings {
        let mut g = self.inner.lock().unwrap();
        g.apply(p);
        self.save(&g);
        g.clone()
    }

    pub fn set_shortcut(&self, id: &str, keys: Vec<String>) -> Option<Settings> {
        let mut g = self.inner.lock().unwrap();
        if !g.shortcuts.contains_key(id) {
            return None;
        }
        g.shortcuts.insert(id.to_string(), keys);
        self.save(&g);
        Some(g.clone())
    }

    fn save(&self, s: &Settings) {
        let Some(path) = &self.path else { return };
        match serde_json::to_vec_pretty(s) {
            Ok(data) => {
                if let Err(e) = super::write_atomic(path, &data) {
                    log::warn!("ayarlar kaydedilemedi: {e}");
                }
            }
            Err(e) => log::warn!("ayarlar serileştirilemedi: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_frontend_contract() {
        let v = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(v["language"], "system");
        assert_eq!(v["showInDock"], false);
        assert_eq!(v["showInMenuBar"], true);
        assert_eq!(v["notifications"]["fileReceived"], true);
        assert_eq!(
            v["shortcuts"]["sendClip"],
            serde_json::json!(["⇧", "⌘", "C"])
        );
        assert_eq!(v["deviceName"], "");
        assert_eq!(v["onboarded"], false);
    }

    #[test]
    fn missing_fields_get_defaults_and_bad_values_fixed() {
        let s: Settings = serde_json::from_str(r#"{"language":"xx","muteOnCall":false}"#).unwrap();
        let s = s.sanitized();
        assert_eq!(s.language, "system");
        assert!(!s.mute_on_call);
        assert_eq!(s.clipboard_mode, "auto");
        assert_eq!(s.shortcuts.len(), 5);
    }

    #[test]
    fn invalid_download_dir_falls_back() {
        let store = SettingsStore::load(None);
        for bad in [
            "goreli/yol",
            "/etc",
            "/olmayan/klasor/xyz",
            "/System/Library",
        ] {
            let s = store.patch(SettingsPatch {
                download_dir: Some(bad.into()),
                ..Default::default()
            });
            assert_ne!(s.download_dir, bad, "{bad}");
        }
        let tmp = std::env::temp_dir();
        let s = store.patch(SettingsPatch {
            download_dir: Some(tmp.to_string_lossy().into_owned()),
            ..Default::default()
        });
        assert_eq!(s.download_dir, tmp.to_string_lossy());
    }

    #[test]
    fn patch_applies_and_filters_name() {
        let store = SettingsStore::load(None);
        let s = store.patch(SettingsPatch {
            device_name: Some("Evim (Mac).".into()),
            clipboard_mode: Some("bogus".into()),
            mute_mode: Some("lower".into()),
            ..Default::default()
        });
        assert_eq!(s.device_name, "Evim Mac");
        assert_eq!(s.clipboard_mode, "auto");
        assert_eq!(s.mute_mode, "lower");
        assert!(store.set_shortcut("nope", vec![]).is_none());
        assert!(store.set_shortcut("sendClip", vec!["A".into()]).is_some());
    }
}
