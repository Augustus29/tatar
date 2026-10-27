//! Cihaz yöneticisi: bağlantıları, bilinen/eşleşmiş cihazları, eşleştirmeyi, yeniden bağlanmayı
//! ve eklenti dağıtımını tek bir görevde (aktör) yürütür. Dışarıyla `ManagerHandle` üzerinden
//! konuşulur; arayüze `EventSink` ile olay gider.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, mpsc, oneshot};

use crate::plugins::{DeviceHandle, Registry};
use crate::protocol::cert::{KeyStore, LocalIdentity, spki_der};
use crate::protocol::discovery::{self, Discovered, Mdns};
use crate::protocol::identity::{
    Identity, MAX_TCP_PORT, MIN_TCP_PORT, PROTOCOL_VERSION, default_device_name,
    default_device_type, filter_device_name,
};
use crate::protocol::link::{self, Established, HandshakeCtx, HsResult, LinkEvent, LinkIo};
use crate::protocol::packet::{NetworkPacket, TYPE_PAIR};
use crate::protocol::pairing::{
    self, Disposition, FailReason, PAIR_TIMEOUT_SECS, PairAction, PairInput, PairState, classify,
    verification_key,
};
use crate::store::{Settings, SettingsPatch, SettingsStore, TrustedDevice, TrustedStore};

/// Aynı anda en çok bu kadar eşleşmemiş bağlantı (notlar §2.2).
const MAX_UNPAIRED_LINKS: usize = 42;
/// Aynı IP / deviceId için iki bağlantı denemesi arasındaki en kısa süre.
const RATE_GAP: Duration = Duration::from_millis(500);
const MAX_PARALLEL_HANDSHAKES: usize = 16;

pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: &str, payload: Value);
}

pub struct ManagerConfig {
    pub data_dir: PathBuf,
    pub keys: Arc<dyn KeyStore>,
    pub udp_port: u16,
    pub tcp_port_range: (u16, u16),
    pub enable_udp: bool,
    pub enable_mdns: bool,
    pub allow_loopback: bool,
    pub registry: Registry,
}

impl ManagerConfig {
    pub fn production(data_dir: PathBuf, keys: Arc<dyn KeyStore>, registry: Registry) -> Self {
        ManagerConfig {
            data_dir,
            keys,
            udp_port: MIN_TCP_PORT,
            tcp_port_range: (MIN_TCP_PORT, MAX_TCP_PORT),
            enable_udp: true,
            enable_mdns: true,
            allow_loopback: false,
            registry,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Arayüze giden görünümler (camelCase; sözleşme: src/lib/api.ts)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub device_type: String,
    pub reachable: bool,
    pub paired: bool,
    pub battery: Option<Value>,
    pub ip: Option<String>,
    pub last_seen: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PairingView {
    pub device_id: String,
    /// "none" | "requestedByUs" | "requestedByPeer" | "paired"
    pub state: String,
    pub verification_key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityView {
    pub device_id: String,
    pub device_name: String,
    pub device_type: String,
    pub protocol_version: i64,
    pub tcp_port: u16,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn short(id: &str) -> &str {
    &id[..id.len().min(8)]
}

// ---------------------------------------------------------------------------------------------
// Komutlar
// ---------------------------------------------------------------------------------------------

#[allow(clippy::large_enum_variant)]
enum Cmd {
    ListDevices(oneshot::Sender<Vec<DeviceView>>),
    RequestPair(String, oneshot::Sender<Result<PairingView, String>>),
    AcceptPair(String),
    RejectPair(String),
    Unpair(String),
    AddByIp(IpAddr),
    ConnectTo(SocketAddr),
    Send(String, NetworkPacket, oneshot::Sender<Result<(), String>>),
    IdentityChanged,
    /// Eklentiler paylaşılan durumu (ör. pil) değiştirdi; cihaz listesini yeniden yayınla.
    Refresh,
    Discovered(Discovered),
    Handshake {
        key: Option<String>,
        addr: SocketAddr,
        target_known: bool,
        result: HsResult<Established>,
    },
    Shutdown,
}

#[derive(Clone)]
pub struct ManagerHandle {
    tx: mpsc::UnboundedSender<Cmd>,
    settings: Arc<SettingsStore>,
    identity: Arc<RwLock<Identity>>,
    tcp_port: u16,
}

impl ManagerHandle {
    async fn ask<T>(&self, f: impl FnOnce(oneshot::Sender<T>) -> Cmd) -> Result<T, String> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(f(tx)).map_err(|_| "yönetici durdu")?;
        rx.await.map_err(|_| "yönetici durdu".to_string())
    }
    fn cast(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }

    pub async fn list_devices(&self) -> Vec<DeviceView> {
        self.ask(Cmd::ListDevices).await.unwrap_or_default()
    }
    pub async fn request_pair(&self, id: &str) -> Result<PairingView, String> {
        self.ask(|r| Cmd::RequestPair(id.to_string(), r)).await?
    }
    pub fn accept_pair(&self, id: &str) {
        self.cast(Cmd::AcceptPair(id.to_string()));
    }
    pub fn reject_pair(&self, id: &str) {
        self.cast(Cmd::RejectPair(id.to_string()));
    }
    pub fn unpair(&self, id: &str) {
        self.cast(Cmd::Unpair(id.to_string()));
    }
    pub fn add_device_by_ip(&self, ip: &str) -> Result<(), String> {
        let ip: IpAddr = ip
            .trim()
            .parse()
            .map_err(|_| "geçersiz IP adresi".to_string())?;
        self.cast(Cmd::AddByIp(ip));
        Ok(())
    }
    /// Doğrudan bir adrese bağlan (testler ve ileri düzey kullanım).
    pub fn connect_to(&self, addr: SocketAddr) {
        self.cast(Cmd::ConnectTo(addr));
    }
    /// Eşleşmiş ve bağlı bir cihaza paket gönderir (karşı tarafın yeteneği denetlenir).
    pub async fn send_packet(&self, id: &str, packet: NetworkPacket) -> Result<(), String> {
        self.ask(|r| Cmd::Send(id.to_string(), packet, r)).await?
    }
    pub fn get_settings(&self) -> Settings {
        self.settings.get()
    }
    /// Yamayı uygular; ad değiştiyse kimliği duyurur. Birleştirilmiş son hali döner.
    pub fn set_settings(&self, patch: SettingsPatch) -> Settings {
        let name_changed = patch.device_name.is_some();
        let s = self.settings.patch(patch);
        if name_changed {
            let name = effective_name(&s);
            self.identity.write().unwrap().device_name = name;
            self.cast(Cmd::IdentityChanged);
        }
        s
    }
    pub fn settings_store(&self) -> Arc<SettingsStore> {
        self.settings.clone()
    }
    pub fn get_identity(&self) -> IdentityView {
        let i = self.identity.read().unwrap();
        IdentityView {
            device_id: i.device_id.clone(),
            device_name: i.device_name.clone(),
            device_type: i.device_type.clone(),
            protocol_version: i.protocol_version,
            tcp_port: self.tcp_port,
        }
    }
    pub fn tcp_port(&self) -> u16 {
        self.tcp_port
    }
    pub fn shutdown(&self) {
        self.cast(Cmd::Shutdown);
    }
}

fn effective_name(s: &Settings) -> String {
    let n = filter_device_name(&s.device_name);
    if n.is_empty() {
        default_device_name()
    } else {
        n
    }
}

// ---------------------------------------------------------------------------------------------
// Aktör
// ---------------------------------------------------------------------------------------------

struct Live {
    generation: u64,
    io: LinkIo,
    cert: Vec<u8>,
    identity: Identity,
    peer_incoming: Arc<Vec<String>>,
    paired_flag: Arc<AtomicBool>,
    /// Eşleşmemiş cihaza "eşleşmedik" cevabını bu bağlantıda bir kez ver.
    warned_unpaired: bool,
}

struct Dev {
    id: String,
    name: String,
    device_type: String,
    protocol_version: i64,
    ip: Option<IpAddr>,
    port: Option<u16>,
    last_seen: Option<i64>,
    link: Option<Live>,
    pair: PairState,
    deadline: Option<Instant>,
}

struct Core {
    cfg_udp_port: u16,
    enable_udp: bool,
    local: Arc<LocalIdentity>,
    trusted: Arc<TrustedStore>,
    settings: Arc<SettingsStore>,
    identity: Arc<RwLock<Identity>>,
    sink: Arc<dyn EventSink>,
    registry: Registry,
    tx: mpsc::UnboundedSender<Cmd>,
    link_tx: mpsc::UnboundedSender<LinkEvent>,
    tcp_port: u16,
    devs: HashMap<String, Dev>,
    connecting: HashSet<String>,
    next_generation: u64,
    rate: HashMap<String, Instant>,
    mdns: Option<Mdns>,
    ifaces: BTreeSet<std::net::Ipv4Addr>,
}

pub async fn start(cfg: ManagerConfig, sink: Arc<dyn EventSink>) -> Result<ManagerHandle, String> {
    let local = Arc::new(LocalIdentity::load_or_create(
        &cfg.data_dir,
        cfg.keys.as_ref(),
    )?);
    let trusted = Arc::new(TrustedStore::load(Some(
        cfg.data_dir.join("trusted_devices.json"),
    )));
    if local.regenerated {
        // Yeni kimlikle eski eşleşmeler geçersizdir.
        trusted.clear();
    }
    let settings = Arc::new(SettingsStore::load(Some(
        cfg.data_dir.join("settings.json"),
    )));

    // Eklentiler ayarlara ve arayüz yenilemeye bu paylaşılan nesne üzerinden ulaşır.
    let shared = cfg.registry.shared();
    shared.set_settings(settings.clone());

    let identity = Identity {
        device_id: local.device_id.clone(),
        device_name: effective_name(&settings.get()),
        device_type: default_device_type().to_string(),
        protocol_version: PROTOCOL_VERSION,
        incoming: cfg.registry.incoming_capabilities(),
        outgoing: cfg.registry.outgoing_capabilities(),
        tcp_port: None,
        target_device_id: None,
        target_protocol_version: None,
    };
    let identity = Arc::new(RwLock::new(identity));

    let (listener, tcp_port) = bind_listener(cfg.tcp_port_range).await?;
    log::info!("TCP dinleyici {tcp_port} portunda");

    let (tx, rx) = mpsc::unbounded_channel();
    let (link_tx, link_rx) = mpsc::unbounded_channel();

    let handle = ManagerHandle {
        tx: tx.clone(),
        settings: settings.clone(),
        identity: identity.clone(),
        tcp_port,
    };

    let discovered_cb: discovery::Callback = {
        let tx = tx.clone();
        Arc::new(move |d| {
            let _ = tx.send(Cmd::Discovered(d));
        })
    };
    if cfg.enable_udp {
        match discovery::start_udp_listener(
            cfg.udp_port,
            local.device_id.clone(),
            cfg.allow_loopback,
            discovered_cb.clone(),
        ) {
            Ok(_) => log::info!("UDP {} dinleniyor", cfg.udp_port),
            Err(e) => log::warn!("UDP {} dinlenemiyor: {:?}", cfg.udp_port, e.kind()),
        }
    }
    let mdns = if cfg.enable_mdns {
        let ident = identity.read().unwrap().clone();
        match Mdns::start(&ident, tcp_port, cfg.allow_loopback, discovered_cb) {
            Ok(m) => Some(m),
            Err(e) => {
                log::warn!("mDNS başlatılamadı: {e}");
                None
            }
        }
    } else {
        None
    };

    // Gelen TCP bağlantıları.
    {
        let sem = Arc::new(Semaphore::new(MAX_PARALLEL_HANDSHAKES));
        let (local, trusted, identity, tx) =
            (local.clone(), trusted.clone(), identity.clone(), tx.clone());
        tokio::spawn(async move {
            loop {
                let (stream, addr) = match listener.accept().await {
                    Ok(x) => x,
                    Err(e) => {
                        log::warn!("kabul hatası: {:?}", e.kind());
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        continue;
                    }
                };
                let Ok(permit) = sem.clone().try_acquire_owned() else {
                    log::warn!("çok fazla eşzamanlı el sıkışma, bağlantı atıldı");
                    continue;
                };
                let ctx = HandshakeCtx {
                    local: local.clone(),
                    trusted: trusted.clone(),
                    identity: identity.read().unwrap().clone(),
                };
                let tx = tx.clone();
                tokio::spawn(async move {
                    let result = link::accept_in(&ctx, stream, addr).await;
                    drop(permit);
                    let _ = tx.send(Cmd::Handshake {
                        key: None,
                        addr,
                        target_known: false,
                        result,
                    });
                });
            }
        });
    }

    {
        let (shared, tx) = (shared.clone(), tx.clone());
        tokio::spawn(async move {
            loop {
                shared.refresh.notified().await;
                if tx.send(Cmd::Refresh).is_err() {
                    break;
                }
            }
        });
    }

    let mut devs = HashMap::new();
    for t in trusted.list() {
        devs.insert(
            t.device_id.clone(),
            Dev {
                id: t.device_id.clone(),
                name: t.name.clone(),
                device_type: t.device_type.clone(),
                protocol_version: t.protocol_version,
                ip: t.last_ip.as_deref().and_then(|s| s.parse().ok()),
                port: t.last_port,
                last_seen: t.last_seen,
                link: None,
                pair: PairState::Paired,
                deadline: None,
            },
        );
    }

    let core = Core {
        cfg_udp_port: cfg.udp_port,
        enable_udp: cfg.enable_udp,
        local,
        trusted,
        settings,
        identity,
        sink,
        registry: cfg.registry,
        tx,
        link_tx,
        tcp_port,
        devs,
        connecting: HashSet::new(),
        next_generation: 0,
        rate: HashMap::new(),
        mdns,
        ifaces: BTreeSet::new(),
    };
    tokio::spawn(core.run(rx, link_rx));
    Ok(handle)
}

async fn bind_listener(range: (u16, u16)) -> Result<(TcpListener, u16), String> {
    for p in range.0..=range.1 {
        if let Ok(l) = TcpListener::bind(("0.0.0.0", p)).await {
            return Ok((l, p));
        }
    }
    Err(format!(
        "{}-{} aralığında boş TCP portu yok",
        range.0, range.1
    ))
}

impl Core {
    async fn run(
        mut self,
        mut rx: mpsc::UnboundedReceiver<Cmd>,
        mut link_rx: mpsc::UnboundedReceiver<LinkEvent>,
    ) {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        let mut tick: u64 = 0;
        self.ifaces = current_ifaces();
        self.emit_devices();
        self.broadcast();
        self.reconnect_trusted();
        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    if !self.on_cmd(cmd) { break }
                }
                Some(ev) = link_rx.recv() => self.on_link_event(ev),
                _ = ticker.tick() => {
                    tick += 1;
                    self.on_tick(tick);
                }
            }
        }
        for d in self.devs.values() {
            if let Some(l) = &d.link {
                l.io.cancel.cancel();
            }
        }
        if let Some(m) = &self.mdns {
            m.shutdown();
        }
    }

    fn on_cmd(&mut self, cmd: Cmd) -> bool {
        match cmd {
            Cmd::ListDevices(r) => {
                let _ = r.send(self.views());
            }
            Cmd::RequestPair(id, r) => {
                let _ = r.send(self.request_pair(&id));
            }
            Cmd::AcceptPair(id) => {
                self.apply_pair(&id, PairInput::UserAccept);
                self.finish_change(&id);
            }
            Cmd::RejectPair(id) => {
                if matches!(
                    self.devs.get(&id).map(|d| d.pair),
                    Some(PairState::Requested { .. } | PairState::RequestedByPeer { .. })
                ) {
                    self.apply_pair(&id, PairInput::UserReject);
                }
                self.finish_change(&id);
            }
            Cmd::Unpair(id) => {
                if self.devs.contains_key(&id) {
                    self.apply_pair(&id, PairInput::UserUnpair);
                }
                // Tutarsız durumlara karşı: güveni her halükarda sil.
                if self.trusted.remove(&id)
                    && let Some(d) = self.devs.get_mut(&id)
                {
                    d.pair = PairState::NotPaired;
                }
                self.finish_change(&id);
            }
            Cmd::AddByIp(ip) => self.add_by_ip(ip),
            Cmd::ConnectTo(addr) => self.spawn_connect(format!("addr:{addr}"), addr, None),
            Cmd::Send(id, packet, r) => {
                let _ = r.send(self.send_to(&id, packet));
            }
            Cmd::Refresh => self.emit_devices(),
            Cmd::IdentityChanged => {
                let ident = self.identity.read().unwrap().clone();
                if let Some(m) = &self.mdns
                    && let Err(e) = m.announce(&ident, self.tcp_port)
                {
                    log::warn!("mDNS yeniden duyurulamadı: {e}");
                }
                self.broadcast();
            }
            Cmd::Discovered(d) => self.on_discovered(d),
            Cmd::Handshake {
                key,
                addr,
                target_known,
                result,
            } => {
                if let Some(k) = &key {
                    self.connecting.remove(k);
                }
                match result {
                    Ok(est) => self.on_established(est),
                    Err(e) => {
                        log::info!("el sıkışma başarısız ({}): {e}", addr.ip());
                        // Dışarı bağlanamadıysak karşı tarafın bize bağlanmasını iste.
                        if key.is_some() && target_known && e.starts_with("bağlanılamadı") {
                            self.unicast_identity(addr.ip());
                        }
                    }
                }
            }
            Cmd::Shutdown => return false,
        }
        true
    }

    // ---------------------------------------------------------------- görünümler / olaylar

    fn views(&self) -> Vec<DeviceView> {
        let mut v: Vec<DeviceView> = self
            .devs
            .values()
            .map(|d| DeviceView {
                id: d.id.clone(),
                name: d.name.clone(),
                device_type: d.device_type.clone(),
                reachable: d.link.is_some(),
                paired: d.pair == PairState::Paired,
                battery: self.registry.shared().battery_of(&d.id),
                ip: d.ip.map(|i| i.to_string()),
                last_seen: d.last_seen,
            })
            .collect();
        v.sort_by(|a, b| {
            b.paired
                .cmp(&a.paired)
                .then(a.name.cmp(&b.name))
                .then(a.id.cmp(&b.id))
        });
        v
    }

    fn emit_devices(&self) {
        self.sink.emit("devices-changed", json!(self.views()));
    }

    /// Bağlantısı olmayan, güvenilir olmayan cihazı listeden kaldırır; listeyi yayar.
    fn finish_change(&mut self, id: &str) {
        self.gc(id);
        self.emit_devices();
    }

    fn gc(&mut self, id: &str) {
        if let Some(d) = self.devs.get(id)
            && d.link.is_none()
            && d.pair != PairState::Paired
            && !self.trusted.is_trusted(id)
        {
            self.devs.remove(id);
        }
    }

    // ---------------------------------------------------------------- eşleştirme

    fn key_for(&self, id: &str, ts: i64) -> String {
        let Some(link) = self.devs.get(id).and_then(|d| d.link.as_ref()) else {
            return String::new();
        };
        match (spki_der(&self.local.cert_der), spki_der(&link.cert)) {
            (Some(a), Some(b)) => verification_key(&a, &b, ts),
            _ => String::new(),
        }
    }

    fn pairing_view(&self, id: &str) -> PairingView {
        let (state, key) = match self.devs.get(id).map(|d| d.pair) {
            Some(PairState::Requested { timestamp }) => {
                ("requestedByUs", self.key_for(id, timestamp))
            }
            Some(PairState::RequestedByPeer { timestamp }) => {
                ("requestedByPeer", self.key_for(id, timestamp))
            }
            Some(PairState::Paired) => ("paired", String::new()),
            _ => ("none", String::new()),
        };
        PairingView {
            device_id: id.to_string(),
            state: state.into(),
            verification_key: key,
        }
    }

    fn request_pair(&mut self, id: &str) -> Result<PairingView, String> {
        if !self.devs.get(id).is_some_and(|d| d.link.is_some()) {
            return Err("cihaza ulaşılamıyor".into());
        }
        self.apply_pair(id, PairInput::UserRequest);
        self.emit_devices();
        Ok(self.pairing_view(id))
    }

    fn apply_pair(&mut self, id: &str, input: PairInput) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let acts = {
            let Some(dev) = self.devs.get_mut(id) else {
                return;
            };
            let (ns, acts) = pairing::step(dev.pair, input, now);
            dev.pair = ns;
            acts
        };
        for a in acts {
            self.do_action(id, a);
        }
    }

    fn do_action(&mut self, id: &str, act: PairAction) {
        match act {
            PairAction::SendRequest { timestamp } => {
                self.send_pair(id, json!({"pair": true, "timestamp": timestamp}))
            }
            PairAction::SendAccept => self.send_pair(id, json!({"pair": true})),
            PairAction::SendReject => self.send_pair(id, json!({"pair": false})),
            PairAction::NotifyUser { timestamp } => {
                let key = self.key_for(id, timestamp);
                let name = self
                    .devs
                    .get(id)
                    .map(|d| d.name.clone())
                    .unwrap_or_default();
                self.sink.emit(
                    "pair-request",
                    json!({"deviceId": id, "state": "requestedByPeer", "verificationKey": key, "deviceName": name}),
                );
            }
            PairAction::StartTimer => {
                if let Some(d) = self.devs.get_mut(id) {
                    d.deadline = Some(Instant::now() + Duration::from_secs(PAIR_TIMEOUT_SECS));
                }
            }
            PairAction::CancelTimer => {
                if let Some(d) = self.devs.get_mut(id) {
                    d.deadline = None;
                }
            }
            PairAction::MarkPaired => {
                let Some(d) = self.devs.get_mut(id) else {
                    return;
                };
                let Some(link) = d.link.as_ref() else { return };
                self.trusted.upsert(TrustedDevice {
                    device_id: id.to_string(),
                    name: d.name.clone(),
                    device_type: d.device_type.clone(),
                    cert: link.cert.clone(),
                    protocol_version: link.identity.protocol_version,
                    last_ip: d.ip.map(|i| i.to_string()),
                    last_port: d.port,
                    last_seen: Some(now_ms()),
                });
                link.paired_flag.store(true, Ordering::Relaxed);
                log::info!("cihaz eşleşti: {}", short(id));
                self.sink.emit(
                    "pair-result",
                    json!({"deviceId": id, "success": true, "accepted": true, "error": null}),
                );
                if let Some(h) = self.handle_for(id) {
                    self.registry.connected(&h);
                }
                self.emit_devices();
            }
            PairAction::Unpaired => {
                self.trusted.remove(id);
                if let Some(l) = self.devs.get(id).and_then(|d| d.link.as_ref()) {
                    l.paired_flag.store(false, Ordering::Relaxed);
                }
                self.registry.disconnected(id);
                log::info!("eşleşme kaldırıldı: {}", short(id));
                self.emit_devices();
            }
            PairAction::Failed(reason) => {
                let error = match reason {
                    FailReason::Rejected => "rejected",
                    FailReason::Timeout => "timeout",
                    FailReason::Cancelled => "cancelled",
                    FailReason::ClockMismatch => "failed",
                };
                self.sink.emit(
                    "pair-result",
                    json!({"deviceId": id, "success": false, "accepted": false, "error": error}),
                );
            }
        }
    }

    fn send_pair(&self, id: &str, body: Value) {
        if let Some(l) = self.devs.get(id).and_then(|d| d.link.as_ref()) {
            let _ = l.io.tx.try_send(NetworkPacket::new(TYPE_PAIR, body));
        }
    }

    // ---------------------------------------------------------------- bağlantı olayları

    fn handle_for(&self, id: &str) -> Option<DeviceHandle> {
        let d = self.devs.get(id)?;
        let l = d.link.as_ref()?;
        Some(DeviceHandle {
            id: id.to_string(),
            name: d.name.clone(),
            peer_incoming: l.peer_incoming.clone(),
            tx: l.io.tx.clone(),
        })
    }

    fn unpaired_link_count(&self) -> usize {
        self.devs
            .values()
            .filter(|d| d.link.is_some() && d.pair != PairState::Paired)
            .count()
    }

    fn on_established(&mut self, est: Established) {
        let id = est.identity.device_id.clone();
        match self.devs.get(&id).and_then(|d| d.link.as_ref()) {
            // Aynı cihaz, farklı sertifika: YENİ bağlantıyı kapat.
            Some(old) if old.cert != est.cert_der => {
                log::warn!(
                    "mevcut bağlantıdan farklı sertifika, yeni bağlantı kapatıldı: {}",
                    short(&id)
                );
                return;
            }
            Some(_) => {}
            None => {
                if !self.trusted.is_trusted(&id) && self.unpaired_link_count() >= MAX_UNPAIRED_LINKS
                {
                    log::warn!("eşleşmemiş bağlantı sınırı doldu, bağlantı atıldı");
                    return;
                }
            }
        }
        self.next_generation += 1;
        let generation = self.next_generation;
        let trusted_dev = self.trusted.get(&id);
        let peer = est.identity.clone();
        let cert = est.cert_der.clone();
        let ip = est.addr.ip();

        let dev = self.devs.entry(id.clone()).or_insert_with(|| Dev {
            id: id.clone(),
            name: peer.device_name.clone(),
            device_type: peer.device_type.clone(),
            protocol_version: peer.protocol_version,
            ip: None,
            port: None,
            last_seen: None,
            link: None,
            pair: if trusted_dev.is_some() {
                PairState::Paired
            } else {
                PairState::NotPaired
            },
            deadline: None,
        });
        if let Some(old) = dev.link.take() {
            // Aynı sertifikayla ikinci bağlantı: eskisini değiştir.
            old.io.cancel.cancel();
        }
        dev.name = peer.device_name.clone();
        dev.device_type = peer.device_type.clone();
        dev.protocol_version = peer.protocol_version;
        dev.ip = Some(ip);
        dev.last_seen = Some(now_ms());
        let paired_flag = Arc::new(AtomicBool::new(dev.pair == PairState::Paired));
        let io = link::spawn_link(
            est,
            id.clone(),
            generation,
            paired_flag.clone(),
            self.link_tx.clone(),
        );
        dev.link = Some(Live {
            generation,
            io,
            cert,
            peer_incoming: Arc::new(peer.incoming.clone()),
            identity: peer,
            paired_flag,
            warned_unpaired: false,
        });
        let paired = dev.pair == PairState::Paired;
        if paired {
            self.trusted.touch(
                &id,
                Some(ip.to_string()),
                dev.port,
                Some(&dev.name.clone()),
                now_ms(),
            );
        }
        log::info!(
            "bağlantı kuruldu: {} ({})",
            short(&id),
            if paired {
                "eşleşmiş"
            } else {
                "eşleşmemiş"
            }
        );
        if paired && let Some(h) = self.handle_for(&id) {
            self.registry.connected(&h);
        }
        self.emit_devices();
    }

    fn on_link_event(&mut self, ev: LinkEvent) {
        match ev {
            LinkEvent::Closed {
                device_id,
                generation,
            } => {
                let Some(dev) = self.devs.get_mut(&device_id) else {
                    return;
                };
                if dev.link.as_ref().map(|l| l.generation) != Some(generation) {
                    return; // eski (değiştirilmiş) bağlantı
                }
                dev.link = None;
                dev.last_seen = Some(now_ms());
                log::info!("bağlantı koptu: {}", short(&device_id));
                self.apply_pair(&device_id, PairInput::LinkLost);
                self.registry.disconnected(&device_id);
                self.finish_change(&device_id);
            }
            LinkEvent::Packet {
                device_id,
                generation,
                packet,
            } => self.on_packet(&device_id, generation, packet),
        }
    }

    fn on_packet(&mut self, id: &str, generation: u64, packet: NetworkPacket) {
        let paired = match self.devs.get(id) {
            Some(d) if d.link.as_ref().map(|l| l.generation) == Some(generation) => {
                d.pair == PairState::Paired
            }
            _ => return,
        };
        match classify(paired, &packet.packet_type) {
            Disposition::Pairing => {
                let input = match packet.body_bool("pair") {
                    Some(true) => PairInput::PeerPair {
                        timestamp: packet.body_i64("timestamp"),
                    },
                    Some(false) => PairInput::PeerUnpair,
                    None => {
                        log::warn!("pair alanı olmayan eşleştirme paketi atıldı");
                        return;
                    }
                };
                self.apply_pair(id, input);
                self.finish_change(id);
            }
            Disposition::Plugin => {
                if let Some(h) = self.handle_for(id)
                    && !self.registry.dispatch(&h, &packet)
                {
                    // Yalnızca tür loglanır.
                    log::debug!("bilinmeyen paket türü yok sayıldı: {}", packet.packet_type);
                }
            }
            Disposition::DropUnpaired => {
                log::warn!(
                    "eşleşmemiş cihazdan paket atıldı: {} ({})",
                    packet.packet_type,
                    short(id)
                );
                // Karşı taraf bizi eşleşmiş sanıyor: durumunu düzeltmesi için bir kez "eşleşmedik" de.
                let first = self
                    .devs
                    .get_mut(id)
                    .and_then(|d| d.link.as_mut())
                    .map(|l| !std::mem::replace(&mut l.warned_unpaired, true))
                    .unwrap_or(false);
                if first {
                    self.send_pair(id, json!({"pair": false}));
                }
            }
            Disposition::DropUnexpected => {
                log::warn!("beklenmeyen kimlik paketi atıldı ({})", short(id));
            }
        }
    }

    fn send_to(&mut self, id: &str, packet: NetworkPacket) -> Result<(), String> {
        let d = self.devs.get(id).ok_or("bilinmeyen cihaz")?;
        if d.pair != PairState::Paired {
            return Err("cihaz eşleşmemiş".into());
        }
        let l = d.link.as_ref().ok_or("cihaza ulaşılamıyor")?;
        if !l.peer_incoming.contains(&packet.packet_type) {
            return Err("karşı taraf bu paket türünü desteklemiyor".into());
        }
        l.io.tx
            .try_send(packet)
            .map_err(|_| "gönderim kuyruğu dolu".to_string())
    }

    // ---------------------------------------------------------------- keşif / yeniden bağlanma

    fn allow(&mut self, key: &str) -> bool {
        let now = Instant::now();
        if self.rate.len() > 1024 {
            self.rate.retain(|_, t| now.duration_since(*t) < RATE_GAP);
        }
        match self.rate.get(key) {
            Some(t) if now.duration_since(*t) < RATE_GAP => false,
            _ => {
                self.rate.insert(key.to_string(), now);
                true
            }
        }
    }

    fn spawn_connect(&mut self, key: String, addr: SocketAddr, target: Option<(String, i64)>) {
        if !self.connecting.insert(key.clone()) {
            return;
        }
        let ctx = HandshakeCtx {
            local: self.local.clone(),
            trusted: self.trusted.clone(),
            identity: self.identity.read().unwrap().clone(),
        };
        let tx = self.tx.clone();
        let target_known = target.is_some();
        tokio::spawn(async move {
            let result = link::connect_out(&ctx, addr, target).await;
            let _ = tx.send(Cmd::Handshake {
                key: Some(key),
                addr,
                target_known,
                result,
            });
        });
    }

    fn on_discovered(&mut self, d: Discovered) {
        if d.device_id == self.local.device_id || d.protocol_version < PROTOCOL_VERSION {
            return;
        }
        let trusted = self.trusted.is_trusted(&d.device_id);
        if let Some(dev) = self.devs.get_mut(&d.device_id) {
            dev.ip = Some(d.ip);
            dev.port = Some(d.tcp_port);
        }
        if trusted {
            self.trusted.touch(
                &d.device_id,
                Some(d.ip.to_string()),
                Some(d.tcp_port),
                None,
                now_ms(),
            );
            if !self.settings.get().auto_connect {
                return;
            }
        }
        if self
            .devs
            .get(&d.device_id)
            .is_some_and(|x| x.link.is_some())
            || self.connecting.contains(&d.device_id)
        {
            return;
        }
        if !self.allow(&format!("ip:{}", d.ip)) || !self.allow(&format!("id:{}", d.device_id)) {
            return;
        }
        self.spawn_connect(
            d.device_id.clone(),
            SocketAddr::new(d.ip, d.tcp_port),
            Some((d.device_id, d.protocol_version)),
        );
    }

    fn udp_datagram(&self) -> Vec<u8> {
        self.identity
            .read()
            .unwrap()
            .to_udp_packet(self.tcp_port)
            .serialize()
    }

    fn broadcast(&self) {
        if !self.enable_udp {
            return;
        }
        let data = self.udp_datagram();
        let port = self.cfg_udp_port;
        tokio::task::spawn_blocking(move || discovery::broadcast_blocking(&data, port));
    }

    fn unicast_identity(&self, ip: IpAddr) {
        if !self.enable_udp {
            return;
        }
        let data = self.udp_datagram();
        let target = SocketAddr::new(ip, self.cfg_udp_port);
        tokio::task::spawn_blocking(move || discovery::unicast_blocking(&data, target));
    }

    fn add_by_ip(&mut self, ip: IpAddr) {
        let ip = discovery::normalize_ip(ip);
        // Telefon bize bağlansın (UDP) ve biz de doğrudan bağlanmayı deneyelim.
        self.unicast_identity(ip);
        if self.allow(&format!("ip:{ip}")) {
            self.spawn_connect(format!("ip:{ip}"), SocketAddr::new(ip, MIN_TCP_PORT), None);
        }
    }

    /// Eşleşmiş ama bağlı olmayan cihazlara son bilinen adresten ulaşmayı dener.
    fn reconnect_trusted(&mut self) {
        if !self.settings.get().auto_connect {
            return;
        }
        let targets: Vec<(String, IpAddr, u16, i64)> = self
            .devs
            .values()
            .filter(|d| d.link.is_none() && d.pair == PairState::Paired)
            .filter_map(|d| {
                Some((
                    d.id.clone(),
                    d.ip?,
                    d.port.unwrap_or(MIN_TCP_PORT),
                    d.protocol_version,
                ))
            })
            .collect();
        for (id, ip, port, proto) in targets {
            if self.connecting.contains(&id)
                || !self.allow(&format!("ip:{ip}"))
                || !self.allow(&format!("id:{id}"))
            {
                continue;
            }
            self.unicast_identity(ip);
            self.spawn_connect(id.clone(), SocketAddr::new(ip, port), Some((id, proto)));
        }
    }

    fn has_unreachable_trusted(&self) -> bool {
        self.devs
            .values()
            .any(|d| d.link.is_none() && d.pair == PairState::Paired)
    }

    fn on_tick(&mut self, tick: u64) {
        // Eşleştirme zaman aşımları.
        let now = Instant::now();
        let due: Vec<String> = self
            .devs
            .values()
            .filter(|d| d.deadline.is_some_and(|t| t <= now))
            .map(|d| d.id.clone())
            .collect();
        for id in due {
            if let Some(d) = self.devs.get_mut(&id) {
                d.deadline = None;
            }
            self.apply_pair(&id, PairInput::Timeout);
            self.finish_change(&id);
        }
        // Ağ değişikliği: arayüz kümesi değiştiyse yeniden duyur ve bağlan.
        if tick.is_multiple_of(5) {
            let now_ifaces = current_ifaces();
            if now_ifaces != self.ifaces {
                self.ifaces = now_ifaces;
                log::info!("ağ arayüzleri değişti, yeniden duyuruluyor");
                self.broadcast();
                self.reconnect_trusted();
            }
        }
        // Bağlı olmayan eşleşmiş cihaz varsa 30 sn'de bir, yoksa 5 dk'da bir yayın.
        if tick.is_multiple_of(30) {
            if self.has_unreachable_trusted() {
                self.broadcast();
                self.reconnect_trusted();
            } else if tick.is_multiple_of(300) {
                self.broadcast();
            }
        }
    }
}

fn current_ifaces() -> BTreeSet<std::net::Ipv4Addr> {
    discovery::local_ipv4_interfaces()
        .into_iter()
        .map(|(ip, _)| ip)
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Entegrasyon testi: iki Tatar örneği loopback üzerinde
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::Plugin;
    use crate::protocol::cert::MemoryKeyStore;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    #[derive(Default)]
    struct TestSink(Mutex<Vec<(String, Value)>>);
    impl EventSink for TestSink {
        fn emit(&self, event: &str, payload: Value) {
            self.0.lock().unwrap().push((event.to_string(), payload));
        }
    }
    impl TestSink {
        fn find(&self, ev: &str, pred: impl Fn(&Value) -> bool) -> Option<Value> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|(e, p)| e == ev && pred(p))
                .map(|(_, p)| p.clone())
        }
    }

    struct PingPlugin(AtomicUsize);
    impl Plugin for PingPlugin {
        fn name(&self) -> &'static str {
            "ping"
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

    async fn until(mut f: impl FnMut() -> bool) {
        for _ in 0..100 {
            if f() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("zaman aşımı");
    }

    async fn instance(
        range: (u16, u16),
        plugin: Arc<PingPlugin>,
    ) -> (ManagerHandle, Arc<TestSink>, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "tatar-mgr-{}",
            crate::protocol::identity::generate_device_id()
        ));
        let mut registry = Registry::new();
        registry.register(plugin);
        let sink = Arc::new(TestSink::default());
        let cfg = ManagerConfig {
            data_dir: dir.clone(),
            keys: Arc::new(MemoryKeyStore::default()),
            udp_port: 0,
            tcp_port_range: range,
            enable_udp: false,
            enable_mdns: false,
            allow_loopback: true,
            registry,
        };
        let h = start(cfg, sink.clone()).await.unwrap();
        (h, sink, dir)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_instances_connect_and_pair() {
        let pa = Arc::new(PingPlugin(AtomicUsize::new(0)));
        let pb = Arc::new(PingPlugin(AtomicUsize::new(0)));
        let (a, sink_a, dir_a) = instance((47100, 47110), pa.clone()).await;
        let (b, sink_b, dir_b) = instance((47120, 47130), pb.clone()).await;
        let (id_a, id_b) = (a.get_identity().device_id, b.get_identity().device_id);

        // A, B'ye bağlanır (A TLS sunucusu, B TLS istemcisi).
        a.connect_to(SocketAddr::from(([127, 0, 0, 1], b.tcp_port())));
        let (ha, hb) = (a.clone(), b.clone());
        let (ia, ib) = (id_a.clone(), id_b.clone());
        for _ in 0..100 {
            let la = ha.list_devices().await;
            let lb = hb.list_devices().await;
            if la.iter().any(|d| d.id == ib && d.reachable)
                && lb.iter().any(|d| d.id == ia && d.reachable)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            a.list_devices()
                .await
                .iter()
                .any(|d| d.id == id_b && d.reachable && !d.paired)
        );
        assert!(
            b.list_devices()
                .await
                .iter()
                .any(|d| d.id == id_a && d.reachable && !d.paired)
        );

        // Eşleşmemişken eklenti paketi gönderilemez.
        assert!(
            a.send_packet(&id_b, NetworkPacket::new("kdeconnect.ping", json!({})))
                .await
                .is_err()
        );

        // A eşleştirme ister; B'de aynı kod görünür.
        let view = a.request_pair(&id_b).await.unwrap();
        assert_eq!(view.state, "requestedByUs");
        assert_eq!(view.verification_key.len(), 8);
        until(|| {
            sink_b
                .find("pair-request", |p| p["deviceId"] == id_a.as_str())
                .is_some()
        })
        .await;
        let req = sink_b.find("pair-request", |_| true).unwrap();
        assert_eq!(req["verificationKey"], view.verification_key.as_str());
        assert_eq!(req["state"], "requestedByPeer");

        b.accept_pair(&id_a);
        until(|| {
            sink_a
                .find("pair-result", |p| p["success"] == true)
                .is_some()
        })
        .await;
        until(|| {
            sink_b
                .find("pair-result", |p| p["success"] == true)
                .is_some()
        })
        .await;
        assert!(
            a.list_devices()
                .await
                .iter()
                .any(|d| d.id == id_b && d.paired)
        );
        assert!(
            b.list_devices()
                .await
                .iter()
                .any(|d| d.id == id_a && d.paired)
        );

        // Eşleşmiş: ping eklentiye ulaşır (yetenekler iki tarafta ilan edildi).
        a.send_packet(&id_b, NetworkPacket::new("kdeconnect.ping", json!({})))
            .await
            .unwrap();
        until(|| pb.0.load(Ordering::SeqCst) == 1).await;
        assert_eq!(pa.0.load(Ordering::SeqCst), 0);

        // Güven kalıcı: sertifika diske yazıldı.
        let store = TrustedStore::load(Some(dir_a.join("trusted_devices.json")));
        assert!(store.is_trusted(&id_b));

        // Eşleşmeyi kaldır: iki taraf da güveni siler.
        a.unpair(&id_b);
        until_async_unpaired(&b, &id_a).await;
        assert!(
            !a.list_devices()
                .await
                .iter()
                .any(|d| d.id == id_b && d.paired)
        );
        a.shutdown();
        b.shutdown();
        let _ = std::fs::remove_dir_all(dir_a);
        let _ = std::fs::remove_dir_all(dir_b);
    }

    async fn until_async_unpaired(h: &ManagerHandle, id: &str) {
        for _ in 0..100 {
            if !h
                .list_devices()
                .await
                .iter()
                .any(|d| d.id == id && d.paired)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("eşleşme kaldırılamadı");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn rejected_pair_request() {
        let (a, sink_a, da) =
            instance((47200, 47210), Arc::new(PingPlugin(AtomicUsize::new(0)))).await;
        let (b, sink_b, db) =
            instance((47220, 47230), Arc::new(PingPlugin(AtomicUsize::new(0)))).await;
        let id_b = b.get_identity().device_id;
        let id_a = a.get_identity().device_id;
        a.connect_to(SocketAddr::from(([127, 0, 0, 1], b.tcp_port())));
        for _ in 0..100 {
            if a.request_pair(&id_b).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        until(|| sink_b.find("pair-request", |_| true).is_some()).await;
        b.reject_pair(&id_a);
        until(|| {
            sink_a
                .find("pair-result", |p| {
                    p["success"] == false && p["error"] == "rejected"
                })
                .is_some()
        })
        .await;
        assert!(!a.list_devices().await.iter().any(|d| d.paired));
        a.shutdown();
        b.shutdown();
        let _ = std::fs::remove_dir_all(da);
        let _ = std::fs::remove_dir_all(db);
    }
}
