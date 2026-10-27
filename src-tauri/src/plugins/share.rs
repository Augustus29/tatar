//! `share` eklentisi ve aktarım motoru: dosya gönderme/alma (yük soketleri), metin ve bağlantı
//! paylaşımı. Protokol ayrıntıları: docs/PROTOCOL_NOTES.md §6.5 ve §7.
//!
//! Özet
//! - Gönderme: dosya başına bir `share.request` başlığı + yük için 1739–1764 arasında bir dinleyici.
//!   Yük soketinde roller kontrol kanalının TERSİ değil, normal yöndedir: bağlanan (alıcı) TLS
//!   istemcisi, kabul eden (gönderici) TLS sunucusudur. Eşin sertifikası saklananla birebir aynı olmalı.
//! - Alma: başlıktaki porta, kontrol bağlantısının KARŞI IP'sine bağlanılır. Veri `.part` dosyasına
//!   akıtılır, bitince indirme klasörü içinde asla üzerine yazmadan son adına taşınır.
//! - Sabit bellek: veri 256 KiB'lık parçalarla akar; dosya bellekte tutulmaz.
//! - Loglara dosya adı/içerik/pano yazılmaz (CLAUDE.md §4 kural 7).

use std::collections::HashMap;
use std::io;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_rustls::{TlsAcceptor, TlsConnector};
use tokio_util::sync::CancellationToken;

use super::{DeviceHandle, Plugin};
use crate::manager::EventSink;
use crate::protocol::cert::LocalIdentity;
use crate::protocol::link::configure_socket;
use crate::protocol::packet::{NetworkPacket, lenient_i64};
use crate::protocol::tls;
use crate::store::SettingsStore;
use crate::transfer::Transfer;
use crate::transfer::names::{ensure_inside, finalize_no_clobber, sanitize_filename};

pub const TYPE_REQUEST: &str = "kdeconnect.share.request";
pub const TYPE_UPDATE: &str = "kdeconnect.share.request.update";

/// Yük soketine bağlanma süresi (alıcı).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Alıcının bağlanmasını bekleme süresi (gönderici). KDE 30 sn, Android 10 sn.
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(30);
const TLS_TIMEOUT: Duration = Duration::from_secs(10);
/// Karşı taraftan bu kadar süre hiç veri gelmez/gitmezse aktarım kopmuş sayılır.
const STALL_TIMEOUT: Duration = Duration::from_secs(120);
const CHUNK: usize = 256 * 1024;
/// `transfer-progress` olaylarının en sık yayın aralığı (~10/sn).
const PROGRESS_EVERY: Duration = Duration::from_millis(100);
/// Listede tutulan en çok aktarım; aşılırsa en eski bitmişler silinir.
const MAX_KEEP: usize = 200;
/// Panoya konacak metnin üst sınırı.
const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_URL_BYTES: usize = 8 * 1024;
/// Boyutu bilinmeyen (`payloadSize = -1`) alımlar için üst sınır.
const MAX_UNKNOWN_SIZE: u64 = 64 * 1024 * 1024 * 1024;
/// Duyurulan tek dosya için makul üst sınır (1 TiB).
const MAX_FILE_SIZE: u64 = 1024 * 1024 * 1024 * 1024;
const MAX_FILES_PER_SEND: usize = 1000;

// ---------------------------------------------------------------------------------------------
// Dış dünya ile sözleşme
// ---------------------------------------------------------------------------------------------

/// Kullanıcıya gösterilecek paylaşım olayları (yerel bildirim vb. uygulama katmanında yapılır).
#[derive(Debug, Clone)]
pub enum ShareNotice {
    FileReceived {
        device: String,
        name: String,
        size: u64,
        folder: PathBuf,
        path: PathBuf,
    },
    /// Telefon bir metin paylaştı: panoya konur ve bildirilir (KDE ile aynı davranış).
    Text { device: String, text: String },
    /// Telefon bir bağlantı paylaştı: KENDİLİĞİNDEN AÇILMAZ; panoya konur ve bildirilir.
    Url { device: String, url: String },
}

pub trait ShareNotifier: Send + Sync + 'static {
    fn notify(&self, notice: ShareNotice);
}

pub struct NoopNotifier;
impl ShareNotifier for NoopNotifier {
    fn notify(&self, _notice: ShareNotice) {}
}

/// Eklentinin çalışması için gereken ortam (yönetici başladıktan sonra `attach` ile verilir).
pub struct ShareEnv {
    pub local: Arc<LocalIdentity>,
    pub settings: Arc<SettingsStore>,
    pub sink: Arc<dyn EventSink>,
    pub notifier: Arc<dyn ShareNotifier>,
    /// Ayarlardaki indirme klasörü boşsa kullanılacak klasör (işletim sisteminin İndirilenler'i).
    pub default_download_dir: PathBuf,
    /// Gönderici yük dinleyicisi port aralığı (varsayılan 1739–1764).
    pub payload_ports: (u16, u16),
}

pub const DEFAULT_PAYLOAD_PORTS: (u16, u16) = (1739, 1764);

// ---------------------------------------------------------------------------------------------
// Hata türleri
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
enum Fail {
    /// Bağlantı koptu / kesildi.
    Lost,
    /// Başka bir hata (açıklama loga yazılır; dosya adı içermez).
    Other(String),
    /// Kullanıcı ya da bağlantı kaybı yüzünden iptal; durum başka yerde ayarlandı.
    Cancelled,
}

impl Fail {
    fn code(&self) -> &'static str {
        match self {
            Fail::Lost => "connectionLost",
            _ => "other",
        }
    }
}

/// Ağ G/Ç hatası: kopma türlerini `Lost`, gerisini `Other` yapar.
fn net_fail(e: io::Error) -> Fail {
    use io::ErrorKind::*;
    match e.kind() {
        UnexpectedEof | BrokenPipe | ConnectionReset | ConnectionAborted | TimedOut
        | NotConnected | WriteZero => Fail::Lost,
        _ => Fail::Other(e.to_string()),
    }
}

/// Yerel dosya sistemi hatası (disk dolu, izin yok...).
fn disk_fail(e: io::Error) -> Fail {
    Fail::Other(e.to_string())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn system_time_ms(t: io::Result<SystemTime>) -> Option<i64> {
    t.ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as i64)
}

/// Yalnızca http/https/mailto bağlantıları (KDE ile aynı); `file:` ve özel şemalar reddedilir.
pub fn url_allowed(url: &str) -> bool {
    if url.is_empty() || url.len() > MAX_URL_BYTES || url.chars().any(char::is_control) {
        return false;
    }
    let lower = url.to_ascii_lowercase();
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|p| lower.starts_with(p))
}

// ---------------------------------------------------------------------------------------------
// Durum
// ---------------------------------------------------------------------------------------------

struct Rec {
    view: Transfer,
    cancel: CancellationToken,
    /// Giden: kaynak dosya. Gelen: bitince kaydedilen yol.
    path: Option<PathBuf>,
    /// Giden toplu gönderim için (dosya sayısı, toplam bayt).
    batch: (u64, u64),
}

/// (görünüm, yol, iptal belirteci, toplu bilgisi)
type ActiveInfo = (Transfer, Option<PathBuf>, CancellationToken, (u64, u64));

struct RecvJob {
    id: String,
    device_name: String,
    name: String,
    /// `Some(n)` bilinen boyut (0 = boş dosya), `None` bilinmiyor.
    size: Option<u64>,
    port: Option<u16>,
    peer_ip: IpAddr,
    cert: Arc<Vec<u8>>,
    last_modified: Option<i64>,
}

#[derive(Default)]
struct State {
    /// En eskiden en yeniye.
    transfers: Vec<Rec>,
    handles: HashMap<String, DeviceHandle>,
    send_q: HashMap<String, mpsc::UnboundedSender<String>>,
    recv_q: HashMap<String, mpsc::UnboundedSender<RecvJob>>,
}

pub struct ShareService {
    env: OnceLock<ShareEnv>,
    state: Mutex<State>,
    seq: AtomicU64,
    /// Yalnızca testler: bu kadar bayt gönderilince yük soketini aniden kes (0 = kapalı).
    #[cfg(test)]
    cut_after: AtomicU64,
}

impl ShareService {
    pub fn new() -> Arc<ShareService> {
        Arc::new(ShareService {
            env: OnceLock::new(),
            state: Mutex::new(State::default()),
            seq: AtomicU64::new(0),
            #[cfg(test)]
            cut_after: AtomicU64::new(0),
        })
    }

    /// Yönetici başladıktan sonra ortamı bağlar. Bağlanmadan gelen paketler yok sayılır.
    pub fn attach(&self, env: ShareEnv) {
        let _ = self.env.set(env);
    }

    fn env(&self) -> Option<&ShareEnv> {
        self.env.get()
    }

    fn st(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn emit_one(&self, v: &Transfer) {
        if let Some(e) = self.env() {
            e.sink.emit(
                "transfer-progress",
                serde_json::to_value(v).unwrap_or_default(),
            );
        }
    }

    fn emit_list(&self) {
        if let Some(e) = self.env() {
            let list = serde_json::to_value(self.list()).unwrap_or_default();
            e.sink.emit("transfers-changed", list);
        }
    }

    /// Yeniden eskiye sıralı (arayüz listesi en yeniyi başta gösterir).
    pub fn list(&self) -> Vec<Transfer> {
        self.st()
            .transfers
            .iter()
            .rev()
            .map(|r| r.view.clone())
            .collect()
    }

    fn new_id(&self) -> String {
        format!(
            "tr-{:x}-{}",
            now_ms(),
            self.seq.fetch_add(1, Ordering::Relaxed)
        )
    }

    // ------------------------------------------------------------------ kayıt yönetimi

    fn add(
        &self,
        device_id: &str,
        name: String,
        size: u64,
        direction: &'static str,
        path: Option<PathBuf>,
        batch: (u64, u64),
    ) -> (String, Transfer, CancellationToken) {
        let id = self.new_id();
        let cancel = CancellationToken::new();
        let view = Transfer {
            id: id.clone(),
            device_id: device_id.to_string(),
            name,
            size,
            direction,
            progress: 0,
            state: "active",
            when: now_ms(),
            error: None,
        };
        let mut st = self.st();
        st.transfers.push(Rec {
            view: view.clone(),
            cancel: cancel.clone(),
            path,
            batch,
        });
        // Liste şişmesin: fazla ise en eski bitmişleri at.
        while st.transfers.len() > MAX_KEEP {
            match st.transfers.iter().position(|r| r.view.state != "active") {
                Some(i) => {
                    st.transfers.remove(i);
                }
                None => break,
            }
        }
        (id, view, cancel)
    }

    /// Kayıt hâlâ etkinse (iptal/başarısız olmadıysa) bilgilerini verir.
    fn active_info(&self, id: &str) -> Option<ActiveInfo> {
        let st = self.st();
        st.transfers
            .iter()
            .find(|r| r.view.id == id && r.view.state == "active")
            .map(|r| (r.view.clone(), r.path.clone(), r.cancel.clone(), r.batch))
    }

    fn set_progress(&self, id: &str, bytes: u64) {
        let view = {
            let mut st = self.st();
            st.transfers
                .iter_mut()
                .find(|r| r.view.id == id && r.view.state == "active")
                .map(|r| {
                    r.view.progress = bytes;
                    r.view.clone()
                })
        };
        if let Some(v) = view {
            self.emit_one(&v);
        }
    }

    fn finish_ok(&self, id: &str, bytes: u64, saved: Option<(String, PathBuf)>) {
        let view = {
            let mut st = self.st();
            st.transfers
                .iter_mut()
                .find(|r| r.view.id == id && r.view.state == "active")
                .map(|r| {
                    r.view.state = "done";
                    r.view.size = bytes;
                    r.view.progress = bytes;
                    if let Some((name, path)) = saved {
                        r.view.name = name;
                        r.path = Some(path);
                    }
                    r.view.clone()
                })
        };
        if let Some(v) = view {
            self.emit_one(&v);
        }
    }

    fn finish_err(&self, id: &str, fail: &Fail) {
        if matches!(fail, Fail::Cancelled) {
            return;
        }
        log::warn!("aktarım başarısız ({}): {:?}", fail.code(), fail);
        let view = {
            let mut st = self.st();
            st.transfers
                .iter_mut()
                .find(|r| r.view.id == id && r.view.state == "active")
                .map(|r| {
                    r.view.state = "failed";
                    r.view.error = Some(fail.code());
                    r.view.clone()
                })
        };
        if let Some(v) = view {
            self.emit_one(&v);
        }
    }

    // ------------------------------------------------------------------ arayüz komutları

    /// Aktifse iptal eder (kayıt listeden de kalkar); bitmiş/başarısız/iptal ise yalnızca listeden siler.
    pub fn cancel_transfer(&self, id: &str) {
        let removed = {
            let mut st = self.st();
            st.transfers
                .iter()
                .position(|r| r.view.id == id)
                .map(|i| st.transfers.remove(i))
        };
        if let Some(rec) = removed {
            // İptal belirteci yük görevini durdurur; görev `.part` dosyasını kendisi siler.
            rec.cancel.cancel();
            self.emit_list();
        }
    }

    /// Başarısız bir GİDEN aktarımı yeniden dener.
    pub fn retry_transfer(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let device_id = {
            let mut st = self.st();
            let rec = st
                .transfers
                .iter_mut()
                .find(|r| r.view.id == id)
                .ok_or("aktarım bulunamadı")?;
            if rec.view.state != "failed" || rec.view.direction != "send" || rec.path.is_none() {
                return Err("yalnızca başarısız giden aktarımlar yeniden denenir".into());
            }
            rec.view.state = "active";
            rec.view.error = None;
            rec.view.progress = 0;
            rec.view.when = now_ms();
            rec.cancel = CancellationToken::new();
            let v = rec.view.clone();
            let dev = v.device_id.clone();
            drop(st);
            self.emit_one(&v);
            dev
        };
        self.enqueue_send(&device_id, id.to_string());
        Ok(())
    }

    /// Aktarımın yerel dosyası: gelen için kaydedilen yol, giden için kaynak. Dosya yoksa `None`.
    pub fn file_path(&self, id: &str) -> Option<PathBuf> {
        let st = self.st();
        let rec = st.transfers.iter().find(|r| r.view.id == id)?;
        if rec.view.state != "done" {
            return None;
        }
        rec.path.clone().filter(|p| p.exists())
    }

    pub fn is_incoming(&self, id: &str) -> bool {
        self.st()
            .transfers
            .iter()
            .any(|r| r.view.id == id && r.view.direction == "receive")
    }

    /// Dosyaları eşleşmiş ve bağlı bir cihaza gönderir; oluşan aktarım kimliklerini döndürür.
    pub fn send_files(
        self: &Arc<Self>,
        device_id: &str,
        paths: Vec<String>,
    ) -> Result<Vec<String>, String> {
        self.env().ok_or("paylaşım hazır değil")?;
        let handle = self
            .st()
            .handles
            .get(device_id)
            .cloned()
            .ok_or("cihaza ulaşılamıyor")?;
        if !handle.peer_incoming.iter().any(|t| t == TYPE_REQUEST) {
            return Err("cihaz dosya almayı desteklemiyor".into());
        }
        // Yalnızca mutlak yoldaki gerçek dosyalar (klasör ve göreli ad kabul edilmez).
        let mut files: Vec<(PathBuf, u64)> = Vec::new();
        for p in paths.into_iter().take(MAX_FILES_PER_SEND) {
            let pb = PathBuf::from(&p);
            if !pb.is_absolute() {
                log::warn!("göreli yol yok sayıldı");
                continue;
            }
            match std::fs::metadata(&pb) {
                Ok(m) if m.is_file() => files.push((pb, m.len())),
                _ => log::warn!("dosya olmayan yol yok sayıldı"),
            }
        }
        if files.is_empty() {
            return Err("gönderilecek dosya yok".into());
        }
        let total: u64 = files.iter().map(|f| f.1).sum();
        let batch = (files.len() as u64, total);
        if batch.0 > 1 {
            // Alıcıya toplu toplamı bildir (Android bunu ilerleme için kullanır).
            handle.send(NetworkPacket::new(
                TYPE_UPDATE,
                json!({"numberOfFiles": batch.0, "totalPayloadSize": batch.1}),
            ));
        }
        let mut ids = Vec::new();
        let mut views = Vec::new();
        for (path, size) in files {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".into());
            let (id, view, _) = self.add(device_id, name, size, "send", Some(path), batch);
            ids.push(id);
            views.push(view);
        }
        for v in &views {
            self.emit_one(v);
        }
        self.emit_list();
        for id in &ids {
            self.enqueue_send(device_id, id.clone());
        }
        Ok(ids)
    }

    // ------------------------------------------------------------------ kuyruklar

    fn enqueue_send(self: &Arc<Self>, device_id: &str, id: String) {
        let tx = {
            let mut st = self.st();
            match st.send_q.get(device_id) {
                Some(tx) if !tx.is_closed() => tx.clone(),
                _ => {
                    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
                    st.send_q.insert(device_id.to_string(), tx.clone());
                    let me = self.clone();
                    let dev = device_id.to_string();
                    // Dosyalar sırayla gider: alıcı (Android) bir başlığı işlerken öbürü beklemez.
                    tokio::spawn(async move {
                        while let Some(id) = rx.recv().await {
                            me.run_upload(&dev, &id).await;
                        }
                    });
                    tx
                }
            }
        };
        let _ = tx.send(id);
    }

    fn enqueue_recv(self: &Arc<Self>, device_id: &str, job: RecvJob) {
        let tx = {
            let mut st = self.st();
            match st.recv_q.get(device_id) {
                Some(tx) if !tx.is_closed() => tx.clone(),
                _ => {
                    let (tx, mut rx) = mpsc::unbounded_channel::<RecvJob>();
                    st.recv_q.insert(device_id.to_string(), tx.clone());
                    let me = self.clone();
                    tokio::spawn(async move {
                        while let Some(job) = rx.recv().await {
                            me.run_download(job).await;
                        }
                    });
                    tx
                }
            }
        };
        let _ = tx.send(job);
    }

    // ------------------------------------------------------------------ giden: yükleme

    async fn run_upload(self: &Arc<Self>, device_id: &str, id: &str) {
        let Some((view, path, cancel, batch)) = self.active_info(id) else {
            return; // kuyrukta beklerken iptal edildi
        };
        let Some(path) = path else { return };
        let res = self
            .upload(device_id, &view, &path, &cancel, batch, id)
            .await;
        match res {
            Ok(bytes) => self.finish_ok(id, bytes, None),
            Err(f) => self.finish_err(id, &f),
        }
    }

    async fn upload(
        &self,
        device_id: &str,
        view: &Transfer,
        path: &Path,
        cancel: &CancellationToken,
        batch: (u64, u64),
        id: &str,
    ) -> Result<u64, Fail> {
        let env = self.env().ok_or_else(|| Fail::Other("ortam yok".into()))?;
        let handle = self
            .st()
            .handles
            .get(device_id)
            .cloned()
            .ok_or(Fail::Lost)?;
        let peer_ip = handle
            .peer_ip
            .ok_or_else(|| Fail::Other("eş adresi bilinmiyor".into()))?;
        if handle.peer_cert.is_empty() {
            return Err(Fail::Other("eş sertifikası yok".into()));
        }

        let mut file = tokio::fs::File::open(path).await.map_err(disk_fail)?;
        let meta = file.metadata().await.map_err(disk_fail)?;
        let size = meta.len();
        let mut body = json!({
            "filename": view.name,
            "open": false,
            "numberOfFiles": batch.0,
            "totalPayloadSize": batch.1,
        });
        if let Some(ms) = system_time_ms(meta.modified()) {
            body["lastModified"] = json!(ms);
        }
        if let Some(ms) = system_time_ms(meta.created()) {
            body["creationTime"] = json!(ms);
        }
        let mut packet = NetworkPacket::new(TYPE_REQUEST, body);

        if size == 0 {
            // Boş dosya: yük alanı yok (Android'in `hasPayload()` davranışı).
            return if handle.send(packet) {
                Ok(0)
            } else {
                Err(Fail::Lost)
            };
        }

        let (listener, port) = bind_payload(env.payload_ports).await?;
        packet.payload_size = Some(size as i64);
        packet.payload_transfer_info = Some(json!({ "port": port }));
        let cfg = tls::server_config(&env.local, Some((*handle.peer_cert).clone()))
            .map_err(Fail::Other)?;
        let acceptor = TlsAcceptor::from(cfg);
        if !handle.send(packet) {
            return Err(Fail::Lost);
        }
        let mut tls = accept_payload(&listener, peer_ip, &acceptor, cancel).await?;
        drop(listener);

        let mut buf = vec![0u8; CHUNK];
        let mut sent = 0u64;
        let mut last = Instant::now();
        while sent < size {
            let want = (size - sent).min(CHUNK as u64) as usize;
            let n = file.read(&mut buf[..want]).await.map_err(disk_fail)?;
            if n == 0 {
                return Err(Fail::Other("dosya beklenenden kısa".into()));
            }
            tokio::select! {
                _ = cancel.cancelled() => return Err(Fail::Cancelled),
                r = timeout(STALL_TIMEOUT, tls.write_all(&buf[..n])) => match r {
                    Err(_) => return Err(Fail::Lost),
                    Ok(Err(e)) => return Err(net_fail(e)),
                    Ok(Ok(())) => {}
                },
            }
            sent += n as u64;
            if last.elapsed() >= PROGRESS_EVERY {
                last = Instant::now();
                self.set_progress(id, sent);
            }
            #[cfg(test)]
            {
                let cut = self.cut_after.load(Ordering::Relaxed);
                if cut > 0 && sent >= cut {
                    // Bağlantı aniden kopuyor: TLS kapanış uyarısı bile gönderilmez.
                    drop(tls);
                    return Err(Fail::Lost);
                }
            }
        }
        // Temiz kapanış: alıcı veri sonunu görür.
        let _ = timeout(Duration::from_secs(5), async {
            tls.flush().await?;
            tls.shutdown().await
        })
        .await;
        Ok(sent)
    }

    // ------------------------------------------------------------------ gelen: indirme

    fn download_dir(&self) -> Result<PathBuf, Fail> {
        let env = self.env().ok_or_else(|| Fail::Other("ortam yok".into()))?;
        let configured = env.settings.get().download_dir;
        let mut dir = PathBuf::from(configured.trim());
        if configured.trim().is_empty() || !dir.is_absolute() {
            dir = env.default_download_dir.clone();
        }
        std::fs::create_dir_all(&dir).map_err(disk_fail)?;
        dir.canonicalize().map_err(disk_fail)
    }

    async fn run_download(self: &Arc<Self>, job: RecvJob) {
        let Some((_, _, cancel, _)) = self.active_info(&job.id) else {
            return; // sırada beklerken iptal edildi / bağlantı koptu
        };
        let mut part: Option<PathBuf> = None;
        let res = self.download(&job, &cancel, &mut part).await;
        match res {
            Ok((path, bytes)) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| job.name.clone());
                self.finish_ok(&job.id, bytes, Some((name.clone(), path.clone())));
                if let Some(env) = self.env()
                    && env.settings.get().notifications.file_received
                {
                    let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
                    let notice = ShareNotice::FileReceived {
                        device: job.device_name.clone(),
                        name,
                        size: bytes,
                        folder,
                        path,
                    };
                    let notifier = env.notifier.clone();
                    tokio::task::spawn_blocking(move || notifier.notify(notice));
                }
            }
            Err(f) => {
                if let Some(p) = part {
                    let _ = tokio::fs::remove_file(p).await;
                }
                self.finish_err(&job.id, &f);
            }
        }
    }

    /// Veriyi `.part` dosyasına akıtır, bitince güvenle son adına taşır. `(yol, bayt)` döner.
    async fn download(
        &self,
        job: &RecvJob,
        cancel: &CancellationToken,
        part_out: &mut Option<PathBuf>,
    ) -> Result<(PathBuf, u64), Fail> {
        let env = self.env().ok_or_else(|| Fail::Other("ortam yok".into()))?;
        let dir = self.download_dir()?;
        let part = dir.join(format!("tatar-{}.part", job.id));
        // Taşıma sırasında güvenli olması için `.part` yolu da klasörün içinde olmalı.
        let part = ensure_inside(&dir, &part).map_err(disk_fail)?;
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&part)
            .await
            .map_err(disk_fail)?;
        *part_out = Some(part.clone());

        let mut got = 0u64;
        if job.size == Some(0) && job.port.is_none() {
            // Boş dosya: bağlanmaya gerek yok.
        } else {
            let port = job
                .port
                .ok_or_else(|| Fail::Other("yük portu yok".into()))?;
            let cfg =
                tls::client_config(&env.local, Some((*job.cert).clone())).map_err(Fail::Other)?;
            let mut tls = connect_payload(job.peer_ip, port, cfg, cancel).await?;
            let mut buf = vec![0u8; CHUNK];
            let mut last = Instant::now();
            loop {
                let limit = match job.size {
                    Some(n) if got >= n => break,
                    Some(n) => (n - got).min(CHUNK as u64) as usize,
                    None => CHUNK,
                };
                let r = tokio::select! {
                    _ = cancel.cancelled() => return Err(Fail::Cancelled),
                    r = timeout(STALL_TIMEOUT, tls.read(&mut buf[..limit])) => r,
                };
                let n = match r {
                    Err(_) => return Err(Fail::Lost),
                    Ok(Ok(n)) => n,
                    // Boyutu bilinmeyen akışta TLS kapanış uyarısı olmadan bitiş de sondur.
                    Ok(Err(e))
                        if job.size.is_none() && e.kind() == io::ErrorKind::UnexpectedEof =>
                    {
                        0
                    }
                    Ok(Err(e)) => return Err(net_fail(e)),
                };
                if n == 0 {
                    if job.size.is_some() {
                        return Err(Fail::Lost); // beklenenden kısa kaldı
                    }
                    break;
                }
                file.write_all(&buf[..n]).await.map_err(disk_fail)?;
                got += n as u64;
                if job.size.is_none() && got > MAX_UNKNOWN_SIZE {
                    return Err(Fail::Other("boyutu bilinmeyen akış çok büyük".into()));
                }
                if last.elapsed() >= PROGRESS_EVERY {
                    last = Instant::now();
                    self.set_progress(&job.id, got);
                }
            }
        }
        file.flush().await.map_err(disk_fail)?;
        drop(file);

        if let Some(ms) = job.last_modified.filter(|m| *m > 0) {
            set_mtime(&part, ms);
        }
        let final_path = finalize_no_clobber(&part, &dir, &job.name).map_err(disk_fail)?;
        *part_out = None;
        Ok((final_path, got))
    }

    // ------------------------------------------------------------------ gelen paketler

    fn on_connected(&self, device: DeviceHandle) {
        self.st().handles.insert(device.id.clone(), device);
    }

    fn on_disconnected(&self, device_id: &str) {
        let affected: Vec<Transfer> = {
            let mut st = self.st();
            st.handles.remove(device_id);
            st.send_q.remove(device_id);
            st.recv_q.remove(device_id);
            let mut out = Vec::new();
            for r in st.transfers.iter_mut() {
                if r.view.device_id == device_id && r.view.state == "active" {
                    r.view.state = "failed";
                    r.view.error = Some("connectionLost");
                    r.cancel.cancel();
                    out.push(r.view.clone());
                }
            }
            out
        };
        for v in &affected {
            self.emit_one(v);
        }
    }

    fn on_packet(self: &Arc<Self>, device: &DeviceHandle, packet: &NetworkPacket) {
        if self.env().is_none() {
            return;
        }
        match packet.packet_type.as_str() {
            TYPE_REQUEST => self.on_request(device, packet),
            // Toplu toplamlar yalnızca bilgi; ilerleme dosya başına izlenir.
            TYPE_UPDATE => log::debug!("share.request.update alındı"),
            _ => {}
        }
    }

    fn on_request(self: &Arc<Self>, device: &DeviceHandle, p: &NetworkPacket) {
        let Some(env) = self.env() else { return };
        let filename = p.body_str("filename");
        if filename.is_some() || p.payload_transfer_info.is_some() {
            self.on_incoming_file(device, p, filename);
        } else if let Some(text) = p.body_str("text") {
            if text.is_empty() || text.len() > MAX_TEXT_BYTES {
                log::warn!("geçersiz boyutta paylaşılan metin atıldı");
                return;
            }
            let notice = ShareNotice::Text {
                device: device.name.clone(),
                text: text.to_string(),
            };
            let notifier = env.notifier.clone();
            tokio::task::spawn_blocking(move || notifier.notify(notice));
        } else if let Some(url) = p.body_str("url") {
            if !url_allowed(url) {
                log::warn!("izin verilmeyen şemada paylaşılan bağlantı atıldı");
                return;
            }
            let notice = ShareNotice::Url {
                device: device.name.clone(),
                url: url.to_string(),
            };
            let notifier = env.notifier.clone();
            tokio::task::spawn_blocking(move || notifier.notify(notice));
        } else {
            log::debug!("tanınmayan share.request içeriği yok sayıldı");
        }
    }

    fn on_incoming_file(
        self: &Arc<Self>,
        device: &DeviceHandle,
        p: &NetworkPacket,
        filename: Option<&str>,
    ) {
        let Some(peer_ip) = device.peer_ip else {
            return;
        };
        let port = p
            .payload_transfer_info
            .as_ref()
            .and_then(|v| v.get("port"))
            .and_then(lenient_i64)
            .filter(|p| (1..=65535).contains(p))
            .map(|p| p as u16);
        let size: Option<u64> = match (p.payload_size, port) {
            (Some(s), _) if s < -1 => {
                log::warn!("geçersiz payloadSize, dosya atıldı");
                return;
            }
            (Some(s), _) if s >= 0 => Some(s as u64),
            (Some(_), _) => None, // -1: bilinmiyor
            (None, Some(_)) => None,
            (None, None) => Some(0), // yük alanı yok: boş dosya
        };
        if size.is_some_and(|n| n > MAX_FILE_SIZE) {
            log::warn!("aşırı büyük dosya duyurusu atıldı");
            return;
        }
        if port.is_none() && size != Some(0) {
            log::warn!("yük portu olmayan dosya duyurusu atıldı");
            return;
        }
        // GÜVENLİK: ağdan gelen ad her zaman temizlenir (CLAUDE.md §4 kural 3).
        let name = sanitize_filename(filename.unwrap_or(""));
        let last_modified = p.body.get("lastModified").and_then(lenient_i64);
        let (id, view, _) = self.add(
            &device.id,
            name.clone(),
            size.unwrap_or(0),
            "receive",
            None,
            (0, 0),
        );
        self.emit_one(&view);
        let job = RecvJob {
            id,
            device_name: device.name.clone(),
            name,
            size,
            port,
            peer_ip,
            cert: device.peer_cert.clone(),
            last_modified,
        };
        self.enqueue_recv(&device.id, job);
    }
}

fn set_mtime(path: &Path, ms: i64) {
    let Ok(ms) = u64::try_from(ms) else { return };
    let t = UNIX_EPOCH + Duration::from_millis(ms);
    if let Ok(f) = std::fs::OpenOptions::new().write(true).open(path) {
        let _ = f.set_modified(t);
    }
}

// ---------------------------------------------------------------------------------------------
// Yük soketleri
// ---------------------------------------------------------------------------------------------

async fn bind_payload(range: (u16, u16)) -> Result<(TcpListener, u16), Fail> {
    for port in range.0..=range.1 {
        if let Ok(l) = TcpListener::bind(("0.0.0.0", port)).await {
            return Ok((l, port));
        }
    }
    Err(Fail::Other("boş yük portu yok".into()))
}

/// Alıcının bağlanmasını bekler. Yalnızca kontrol bağlantısının karşı IP'sinden ve eşin
/// sabitlenmiş sertifikasıyla TLS el sıkışmasını tamamlayan bağlantı kabul edilir.
async fn accept_payload(
    listener: &TcpListener,
    peer_ip: IpAddr,
    acceptor: &TlsAcceptor,
    cancel: &CancellationToken,
) -> Result<tokio_rustls::server::TlsStream<TcpStream>, Fail> {
    let deadline = Instant::now() + ACCEPT_TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Fail::Other("alıcı bağlanmadı".into()));
        }
        let (sock, addr) = tokio::select! {
            _ = cancel.cancelled() => return Err(Fail::Cancelled),
            r = timeout(left, listener.accept()) => match r {
                Err(_) => return Err(Fail::Other("alıcı bağlanmadı".into())),
                Ok(Err(e)) => return Err(net_fail(e)),
                Ok(Ok(x)) => x,
            },
        };
        if addr.ip().to_canonical() != peer_ip.to_canonical() {
            log::warn!("yük soketine beklenmeyen adresten bağlantı reddedildi");
            continue;
        }
        configure_socket(&sock);
        match timeout(TLS_TIMEOUT, acceptor.accept(sock)).await {
            Ok(Ok(t)) => return Ok(t),
            _ => log::warn!("yük soketi TLS el sıkışması başarısız, bağlantı atıldı"),
        }
    }
}

async fn connect_payload(
    ip: IpAddr,
    port: u16,
    cfg: Arc<rustls::ClientConfig>,
    cancel: &CancellationToken,
) -> Result<tokio_rustls::client::TlsStream<TcpStream>, Fail> {
    let connect = async {
        let sock = timeout(CONNECT_TIMEOUT, TcpStream::connect((ip, port)))
            .await
            .map_err(|_| Fail::Other("yük soketine bağlanılamadı (zaman aşımı)".into()))?
            .map_err(|e| Fail::Other(format!("yük soketine bağlanılamadı: {e}")))?;
        configure_socket(&sock);
        timeout(
            TLS_TIMEOUT,
            TlsConnector::from(cfg).connect(tls::dummy_server_name(), sock),
        )
        .await
        .map_err(|_| Fail::Other("yük soketi TLS zaman aşımı".into()))?
        .map_err(|e| Fail::Other(format!("yük soketi TLS hatası: {e}")))
    };
    tokio::select! {
        _ = cancel.cancelled() => Err(Fail::Cancelled),
        r = connect => r,
    }
}

// ---------------------------------------------------------------------------------------------
// Eklenti kaydı
// ---------------------------------------------------------------------------------------------

/// `Registry`'ye kaydolan ince sarmalayıcı; asıl iş `ShareService`'te.
pub struct SharePlugin(pub Arc<ShareService>);

impl Plugin for SharePlugin {
    fn name(&self) -> &'static str {
        "share"
    }
    fn incoming_types(&self) -> &'static [&'static str] {
        &[TYPE_REQUEST, TYPE_UPDATE]
    }
    fn outgoing_types(&self) -> &'static [&'static str] {
        &[TYPE_REQUEST, TYPE_UPDATE]
    }
    fn handle(&self, device: &DeviceHandle, packet: &NetworkPacket) {
        self.0.on_packet(device, packet);
    }
    fn device_connected(&self, device: &DeviceHandle) {
        self.0.on_connected(device.clone());
    }
    fn device_disconnected(&self, device_id: &str) {
        self.0.on_disconnected(device_id);
    }
}

// ---------------------------------------------------------------------------------------------
// Testler
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::{ManagerConfig, ManagerHandle, start};
    use crate::plugins::Registry;
    use crate::protocol::cert::MemoryKeyStore;
    use crate::store::SettingsPatch;
    use serde_json::Value;
    use std::net::SocketAddr;

    #[test]
    fn url_scheme_allowlist() {
        assert!(url_allowed("https://example.com/a?b=c"));
        assert!(url_allowed("HTTP://example.com"));
        assert!(url_allowed("mailto:ali@example.com"));
        assert!(!url_allowed("file:///etc/passwd"));
        assert!(!url_allowed("javascript:alert(1)"));
        assert!(!url_allowed("smb://host/share"));
        assert!(!url_allowed("tatar://x"));
        assert!(!url_allowed(""));
        assert!(!url_allowed("https://a.com/\nb"));
    }

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

    #[derive(Default)]
    struct TestNotifier(Mutex<Vec<ShareNotice>>);
    impl ShareNotifier for TestNotifier {
        fn notify(&self, n: ShareNotice) {
            self.0.lock().unwrap().push(n);
        }
    }

    struct Inst {
        mgr: ManagerHandle,
        svc: Arc<ShareService>,
        sink: Arc<TestSink>,
        notes: Arc<TestNotifier>,
        dir: PathBuf,
        downloads: PathBuf,
    }

    async fn instance(tcp: (u16, u16), payload: (u16, u16)) -> Inst {
        let dir = std::env::temp_dir().join(format!(
            "tatar-share-{}",
            crate::protocol::identity::generate_device_id()
        ));
        let downloads = dir.join("indirilenler");
        std::fs::create_dir_all(&downloads).unwrap();
        let svc = ShareService::new();
        let mut registry = Registry::new();
        registry.register(Arc::new(SharePlugin(svc.clone())));
        let sink = Arc::new(TestSink::default());
        let notes = Arc::new(TestNotifier::default());
        let cfg = ManagerConfig {
            data_dir: dir.clone(),
            keys: Arc::new(MemoryKeyStore::default()),
            udp_port: 0,
            tcp_port_range: tcp,
            enable_udp: false,
            enable_mdns: false,
            allow_loopback: true,
            registry,
        };
        let mgr = start(cfg, sink.clone()).await.unwrap();
        mgr.set_settings(SettingsPatch {
            download_dir: Some(downloads.to_string_lossy().into_owned()),
            ..Default::default()
        });
        svc.attach(ShareEnv {
            local: mgr.local_identity(),
            settings: mgr.settings_store(),
            sink: sink.clone(),
            notifier: notes.clone(),
            default_download_dir: downloads.clone(),
            payload_ports: payload,
        });
        Inst {
            mgr,
            svc,
            sink,
            notes,
            dir,
            downloads,
        }
    }

    async fn until(mut f: impl FnMut() -> bool) {
        for _ in 0..400 {
            if f() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("zaman aşımı");
    }

    /// İki örneği bağlar ve eşleştirir (A -> B).
    async fn pair(a: &Inst, b: &Inst) -> (String, String) {
        let (ia, ib) = (
            a.mgr.get_identity().device_id,
            b.mgr.get_identity().device_id,
        );
        a.mgr
            .connect_to(SocketAddr::from(([127, 0, 0, 1], b.mgr.tcp_port())));
        for _ in 0..100 {
            let la = a.mgr.list_devices().await;
            let lb = b.mgr.list_devices().await;
            if la.iter().any(|d| d.id == ib && d.reachable)
                && lb.iter().any(|d| d.id == ia && d.reachable)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        a.mgr.request_pair(&ib).await.unwrap();
        until(|| {
            b.sink
                .find("pair-request", |p| p["deviceId"] == ia.as_str())
                .is_some()
        })
        .await;
        b.mgr.accept_pair(&ia);
        until(|| {
            a.sink
                .find("pair-result", |p| p["success"] == true)
                .is_some()
        })
        .await;
        until(|| {
            b.sink
                .find("pair-result", |p| p["success"] == true)
                .is_some()
        })
        .await;
        // Eklentiler bağlantıyı görsün.
        until(|| a.svc.st().handles.contains_key(&ib) && b.svc.st().handles.contains_key(&ia))
            .await;
        (ia, ib)
    }

    fn make_file(dir: &Path, name: &str, len: usize) -> (PathBuf, Vec<u8>) {
        // Yarı rastgele ama tekrarlanabilir içerik.
        let mut data = Vec::with_capacity(len);
        let mut x: u32 = 0x9E37_79B9;
        while data.len() < len {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            data.push((x >> 24) as u8);
        }
        let p = dir.join(name);
        std::fs::write(&p, &data).unwrap();
        (p, data)
    }

    fn leftover_parts(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".part"))
            .collect()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transfers_multi_mb_file_between_instances() {
        let a = instance((47200, 47210), (47220, 47225)).await;
        let b = instance((47230, 47240), (47250, 47255)).await;
        let (_ia, ib) = pair(&a, &b).await;

        let (src, data) = make_file(&a.dir, "Şartname_Rev2.docx", 8 * 1024 * 1024 + 123);
        // Hedefte aynı adlı bir dosya zaten var: üzerine yazılmamalı.
        std::fs::write(b.downloads.join("Şartname_Rev2.docx"), b"eski").unwrap();
        let ids = a
            .svc
            .send_files(&ib, vec![src.to_string_lossy().into_owned()])
            .unwrap();
        assert_eq!(ids.len(), 1);

        until(|| {
            a.sink
                .find("transfer-progress", |p| {
                    p["state"] == "done" && p["direction"] == "send"
                })
                .is_some()
        })
        .await;
        until(|| {
            b.sink
                .find("transfer-progress", |p| {
                    p["state"] == "done" && p["direction"] == "receive"
                })
                .is_some()
        })
        .await;

        let got = std::fs::read(b.downloads.join("Şartname_Rev2 (1).docx")).unwrap();
        assert_eq!(got.len(), data.len());
        assert!(got == data, "içerik bozuk");
        assert_eq!(
            std::fs::read(b.downloads.join("Şartname_Rev2.docx")).unwrap(),
            b"eski"
        );
        assert!(leftover_parts(&b.downloads).is_empty());
        let list = b.svc.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].state, "done");
        assert_eq!(list[0].name, "Şartname_Rev2 (1).docx");
        assert_eq!(list[0].size, data.len() as u64);
        // Bildirim gitti.
        until(|| !b.notes.0.lock().unwrap().is_empty()).await;
        assert!(matches!(
            b.notes.0.lock().unwrap()[0],
            ShareNotice::FileReceived { .. }
        ));
        // Listeden silme.
        let id = list[0].id.clone();
        assert!(b.svc.file_path(&id).is_some());
        b.svc.cancel_transfer(&id);
        assert!(b.svc.list().is_empty());

        a.mgr.shutdown();
        b.mgr.shutdown();
        let _ = std::fs::remove_dir_all(&a.dir);
        let _ = std::fs::remove_dir_all(&b.dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn sender_drop_mid_transfer_fails_and_leaves_no_file() {
        let a = instance((47300, 47310), (47320, 47325)).await;
        let b = instance((47330, 47340), (47350, 47355)).await;
        let (_ia, ib) = pair(&a, &b).await;

        let (src, _) = make_file(&a.dir, "buyuk.bin", 6 * 1024 * 1024);
        a.svc.cut_after.store(1024 * 1024, Ordering::Relaxed);
        a.svc
            .send_files(&ib, vec![src.to_string_lossy().into_owned()])
            .unwrap();

        until(|| {
            b.sink
                .find("transfer-progress", |p| p["state"] == "failed")
                .is_some()
        })
        .await;
        let failed = b
            .sink
            .find("transfer-progress", |p| p["state"] == "failed")
            .unwrap();
        assert_eq!(failed["error"], "connectionLost");
        until(|| {
            a.sink
                .find("transfer-progress", |p| p["state"] == "failed")
                .is_some()
        })
        .await;

        // Alıcıda ne son dosya ne de .part kaldı.
        let names: Vec<_> = std::fs::read_dir(&b.downloads)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.is_empty(), "artık dosyalar: {names:?}");

        // Gönderen tarafta yeniden dene çalışır (bu kez kesme yok).
        a.svc.cut_after.store(0, Ordering::Relaxed);
        let aid = a.svc.list()[0].id.clone();
        a.svc.retry_transfer(&aid).unwrap();
        until(|| b.downloads.join("buyuk.bin").exists() && leftover_parts(&b.downloads).is_empty())
            .await;
        until(|| a.svc.list()[0].state == "done").await;

        a.mgr.shutdown();
        b.mgr.shutdown();
        let _ = std::fs::remove_dir_all(&a.dir);
        let _ = std::fs::remove_dir_all(&b.dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn rejects_non_files_and_hostile_names() {
        let a = instance((47400, 47410), (47420, 47425)).await;
        let b = instance((47430, 47440), (47450, 47455)).await;
        let (ia, ib) = pair(&a, &b).await;

        // Göreli yol ve klasör gönderilemez.
        assert!(a.svc.send_files(&ib, vec!["goreli.txt".into()]).is_err());
        assert!(
            a.svc
                .send_files(&ib, vec![a.dir.to_string_lossy().into_owned()])
                .is_err()
        );
        // Bilinmeyen cihaz.
        assert!(a.svc.send_files("yok", vec![]).is_err());

        // Kötü niyetli ad: B tarafında, A'nın gönderdiği başlıkta yol kaçışı denenir.
        let (src, data) = make_file(&a.dir, "x.bin", 4096);
        let handle = a.svc.st().handles.get(&ib).cloned().unwrap();
        let listener = TcpListener::bind(("0.0.0.0", 47460)).await.unwrap();
        let cfg =
            tls::server_config(&a.mgr.local_identity(), Some((*handle.peer_cert).clone())).unwrap();
        let mut pkt = NetworkPacket::new(
            TYPE_REQUEST,
            json!({"filename": "../../etc/.passwd", "numberOfFiles": 1, "totalPayloadSize": 4096}),
        );
        pkt.payload_size = Some(4096);
        pkt.payload_transfer_info = Some(json!({"port": 47460}));
        assert!(handle.send(pkt));
        let acceptor = TlsAcceptor::from(cfg);
        let mut s = accept_payload(
            &listener,
            "127.0.0.1".parse().unwrap(),
            &acceptor,
            &CancellationToken::new(),
        )
        .await
        .unwrap_or_else(|_| panic!("kabul edilemedi"));
        s.write_all(&data).await.unwrap();
        s.shutdown().await.unwrap();
        drop(src);

        until(|| {
            b.sink
                .find("transfer-progress", |p| p["state"] == "done")
                .is_some()
        })
        .await;
        // Yalnızca indirme klasörünün içinde, temizlenmiş adla.
        assert_eq!(std::fs::read(b.downloads.join("passwd")).unwrap(), data);
        assert!(!b.dir.join("etc").exists());
        let _ = ia;

        a.mgr.shutdown();
        b.mgr.shutdown();
        let _ = std::fs::remove_dir_all(&a.dir);
        let _ = std::fs::remove_dir_all(&b.dir);
    }
}
