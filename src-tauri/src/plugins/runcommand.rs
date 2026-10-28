//! `kdeconnect.runcommand` (giden: komut listesi) ve `kdeconnect.runcommand.request` (gelen).
//! Komutlar yalnızca bu bilgisayarın ayarlarında tanımlanır; telefon yalnızca anahtar (kimlik) yollar,
//! komut metnini ASLA kendisi vermez. Komut metni ve çıktısı loglanmaz.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};

use super::notifier::SharedNotifier;
use super::{DeviceHandle, Plugin, Shared};
use crate::i18n::{notify_text, resolve_language};
use crate::protocol::packet::NetworkPacket;
use crate::store::RunCommand;

pub const TYPE_RUNCOMMAND: &str = "kdeconnect.runcommand";
pub const TYPE_REQUEST: &str = "kdeconnect.runcommand.request";

/// Komutu gerçekten çalıştıran katman (testlerde sahte).
pub trait CommandRunner: Send + Sync {
    fn run(&self, command: &str);
}

/// Komutu kabukta (macOS/Linux: `sh -c`, Windows: `cmd /C`) ayrık ve beklemeden çalıştırır.
pub struct ShellRunner;

impl CommandRunner for ShellRunner {
    fn run(&self, command: &str) {
        use std::process::{Command, Stdio};
        #[cfg(windows)]
        let mut cmd = {
            use std::os::windows::process::CommandExt;
            let mut c = Command::new("cmd");
            // Tırnaklar olduğu gibi geçmeli; normal `arg` onları kaçışlar ve cmd'yi bozar.
            c.arg("/C").raw_arg(command);
            // Siyah konsol penceresi açılmasın.
            c.creation_flags(0x0800_0000);
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            use std::os::unix::process::CommandExt;
            let mut c = Command::new("/bin/sh");
            c.arg("-c").arg(command);
            // Kendi süreç grubunda çalışsın (bizim sinyallerimizden ayrı).
            c.process_group(0);
            c
        };
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match cmd.spawn() {
            // Çıkışı bekleyen küçük bir iş parçacığı: zombi süreç kalmaz, ana akış engellenmez.
            Ok(mut child) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(e) => log::warn!("komut başlatılamadı: {}", e.kind()),
        }
    }
}

/// `commandList` alanı, JSON'un kendisi değil JSON **metni**dir: kimlik -> {name, command}.
pub fn command_list_packet(commands: &[RunCommand]) -> NetworkPacket {
    let mut map = Map::new();
    for c in commands {
        map.insert(c.id.clone(), json!({"name": c.name, "command": c.command}));
    }
    let list = Value::Object(map).to_string();
    NetworkPacket::new(
        TYPE_RUNCOMMAND,
        json!({"commandList": list, "canAddCommand": true}),
    )
}

/// Telefonun yolladığı anahtarın bizim listemizdeki karşılığı.
pub fn lookup<'a>(commands: &'a [RunCommand], key: &str) -> Option<&'a RunCommand> {
    commands.iter().find(|c| c.id == key)
}

pub struct RunCommandPlugin {
    shared: Arc<Shared>,
    notifier: SharedNotifier,
    runner: Arc<dyn CommandRunner>,
    /// Telefon "komut ekle" isteyince (ana pencereyi öne getirmek için).
    on_setup: Arc<dyn Fn() + Send + Sync>,
    devices: Mutex<HashMap<String, DeviceHandle>>,
}

impl RunCommandPlugin {
    pub fn new(
        shared: Arc<Shared>,
        notifier: SharedNotifier,
        runner: Arc<dyn CommandRunner>,
        on_setup: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        RunCommandPlugin {
            shared,
            notifier,
            runner,
            on_setup,
            devices: Mutex::new(HashMap::new()),
        }
    }

    fn send_list(&self, device: &DeviceHandle) {
        let list = self.shared.settings().commands;
        device.send(command_list_packet(&list));
    }

    /// Ayarlardaki liste değişince bağlı bütün cihazlara yeni listeyi yollar.
    pub fn send_to_all(&self) {
        let devices: Vec<DeviceHandle> = self.devices.lock().unwrap().values().cloned().collect();
        for d in &devices {
            self.send_list(d);
        }
    }

    fn run_key(&self, device: &DeviceHandle, key: &str) {
        let settings = self.shared.settings();
        let Some(cmd) = lookup(&settings.commands, key) else {
            log::warn!(
                "bilinmeyen komut anahtarı yok sayıldı: {}",
                crate::diag::clean(key)
            );
            return;
        };
        log::info!("telefondan komut çalıştırılıyor");
        self.runner.run(&cmd.command);
        let lang = resolve_language(&settings.language);
        self.notifier.notify(
            &notify_text(lang, "commandRanTitle", &[]),
            &notify_text(
                lang,
                "commandRanBody",
                &[("name", &cmd.name), ("device", &device.name)],
            ),
        );
    }
}

impl Plugin for RunCommandPlugin {
    fn name(&self) -> &'static str {
        "runcommand"
    }
    fn incoming_types(&self) -> &'static [&'static str] {
        &[TYPE_REQUEST]
    }
    fn outgoing_types(&self) -> &'static [&'static str] {
        &[TYPE_RUNCOMMAND]
    }
    fn handle(&self, device: &DeviceHandle, packet: &NetworkPacket) {
        if packet.packet_type != TYPE_REQUEST {
            return;
        }
        if packet.body_bool("requestCommandList") == Some(true) {
            self.send_list(device);
        } else if let Some(key) = packet.body_str("key") {
            self.run_key(device, key);
        } else if packet.body.get("setup").is_some() {
            (self.on_setup)();
        } else if packet.body.get("stop").is_none() {
            // `stop`: çıktı/durdurma desteği yok, sessizce yok sayılır.
            log::debug!("runcommand isteği tanınmadı, yok sayıldı");
        }
    }
    fn device_connected(&self, device: &DeviceHandle) {
        self.devices
            .lock()
            .unwrap()
            .insert(device.id.clone(), device.clone());
        self.send_list(device);
    }
    fn device_disconnected(&self, device_id: &str) {
        self.devices.lock().unwrap().remove(device_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::notifier::Notifier;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::mpsc;

    #[derive(Default)]
    struct Fake {
        ran: Mutex<Vec<String>>,
        notes: AtomicUsize,
    }
    impl CommandRunner for Fake {
        fn run(&self, c: &str) {
            self.ran.lock().unwrap().push(c.to_string());
        }
    }
    impl Notifier for Fake {
        fn notify(&self, _t: &str, _b: &str) {
            self.notes.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn cmds() -> Vec<RunCommand> {
        vec![
            RunCommand {
                id: "a1".into(),
                name: "Uyku".into(),
                command: "pmset sleepnow".into(),
            },
            RunCommand {
                id: "b2".into(),
                name: "Sessiz".into(),
                command: "osascript -e 'set volume output muted true'".into(),
            },
        ]
    }

    #[test]
    fn list_is_json_string_of_object() {
        let p = command_list_packet(&cmds());
        assert_eq!(p.packet_type, "kdeconnect.runcommand");
        let s = p.body_str("commandList").expect("metin olmalı");
        let v: Value = serde_json::from_str(s).unwrap();
        assert_eq!(v["a1"]["name"], "Uyku");
        assert_eq!(v["a1"]["command"], "pmset sleepnow");
        assert_eq!(v.as_object().unwrap().len(), 2);
        assert_eq!(p.body_bool("canAddCommand"), Some(true));
        let empty = command_list_packet(&[]);
        assert_eq!(empty.body_str("commandList"), Some("{}"));
    }

    #[test]
    fn key_lookup_only_in_local_list() {
        let c = cmds();
        assert_eq!(lookup(&c, "b2").unwrap().name, "Sessiz");
        assert!(lookup(&c, "yok").is_none());
        assert!(lookup(&c, "").is_none());
    }

    fn setup() -> (
        RunCommandPlugin,
        Arc<Fake>,
        DeviceHandle,
        mpsc::Receiver<NetworkPacket>,
        Arc<AtomicUsize>,
    ) {
        let fake = Arc::new(Fake::default());
        let shared = Arc::new(Shared::default());
        let store = Arc::new(crate::store::SettingsStore::load(None));
        store.patch(crate::store::SettingsPatch {
            commands: Some(cmds()),
            ..Default::default()
        });
        shared.set_settings(store);
        let setups = Arc::new(AtomicUsize::new(0));
        let s2 = setups.clone();
        let plugin = RunCommandPlugin::new(
            shared,
            fake.clone(),
            fake.clone(),
            Arc::new(move || {
                s2.fetch_add(1, Ordering::SeqCst);
            }),
        );
        let (tx, rx) = mpsc::channel(8);
        let dev = DeviceHandle {
            id: "d".into(),
            name: "Telefon".into(),
            peer_incoming: Arc::new(vec![TYPE_RUNCOMMAND.into()]),
            tx,
            peer_ip: None,
            peer_cert: Arc::new(Vec::new()),
        };
        (plugin, fake, dev, rx, setups)
    }

    fn req(body: Value) -> NetworkPacket {
        NetworkPacket::new(TYPE_REQUEST, body)
    }

    #[test]
    fn known_key_runs_unknown_ignored() {
        let (p, fake, dev, _rx, _) = setup();
        p.handle(&dev, &req(json!({"key": "a1"})));
        assert_eq!(*fake.ran.lock().unwrap(), vec!["pmset sleepnow"]);
        assert_eq!(fake.notes.load(Ordering::SeqCst), 1);
        // Telefon komut metni yollasa bile yok sayılır; anahtar listede yoksa çalışmaz.
        p.handle(
            &dev,
            &req(json!({"key": "rm -rf /", "command": "rm -rf /"})),
        );
        p.handle(&dev, &req(json!({"command": "rm -rf /"})));
        assert_eq!(fake.ran.lock().unwrap().len(), 1);
        assert_eq!(fake.notes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn malformed_packets_are_ignored() {
        let (p, fake, dev, mut rx, setups) = setup();
        for body in [
            json!({}),
            json!({"key": 5}),
            json!({"key": null}),
            json!({"key": ["a1"]}),
            json!({"requestCommandList": "belki"}),
            json!({"stop": true}),
        ] {
            p.handle(&dev, &req(body));
        }
        assert!(fake.ran.lock().unwrap().is_empty());
        assert!(rx.try_recv().is_err());
        assert_eq!(setups.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn request_list_and_setup_and_connect() {
        let (p, _fake, dev, mut rx, setups) = setup();
        p.handle(&dev, &req(json!({"requestCommandList": true})));
        assert_eq!(rx.try_recv().unwrap().packet_type, TYPE_RUNCOMMAND);
        p.handle(&dev, &req(json!({"setup": true})));
        assert_eq!(setups.load(Ordering::SeqCst), 1);
        p.device_connected(&dev);
        assert!(rx.try_recv().is_ok());
        p.send_to_all();
        assert!(rx.try_recv().is_ok());
        p.device_disconnected("d");
        p.send_to_all();
        assert!(rx.try_recv().is_err());
    }
}
