//! Arama gelince çalan müziği duraklat, arama bitince YALNIZCA bizim duraklattığımızı sürdür.
//! Telefon eklentisi (`telephony.rs`) bu durum makinesini arama olaylarıyla besler.
//!
//! Medya arka ucu çağrıları engelleyebildiği için (alt süreç, WinRT) işler tek bir işçi iş
//! parçacığında sırayla yapılır; arama olayı işleyicisi hiç beklemez.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use crate::platform::media::{MediaAction, MediaBackend};

/// Arama olayı bu sürede devam etmezse (paket kayboldu...) duraklattıklarımız sürdürülür.
pub const RING_WATCHDOG: Duration = Duration::from_secs(120);

#[derive(Default)]
struct Inner {
    /// Çalan/etkin aramaya sahip cihazlar (örtüşen aramalar için).
    owners: HashSet<String>,
    /// Bizim duraklattığımız oynatıcılar.
    paused: Vec<String>,
}

enum Job {
    Ringing(String),
    Ended(String),
    Expire(String, u64),
}

pub struct CallMedia {
    backend: Arc<dyn MediaBackend>,
    inner: Mutex<Inner>,
    /// Cihaz başına son arama olayının damgası (bekçi köpeği bayat mı diye bakar).
    stamps: Mutex<(HashMap<String, u64>, u64)>,
    jobs: Mutex<Option<Sender<Job>>>,
    me: Weak<CallMedia>,
}

impl CallMedia {
    pub fn new(backend: Arc<dyn MediaBackend>) -> Arc<CallMedia> {
        Arc::new_cyclic(|me| CallMedia {
            backend,
            inner: Mutex::new(Inner::default()),
            stamps: Mutex::new((HashMap::new(), 0)),
            jobs: Mutex::new(None),
            me: me.clone(),
        })
    }

    // ------------------------------------------------------------------ eşzamanlı çekirdek

    /// Cihazdan "çalıyor" geldi: çalanları duraklatır. Duraklatılan sayısını döndürür.
    /// Aynı cihazdan tekrar ya da başka arama zaten sürerken hiçbir şey yapmaz.
    pub fn ringing(&self, device_id: &str) -> usize {
        let mut g = self.inner.lock().unwrap();
        if !g.owners.insert(device_id.to_string()) || g.owners.len() > 1 {
            return 0;
        }
        for p in self.backend.players() {
            let playing = self.backend.state(&p).is_some_and(|s| s.is_playing);
            if playing && self.backend.action(&p, MediaAction::Pause) {
                g.paused.push(p);
            }
        }
        g.paused.len()
    }

    /// Arama bitti (ya da bağlantı koptu). Son arama bittiyse, hâlâ duran ve bizim
    /// duraklattığımız oynatıcıları sürdürür. Sürdürülen sayısını döndürür.
    pub fn ended(&self, device_id: &str) -> usize {
        self.stamps.lock().unwrap().0.remove(device_id);
        let mut g = self.inner.lock().unwrap();
        if !g.owners.remove(device_id) || !g.owners.is_empty() {
            return 0;
        }
        let paused = std::mem::take(&mut g.paused);
        let now = self.backend.players();
        let mut resumed = 0;
        for p in paused {
            // Kullanıcı arada kendisi sürdürdüyse ya da oynatıcı kapandıysa dokunma.
            if !now.contains(&p) {
                continue;
            }
            let idle = self.backend.state(&p).is_some_and(|s| !s.is_playing);
            if idle && self.backend.action(&p, MediaAction::Play) {
                resumed += 1;
            }
        }
        resumed
    }

    /// Yeni bir arama olayı damgası verir; eski bekçi köpeklerini geçersiz kılar.
    pub fn touch(&self, device_id: &str) -> u64 {
        let mut g = self.stamps.lock().unwrap();
        g.1 += 1;
        let s = g.1;
        g.0.insert(device_id.to_string(), s);
        s
    }

    /// Bekçi köpeği süresi doldu: damga hâlâ günceliyse aramayı bitmiş sayar.
    pub fn expire(&self, device_id: &str, stamp: u64) -> usize {
        let current = self.stamps.lock().unwrap().0.get(device_id).copied();
        if current == Some(stamp) {
            self.ended(device_id)
        } else {
            0
        }
    }

    // ------------------------------------------------------------------ eşzamansız kabuk

    fn send(&self, job: Job) {
        let mut g = self.jobs.lock().unwrap();
        let tx = g.get_or_insert_with(|| {
            let (tx, rx) = channel::<Job>();
            let me = self.me.clone();
            std::thread::spawn(move || {
                while let Ok(job) = rx.recv() {
                    let Some(m) = me.upgrade() else { break };
                    match job {
                        Job::Ringing(d) => {
                            m.ringing(&d);
                        }
                        Job::Ended(d) => {
                            m.ended(&d);
                        }
                        Job::Expire(d, s) => {
                            if m.expire(&d, s) > 0 {
                                log::info!("arama olayı gelmedi, medya zaman aşımıyla sürdürüldü");
                            }
                        }
                    }
                }
            });
            tx
        });
        let _ = tx.send(job);
    }

    /// Arama çalıyor: arka planda duraklat; bekçi köpeğini kur.
    pub fn ringing_async(&self, device_id: &str) {
        let stamp = self.touch(device_id);
        self.send(Job::Ringing(device_id.to_string()));
        let me = self.me.clone();
        let dev = device_id.to_string();
        std::thread::spawn(move || {
            std::thread::sleep(RING_WATCHDOG);
            if let Some(m) = me.upgrade() {
                m.send(Job::Expire(dev, stamp));
            }
        });
    }

    pub fn ended_async(&self, device_id: &str) {
        self.send(Job::Ended(device_id.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::media::PlayerState;

    struct Fake {
        players: Mutex<Vec<(String, bool)>>,
        calls: Mutex<Vec<String>>,
    }
    impl Fake {
        fn new(p: &[(&str, bool)]) -> Arc<Fake> {
            Arc::new(Fake {
                players: Mutex::new(p.iter().map(|(n, pl)| (n.to_string(), *pl)).collect()),
                calls: Mutex::new(vec![]),
            })
        }
        fn playing(&self, name: &str) -> bool {
            self.players
                .lock()
                .unwrap()
                .iter()
                .any(|p| p.0 == name && p.1)
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }
    impl MediaBackend for Arc<Fake> {
        fn players(&self) -> Vec<String> {
            self.players
                .lock()
                .unwrap()
                .iter()
                .map(|p| p.0.clone())
                .collect()
        }
        fn state(&self, player: &str) -> Option<PlayerState> {
            self.players
                .lock()
                .unwrap()
                .iter()
                .find(|p| p.0 == player)
                .map(|p| PlayerState {
                    is_playing: p.1,
                    ..Default::default()
                })
        }
        fn action(&self, player: &str, a: MediaAction) -> bool {
            self.calls.lock().unwrap().push(format!("{player}:{a:?}"));
            if let Some(p) = self
                .players
                .lock()
                .unwrap()
                .iter_mut()
                .find(|p| p.0 == player)
            {
                match a {
                    MediaAction::Pause => p.1 = false,
                    MediaAction::Play => p.1 = true,
                    _ => {}
                }
            }
            true
        }
        fn set_volume(&self, _: &str, _: i32) -> bool {
            false
        }
        fn set_position(&self, _: &str, _: i64) -> bool {
            false
        }
    }

    fn cm(f: &Arc<Fake>) -> Arc<CallMedia> {
        CallMedia::new(Arc::new(f.clone()))
    }

    #[test]
    fn pauses_playing_and_resumes_only_those() {
        let f = Fake::new(&[("Music", true), ("Spotify", false)]);
        let m = cm(&f);
        assert_eq!(m.ringing("p"), 1);
        assert!(!f.playing("Music"));
        assert_eq!(m.ended("p"), 1);
        assert!(f.playing("Music"));
        assert!(!f.playing("Spotify"), "zaten duran oynatıcı sürdürülmemeli");
        assert_eq!(f.calls(), vec!["Music:Pause", "Music:Play"]);
    }

    #[test]
    fn nothing_playing_does_nothing() {
        let f = Fake::new(&[("Music", false)]);
        let m = cm(&f);
        assert_eq!(m.ringing("p"), 0);
        assert_eq!(m.ended("p"), 0);
        assert!(f.calls().is_empty());
    }

    #[test]
    fn user_resumed_or_closed_player_is_left_alone() {
        let f = Fake::new(&[("Music", true), ("Spotify", true)]);
        let m = cm(&f);
        assert_eq!(m.ringing("p"), 2);
        // Kullanıcı Müzik'i kendisi sürdürdü, Spotify kapandı.
        f.players.lock().unwrap()[0].1 = true;
        f.players.lock().unwrap().remove(1);
        assert_eq!(m.ended("p"), 0);
        assert_eq!(f.calls(), vec!["Music:Pause", "Spotify:Pause"]);
    }

    #[test]
    fn duplicate_and_overlapping_calls() {
        let f = Fake::new(&[("Music", true)]);
        let m = cm(&f);
        assert_eq!(m.ringing("a"), 1);
        assert_eq!(m.ringing("a"), 0);
        assert_eq!(m.ringing("b"), 0);
        assert_eq!(m.ended("a"), 0);
        assert!(!f.playing("Music"));
        assert_eq!(m.ended("b"), 1);
        assert!(f.playing("Music"));
        // İkinci bitiş etkisiz.
        assert_eq!(m.ended("b"), 0);
    }

    #[test]
    fn watchdog_only_when_no_followup() {
        let f = Fake::new(&[("Music", true)]);
        let m = cm(&f);
        m.ringing("p");
        let s1 = m.touch("p");
        let s2 = m.touch("p"); // `talking` geldi
        assert_eq!(m.expire("p", s1), 0);
        assert!(!f.playing("Music"));
        assert_eq!(m.expire("p", s2), 1);
        assert!(f.playing("Music"));
    }

    #[test]
    fn async_shell_runs_in_order() {
        let f = Fake::new(&[("Music", true)]);
        let m = cm(&f);
        m.ringing_async("p");
        m.ended_async("p");
        let start = std::time::Instant::now();
        while f.calls().len() < 2 {
            assert!(start.elapsed() < Duration::from_secs(3), "iş tamamlanmadı");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(f.calls(), vec!["Music:Pause", "Music:Play"]);
        assert!(f.playing("Music"));
    }
}
