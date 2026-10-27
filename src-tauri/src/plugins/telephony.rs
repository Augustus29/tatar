//! `kdeconnect.telephony` (telefon -> Tatar). Gelen aramada sistem sesini kapatır / azaltır,
//! arama bitince (ya da bağlantı kopunca) önceki durumu aynen geri yükler.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use super::notifier::SharedNotifier;
use super::{DeviceHandle, Plugin, Shared};
use crate::i18n::{notify_text, resolve_language};
use crate::platform::audio::{AudioBackend, OutputState};
use crate::protocol::packet::NetworkPacket;

pub const TYPE_TELEPHONY: &str = "kdeconnect.telephony";
/// "Sesi azalt" kipinde yeni ses = eski sesin bu oranı.
pub const LOWER_FACTOR: f32 = 0.2;

/// Aramada sese ne yapıldı?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    Muted,
    Lowered,
    /// Ses zaten kapalıydı ya da okunamadı: dokunulmadı, geri yükleme de yapılmayacak.
    Untouched,
}

#[derive(Default)]
struct Inner {
    /// Şu an çalan/etkin aramaya sahip cihazlar (örtüşen aramalar için).
    owners: HashSet<String>,
    /// Bizim değiştirmeden önceki durum; yalnızca gerçekten değiştirdiysek dolu.
    saved: Option<OutputState>,
}

/// Arama -> ses durum makinesi. Ses arka ucu soyutlandığı için sahte bir uçla test edilir.
pub struct CallMuter<A: AudioBackend> {
    audio: A,
    inner: Mutex<Inner>,
}

impl<A: AudioBackend> CallMuter<A> {
    pub fn new(audio: A) -> Self {
        CallMuter {
            audio,
            inner: Mutex::new(Inner::default()),
        }
    }

    /// Cihazdan "çalıyor" geldi. Aynı cihazdan tekrar gelirse ya da başka arama zaten
    /// sesi değiştirdiyse bir şey yapmaz (`None`). `lower` = true: sesi azalt, false: kapat.
    pub fn ringing(&self, device_id: &str, lower: bool) -> Option<Applied> {
        let mut g = self.inner.lock().unwrap();
        if !g.owners.insert(device_id.to_string()) || g.owners.len() > 1 {
            return None;
        }
        let state = self.audio.get()?;
        if state.muted {
            return Some(Applied::Untouched); // zaten kapalı: açmayacağız
        }
        let target = if lower {
            OutputState {
                muted: false,
                volume: (state.volume * LOWER_FACTOR).clamp(0.0, 1.0),
            }
        } else {
            OutputState {
                muted: true,
                volume: state.volume,
            }
        };
        if !self.audio.set(target) {
            return Some(Applied::Untouched);
        }
        g.saved = Some(state);
        Some(if lower {
            Applied::Lowered
        } else {
            Applied::Muted
        })
    }

    /// Cihazın araması bitti (ya da bağlantısı koptu). Son arama bittiyse sesi geri yükler.
    /// Geri yükleme yapıldıysa `true`.
    pub fn ended(&self, device_id: &str) -> bool {
        let mut g = self.inner.lock().unwrap();
        if !g.owners.remove(device_id) || !g.owners.is_empty() {
            return false;
        }
        match g.saved.take() {
            Some(prev) => self.audio.set(prev),
            None => false,
        }
    }
}

pub struct TelephonyPlugin<A: AudioBackend + 'static> {
    shared: Arc<Shared>,
    notifier: SharedNotifier,
    muter: CallMuter<A>,
}

impl<A: AudioBackend + 'static> TelephonyPlugin<A> {
    pub fn new(shared: Arc<Shared>, notifier: SharedNotifier, audio: A) -> Self {
        TelephonyPlugin {
            shared,
            notifier,
            muter: CallMuter::new(audio),
        }
    }
}

impl<A: AudioBackend + 'static> Plugin for TelephonyPlugin<A> {
    fn name(&self) -> &'static str {
        "telephony"
    }
    fn incoming_types(&self) -> &'static [&'static str] {
        &[TYPE_TELEPHONY]
    }
    fn outgoing_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn handle(&self, device: &DeviceHandle, packet: &NetworkPacket) {
        // Android `isCancel`ı dize olarak yollar; `body_bool` hoşgörülüdür.
        let cancel = packet.body_bool("isCancel").unwrap_or(false);
        match (packet.body_str("event"), cancel) {
            (Some("ringing"), false) => {
                let settings = self.shared.settings();
                if !settings.mute_on_call {
                    return;
                }
                let lower = settings.mute_mode == "lower";
                let Some(applied) = self.muter.ringing(&device.id, lower) else {
                    return;
                };
                let lang = resolve_language(&settings.language);
                let (t, b) = match applied {
                    Applied::Muted => ("callTitle", "callBody"),
                    Applied::Lowered => ("callLoweredTitle", "callLoweredBody"),
                    Applied::Untouched => return,
                };
                self.notifier
                    .notify(&notify_text(lang, t, &[]), &notify_text(lang, b, &[]));
            }
            // Arama sona erdi (iptal) ya da cevapsız kaldı. Cevaplanınca (talking) görüşme
            // sürdüğü için ses kapalı kalır; görüşme bitince talking + isCancel gelir.
            (Some("ringing" | "talking"), true) | (Some("missedCall"), false) => {
                self.muter.ended(&device.id);
            }
            _ => {}
        }
    }

    fn device_disconnected(&self, device_id: &str) {
        self.muter.ended(device_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeAudio {
        state: Mutex<Option<OutputState>>,
        sets: Mutex<Vec<OutputState>>,
    }
    impl FakeAudio {
        fn new(muted: bool, volume: f32) -> Arc<Self> {
            Arc::new(FakeAudio {
                state: Mutex::new(Some(OutputState { muted, volume })),
                sets: Mutex::new(vec![]),
            })
        }
        fn current(&self) -> OutputState {
            self.state.lock().unwrap().unwrap()
        }
    }
    impl AudioBackend for Arc<FakeAudio> {
        fn get(&self) -> Option<OutputState> {
            *self.state.lock().unwrap()
        }
        fn set(&self, s: OutputState) -> bool {
            *self.state.lock().unwrap() = Some(s);
            self.sets.lock().unwrap().push(s);
            true
        }
    }

    fn st(muted: bool, volume: f32) -> OutputState {
        OutputState { muted, volume }
    }

    #[test]
    fn ringing_mutes_then_restores_exactly() {
        let a = FakeAudio::new(false, 0.65);
        let m = CallMuter::new(a.clone());
        assert_eq!(m.ringing("p", false), Some(Applied::Muted));
        assert_eq!(a.current(), st(true, 0.65));
        assert!(m.ended("p"));
        assert_eq!(a.current(), st(false, 0.65));
    }

    #[test]
    fn lower_mode_scales_and_restores() {
        let a = FakeAudio::new(false, 0.5);
        let m = CallMuter::new(a.clone());
        assert_eq!(m.ringing("p", true), Some(Applied::Lowered));
        let c = a.current();
        assert!(!c.muted);
        assert!((c.volume - 0.1).abs() < 1e-6);
        assert!(m.ended("p"));
        assert_eq!(a.current(), st(false, 0.5));
    }

    #[test]
    fn already_muted_is_left_alone() {
        let a = FakeAudio::new(true, 0.4);
        let m = CallMuter::new(a.clone());
        assert_eq!(m.ringing("p", false), Some(Applied::Untouched));
        assert!(!m.ended("p"), "geri yükleme yapılmamalı");
        assert_eq!(a.current(), st(true, 0.4));
        assert!(a.sets.lock().unwrap().is_empty(), "hiç yazma olmamalı");
    }

    #[test]
    fn duplicate_ringing_is_idempotent() {
        let a = FakeAudio::new(false, 0.8);
        let m = CallMuter::new(a.clone());
        assert_eq!(m.ringing("p", false), Some(Applied::Muted));
        assert_eq!(m.ringing("p", false), None);
        // İkinci "çalıyor" kayıtlı durumu ezmemeli (sessiz hali "önceki" sanılmamalı).
        assert!(m.ended("p"));
        assert_eq!(a.current(), st(false, 0.8));
        assert_eq!(a.sets.lock().unwrap().len(), 2);
    }

    #[test]
    fn overlapping_devices_restore_after_last() {
        let a = FakeAudio::new(false, 0.7);
        let m = CallMuter::new(a.clone());
        assert_eq!(m.ringing("a", false), Some(Applied::Muted));
        assert_eq!(m.ringing("b", false), None);
        assert!(!m.ended("a"));
        assert!(a.current().muted);
        assert!(m.ended("b"));
        assert_eq!(a.current(), st(false, 0.7));
    }

    #[test]
    fn cancel_without_ringing_does_nothing() {
        let a = FakeAudio::new(false, 0.7);
        let m = CallMuter::new(a.clone());
        assert!(!m.ended("p"));
        assert!(a.sets.lock().unwrap().is_empty());
        // Sonrasında normal akış yine çalışır.
        assert_eq!(m.ringing("p", false), Some(Applied::Muted));
        assert!(m.ended("p"));
        assert!(!m.ended("p"), "ikinci bitiş etkisiz");
        assert_eq!(a.current(), st(false, 0.7));
    }

    #[test]
    fn unreadable_audio_is_ignored_but_tracked() {
        let a = FakeAudio::new(false, 0.7);
        *a.state.lock().unwrap() = None;
        let m = CallMuter::new(a.clone());
        assert_eq!(m.ringing("p", false), None);
        assert!(!m.ended("p"));
    }

    #[test]
    fn plugin_handles_string_is_cancel_and_disconnect() {
        use crate::plugins::notifier::Notifier;
        use serde_json::json;
        use tokio::sync::mpsc;

        struct Quiet;
        impl Notifier for Quiet {
            fn notify(&self, _t: &str, _b: &str) {}
        }
        let a = FakeAudio::new(false, 0.9);
        let p = TelephonyPlugin::new(Arc::new(Shared::default()), Arc::new(Quiet), a.clone());
        let (tx, _rx) = mpsc::channel(1);
        let dev = DeviceHandle {
            id: "ph".into(),
            name: "Phone".into(),
            peer_incoming: Arc::new(vec![]),
            tx,
        };
        let pk = |body| NetworkPacket::new(TYPE_TELEPHONY, body);
        p.handle(&dev, &pk(json!({"event": "ringing"})));
        assert!(a.current().muted);
        p.handle(&dev, &pk(json!({"event": "ringing", "isCancel": "true"})));
        assert_eq!(a.current(), st(false, 0.9));
        // Bağlantı kopunca da geri yüklenir.
        p.handle(&dev, &pk(json!({"event": "ringing"})));
        assert!(a.current().muted);
        p.device_disconnected("ph");
        assert_eq!(a.current(), st(false, 0.9));
        // Cevaplanınca (talking) görüşme boyunca kapalı kalır, bitince geri yüklenir.
        p.handle(&dev, &pk(json!({"event": "ringing"})));
        p.handle(&dev, &pk(json!({"event": "talking"})));
        assert!(a.current().muted);
        p.handle(&dev, &pk(json!({"event": "talking", "isCancel": true})));
        assert_eq!(a.current(), st(false, 0.9));
    }
}
