//! `kdeconnect.notification` (yalnızca telefondan bize). Telefondaki bildirimler sistem
//! bildirimi olarak yansıtılır. Simgeler (`payloadHash` + yük) yok sayılır: yük için soket
//! açılmaz, paket yalnızca metin alanlarıyla işlenir. İçerik asla loglanmaz (yalnızca sayaç).

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::notifier::SharedNotifier;
use super::{DeviceHandle, Plugin, Shared};
use crate::i18n::{notify_text, resolve_language};
use crate::protocol::packet::NetworkPacket;
use crate::transfer::names::clean_display_text;

pub const TYPE_NOTIFICATION: &str = "kdeconnect.notification";

const MAX_APP: usize = 60;
const MAX_TITLE: usize = 100;
const MAX_TEXT: usize = 300;
/// Bildirim seli sınırı: bu pencere içinde en çok bu kadar bildirim gösterilir.
const RATE_MAX: usize = 5;
const RATE_WINDOW: Duration = Duration::from_secs(10);
/// Bellekte tutulan "son gösterilen içerik" kaydı üst sınırı.
const MAX_SEEN: usize = 512;

#[derive(Default)]
struct State {
    /// (cihaz, bildirim kimliği) -> son içeriğin özeti (tekrar göstermemek için).
    seen: HashMap<(String, String), u64>,
    shown: VecDeque<Instant>,
}

pub struct NotificationPlugin {
    shared: Arc<Shared>,
    notifier: SharedNotifier,
    state: Mutex<State>,
}

impl NotificationPlugin {
    pub fn new(shared: Arc<Shared>, notifier: SharedNotifier) -> Self {
        NotificationPlugin {
            shared,
            notifier,
            state: Mutex::new(State::default()),
        }
    }

    fn handle_at(&self, device: &DeviceHandle, packet: &NetworkPacket, now: Instant) {
        let settings = self.shared.settings();
        // Yetenek ilan edilmiş kalır (basitlik); kapalıyken paketler sessizce atılır.
        if !settings.notification_mirror {
            return;
        }
        let Some(id) = packet
            .body_str("id")
            .filter(|s| !s.is_empty() && s.len() <= 512)
        else {
            log::debug!("bildirim paketinde kimlik yok, yok sayıldı");
            return;
        };
        let key = (device.id.clone(), id.to_string());

        // Telefon bildirimi kaldırdı: yalnızca durumdan sil, gösterme.
        if packet.body_bool("isCancel").unwrap_or(false) {
            self.state.lock().unwrap().seen.remove(&key);
            return;
        }

        let app = clean_display_text(packet.body_str("appName").unwrap_or(""), MAX_APP);
        let title = clean_display_text(packet.body_str("title").unwrap_or(""), MAX_TITLE);
        let mut text = clean_display_text(packet.body_str("text").unwrap_or(""), MAX_TEXT);
        if title.is_empty() && text.is_empty() {
            text = clean_display_text(packet.body_str("ticker").unwrap_or(""), MAX_TEXT);
        }
        let summary = summary_of(&title, &text);

        {
            let mut st = self.state.lock().unwrap();
            if st.seen.len() >= MAX_SEEN && !st.seen.contains_key(&key) {
                st.seen.clear();
            }
            let previous = st.seen.insert(key, summary);
            // Telefon `silent` derse (bağlanırken var olanlar ya da "yalnızca bir kez uyar"
            // güncellemeleri) uyarı verilmez; ama içerik kaydedilir.
            if packet.body_bool("silent").unwrap_or(false) {
                return;
            }
            // Aynı kimlik, aynı içerik: tekrar gösterme.
            if previous == Some(summary) {
                return;
            }
            // Sel koruması.
            while st
                .shown
                .front()
                .is_some_and(|t| now.duration_since(*t) > RATE_WINDOW)
            {
                st.shown.pop_front();
            }
            if st.shown.len() >= RATE_MAX {
                log::debug!("bildirim sınırı aşıldı, bir bildirim atlandı");
                return;
            }
            st.shown.push_back(now);
        }

        let lang = resolve_language(&settings.language);
        let app = if app.is_empty() {
            device.name.clone()
        } else {
            app
        };
        let generic = settings.notification_hide_content || (title.is_empty() && text.is_empty());
        let (head, body) = if generic {
            (
                format!("{app}: {}", notify_text(lang, "mirrorGeneric", &[])),
                device.name.clone(),
            )
        } else if title.is_empty() {
            (app, text)
        } else {
            (format!("{app}: {title}"), text)
        };
        log::info!("telefon bildirimi gösteriliyor");
        self.notifier.notify(&head, &body);
    }
}

fn summary_of(title: &str, text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    title.hash(&mut h);
    text.hash(&mut h);
    h.finish()
}

impl Plugin for NotificationPlugin {
    fn name(&self) -> &'static str {
        "notification"
    }
    fn incoming_types(&self) -> &'static [&'static str] {
        &[TYPE_NOTIFICATION]
    }
    fn outgoing_types(&self) -> &'static [&'static str] {
        &[]
    }
    fn handle(&self, device: &DeviceHandle, packet: &NetworkPacket) {
        self.handle_at(device, packet, Instant::now());
    }
    fn device_disconnected(&self, device_id: &str) {
        self.state
            .lock()
            .unwrap()
            .seen
            .retain(|(d, _), _| d != device_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::notifier::Notifier;
    use crate::store::{Settings, SettingsPatch, SettingsStore};
    use serde_json::json;
    use tokio::sync::mpsc;

    #[derive(Default)]
    struct Fake(Mutex<Vec<(String, String)>>);
    impl Notifier for Fake {
        fn notify(&self, t: &str, b: &str) {
            self.0.lock().unwrap().push((t.into(), b.into()));
        }
    }

    fn setup(patch: SettingsPatch) -> (NotificationPlugin, Arc<Fake>, DeviceHandle) {
        let shared = Arc::new(Shared::default());
        let store = Arc::new(SettingsStore::load(None));
        store.patch(patch);
        shared.set_settings(store);
        let fake = Arc::new(Fake::default());
        let p = NotificationPlugin::new(shared, fake.clone());
        let (tx, _rx) = mpsc::channel(1);
        let dev = DeviceHandle {
            id: "ph".into(),
            name: "Phone".into(),
            peer_incoming: Arc::new(vec![]),
            tx,
            peer_ip: None,
            peer_cert: Arc::new(Vec::new()),
        };
        (p, fake, dev)
    }

    fn pkt(body: serde_json::Value) -> NetworkPacket {
        NetworkPacket::new(TYPE_NOTIFICATION, body)
    }

    fn lang_patch() -> SettingsPatch {
        SettingsPatch {
            language: Some("en".into()),
            ..Default::default()
        }
    }

    #[test]
    fn shows_notification() {
        let (p, f, d) = setup(lang_patch());
        p.handle(
            &d,
            &pkt(json!({"id":"1","appName":"Mail","title":"Hi","text":"Body","silent":false})),
        );
        let v = f.0.lock().unwrap();
        assert_eq!(
            v.as_slice(),
            &[("Mail: Hi".to_string(), "Body".to_string())]
        );
    }

    #[test]
    fn cancel_and_silent_are_ignored() {
        let (p, f, d) = setup(lang_patch());
        p.handle(&d, &pkt(json!({"id":"1","isCancel":true})));
        p.handle(&d, &pkt(json!({"id":"2","isCancel":"true","title":"x"})));
        p.handle(&d, &pkt(json!({"id":"3","title":"x","silent":true})));
        assert!(f.0.lock().unwrap().is_empty());
    }

    #[test]
    fn dedupe_and_cancel_resets() {
        let (p, f, d) = setup(lang_patch());
        let n = pkt(json!({"id":"1","appName":"A","title":"t","text":"x"}));
        p.handle(&d, &n);
        p.handle(&d, &n);
        assert_eq!(f.0.lock().unwrap().len(), 1);
        p.handle(
            &d,
            &pkt(json!({"id":"1","appName":"A","title":"t","text":"y"})),
        );
        assert_eq!(f.0.lock().unwrap().len(), 2);
        p.handle(&d, &pkt(json!({"id":"1","isCancel":true})));
        p.handle(
            &d,
            &pkt(json!({"id":"1","appName":"A","title":"t","text":"y"})),
        );
        assert_eq!(f.0.lock().unwrap().len(), 3);
    }

    #[test]
    fn silent_records_content_so_repeat_is_not_shown() {
        let (p, f, d) = setup(lang_patch());
        p.handle(&d, &pkt(json!({"id":"1","title":"t","silent":true})));
        p.handle(&d, &pkt(json!({"id":"1","title":"t","silent":false})));
        assert!(f.0.lock().unwrap().is_empty());
    }

    #[test]
    fn disabled_setting_drops() {
        let (p, f, d) = setup(SettingsPatch {
            notification_mirror: Some(false),
            ..lang_patch()
        });
        p.handle(&d, &pkt(json!({"id":"1","title":"t"})));
        assert!(f.0.lock().unwrap().is_empty());
        assert!(Settings::default().notification_mirror);
    }

    #[test]
    fn hidden_content_shows_only_app() {
        let (p, f, d) = setup(SettingsPatch {
            notification_hide_content: Some(true),
            ..lang_patch()
        });
        p.handle(
            &d,
            &pkt(json!({"id":"1","appName":"Bank","title":"Secret","text":"1234"})),
        );
        let v = f.0.lock().unwrap();
        assert_eq!(v.len(), 1);
        assert!(v[0].0.starts_with("Bank: "));
        assert!(!v[0].0.contains("Secret") && !v[0].1.contains("1234"));
    }

    #[test]
    fn sanitizes_and_limits() {
        let (p, f, d) = setup(lang_patch());
        let long = "a".repeat(1000);
        p.handle(
            &d,
            &pkt(json!({"id":"1","appName":"A\u{202e}pp","title":"T\nx","text":long})),
        );
        let v = f.0.lock().unwrap();
        assert_eq!(v[0].0, "App: T x");
        assert_eq!(v[0].1.chars().count(), MAX_TEXT);
    }

    #[test]
    fn rate_limited_then_recovers() {
        let (p, f, d) = setup(lang_patch());
        let t0 = Instant::now();
        for i in 0..10 {
            p.handle_at(&d, &pkt(json!({"id":format!("n{i}"),"title":"t"})), t0);
        }
        assert_eq!(f.0.lock().unwrap().len(), RATE_MAX);
        p.handle_at(
            &d,
            &pkt(json!({"id":"late","title":"t"})),
            t0 + RATE_WINDOW + Duration::from_secs(1),
        );
        assert_eq!(f.0.lock().unwrap().len(), RATE_MAX + 1);
    }

    #[test]
    fn only_advertises_incoming_notification() {
        let (p, _, _) = setup(lang_patch());
        assert_eq!(p.incoming_types(), &[TYPE_NOTIFICATION]);
        assert!(p.outgoing_types().is_empty());
    }
}
