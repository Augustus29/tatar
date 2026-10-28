//! Bağlantı kurulumu (kontrol kanalı) ve kurulmuş bağlantının okuma/yazma görevleri.
//!
//! Sıra: TCP -> (açan taraf) düz kimlik -> TLS (roller TERS) -> iki taraf TLS içinde tam kimlik
//! -> CVE-2025-66270 denetimleri -> bağlantı kurulur.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::io::{AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_rustls::{TlsAcceptor, TlsConnector, TlsStream};
use tokio_util::sync::CancellationToken;

use super::cert::{LocalIdentity, common_name};
use super::identity::{
    Identity, MAX_TCP_PORT, MIN_TCP_PORT, PROTOCOL_VERSION, verify_secure_identity,
};
use super::packet::{
    MAX_IDENTITY_LINE, MAX_PACKET_LINE, MAX_UNPAIRED_LINE, NetworkPacket, read_line_limited,
};
use super::tls;
use crate::store::TrustedStore;

const IO_TIMEOUT: Duration = Duration::from_secs(5);
const TLS_TIMEOUT: Duration = Duration::from_secs(10);
/// Tek bir el sıkışmanın (TCP + düz kimlik + TLS + TLS kimliği) toplam süresi. Adım başına
/// zaman aşımları toplamda bunu aşabildiği için (yavaş gönderen saldırgan) ayrıca uygulanır.
pub const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(15);
/// Eşleşmemiş bir bağlantıda bu kadar bozuk paketten sonra bağlantı kapatılır.
pub const MAX_BAD_PACKETS_UNPAIRED: u32 = 20;
/// Okuyucudan yöneticiye giden olay kuyruğu (dolunca okuyucu bekler: geri basınç).
pub const LINK_EVENT_QUEUE: usize = 256;

pub type HsResult<T> = Result<T, String>;

/// Handshake için gereken ortak bilgiler.
#[derive(Clone)]
pub struct HandshakeCtx {
    pub local: Arc<LocalIdentity>,
    pub trusted: Arc<TrustedStore>,
    /// Şu anki kimliğimiz (ad değişebilir).
    pub identity: Identity,
}

pub type TlsIo = TlsStream<TcpStream>;

/// Kurulmuş (doğrulanmış) bağlantı.
pub struct Established {
    pub identity: Identity,
    pub cert_der: Vec<u8>,
    /// Eşin gerçek uzak adresi (TCP soketinden; keşif duyurusundan değil).
    pub addr: SocketAddr,
    /// Eşin TCP dinleme portu: giden bağlantıda bağlandığımız port; gelende kimlikteki `tcpPort`
    /// (aralıkta ise). Gelen bağlantının kaynak portu geçicidir, bu yüzden kullanılmaz.
    pub listen_port: Option<u16>,
    /// Bağlantıyı biz açtık mı (giden)? Çift yönlü eşzamanlı bağlantıda kazananı seçmek için.
    pub outgoing: bool,
    pub reader: BufReader<ReadHalf<TlsIo>>,
    pub writer: WriteHalf<TlsIo>,
}

/// TCP keepalive: 30 sn boşta, 10 sn aralık, 3 deneme (KDE ile aynı).
pub fn configure_socket(stream: &TcpStream) {
    let _ = stream.set_nodelay(true);
    let ka = socket2::TcpKeepalive::new()
        .with_time(Duration::from_secs(30))
        .with_interval(Duration::from_secs(10));
    #[cfg(any(unix, target_os = "windows"))]
    #[cfg(not(any(target_os = "openbsd", target_os = "haiku", target_os = "vita")))]
    let ka = ka.with_retries(3);
    let _ = socket2::SockRef::from(stream).set_tcp_keepalive(&ka);
}

fn peer_cert_der(tls: &TlsIo) -> Option<Vec<u8>> {
    tls.get_ref()
        .1
        .peer_certificates()?
        .first()
        .map(|c| c.as_ref().to_vec())
}

/// Eşleşmiş bir cihaz için protokol düşürme denetimi (notlar §3.4).
pub fn is_downgrade(trusted: &TrustedStore, device_id: &str, version: i64) -> bool {
    trusted
        .get(device_id)
        .is_some_and(|t| version < t.protocol_version)
}

/// TLS sonrası ortak adımlar. `pre_*` TLS öncesi görülen değerlerdir.
async fn finish(
    ctx: &HandshakeCtx,
    tls: TlsIo,
    addr: SocketAddr,
    pre_device_id: Option<String>,
    pre_protocol: i64,
    outgoing: bool,
) -> HsResult<Established> {
    let cert_der = peer_cert_der(&tls).ok_or("eş sertifika sunmadı")?;
    let (rd, mut wr) = tokio::io::split(tls);
    let mut reader = BufReader::new(rd);

    // İki taraf da hemen yazar; eşin satırını beklemeden gönderiyoruz.
    let ours = ctx.identity.to_secure_packet().serialize();
    timeout(IO_TIMEOUT, async {
        wr.write_all(&ours).await?;
        wr.flush().await
    })
    .await
    .map_err(|_| "kimlik yazma zaman aşımı")?
    .map_err(|e| format!("kimlik yazılamadı: {e}"))?;

    let line = timeout(
        IO_TIMEOUT,
        read_line_limited(&mut reader, MAX_IDENTITY_LINE),
    )
    .await
    .map_err(|_| "TLS kimliği zaman aşımı")?
    .map_err(|e| format!("TLS kimliği okunamadı: {e}"))?
    .ok_or("bağlantı kimlik gelmeden kapandı")?;
    let packet =
        NetworkPacket::parse(&line, MAX_IDENTITY_LINE).map_err(|e| format!("TLS kimliği: {e}"))?;
    let secure =
        Identity::from_packet(&packet).map_err(|e| format!("TLS kimliği geçersiz: {e:?}"))?;

    // CVE-2025-66270: kimlik, sürüm ve sertifika CN'i TLS öncesiyle uyuşmalı.
    let cn = common_name(&cert_der);
    verify_secure_identity(
        pre_device_id.as_deref(),
        pre_protocol,
        &secure,
        cn.as_deref(),
    )
    .map_err(|e| format!("kimlik uyuşmazlığı: {e:?}"))?;

    if secure.protocol_version < PROTOCOL_VERSION {
        return Err("eski protokol sürümü".into());
    }
    if secure.device_id == ctx.local.device_id {
        return Err("kendi kimliğimizle bağlantı".into());
    }
    // Eşleşmiş cihaz: sertifika saklananla birebir aynı olmalı (TLS katmanına ek, savunma derinliği).
    if let Some(t) = ctx.trusted.get(&secure.device_id) {
        if t.cert != cert_der {
            return Err("eşleşmiş cihazın sertifikası değişmiş".into());
        }
        if secure.protocol_version < t.protocol_version {
            return Err("protokol düşürme girişimi".into());
        }
    }
    let listen_port = if outgoing {
        Some(addr.port())
    } else {
        secure
            .tcp_port
            .filter(|p| (MIN_TCP_PORT..=MAX_TCP_PORT).contains(p))
    };
    Ok(Established {
        identity: secure,
        cert_der,
        addr,
        listen_port,
        outgoing,
        reader,
        writer: wr,
    })
}

/// Bağlantıyı BİZ açtık: düz kimliği yaz, TLS SUNUCUSU ol.
/// `target`: biliniyorsa (UDP/mDNS'ten) (deviceId, protocolVersion); IP ile eklemede `None`.
pub async fn connect_out(
    ctx: &HandshakeCtx,
    addr: SocketAddr,
    target: Option<(String, i64)>,
) -> HsResult<Established> {
    timeout(HANDSHAKE_DEADLINE, connect_out_inner(ctx, addr, target))
        .await
        .map_err(|_| "el sıkışma toplam süreyi aştı".to_string())?
}

async fn connect_out_inner(
    ctx: &HandshakeCtx,
    addr: SocketAddr,
    target: Option<(String, i64)>,
) -> HsResult<Established> {
    let mut stream = timeout(IO_TIMEOUT, TcpStream::connect(addr))
        .await
        .map_err(|_| "bağlantı zaman aşımı")?
        .map_err(|e| format!("bağlanılamadı: {e}"))?;
    configure_socket(&stream);

    let (tid, tproto) = match &target {
        Some((id, v)) => (Some(id.as_str()), *v),
        None => (None, PROTOCOL_VERSION),
    };
    let plain = ctx.identity.to_plain_tcp_packet(tid, tproto).serialize();
    timeout(IO_TIMEOUT, async {
        stream.write_all(&plain).await?;
        stream.flush().await
    })
    .await
    .map_err(|_| "düz kimlik yazma zaman aşımı")?
    .map_err(|e| format!("düz kimlik yazılamadı: {e}"))?;

    let pinned = tid.and_then(|id| ctx.trusted.get(id)).map(|t| t.cert);
    let cfg = tls::server_config(&ctx.local, pinned)?;
    let tls = timeout(TLS_TIMEOUT, TlsAcceptor::from(cfg).accept(stream))
        .await
        .map_err(|_| "TLS zaman aşımı")?
        .map_err(|e| format!("TLS el sıkışması başarısız: {e}"))?;
    finish(
        ctx,
        TlsStream::Server(tls),
        addr,
        tid.map(str::to_string),
        tproto,
        true,
    )
    .await
}

/// Bağlantıyı BİZE açtılar: düz kimliği oku, TLS İSTEMCİSİ ol.
pub async fn accept_in(
    ctx: &HandshakeCtx,
    stream: TcpStream,
    addr: SocketAddr,
) -> HsResult<Established> {
    timeout(HANDSHAKE_DEADLINE, accept_in_inner(ctx, stream, addr))
        .await
        .map_err(|_| "el sıkışma toplam süreyi aştı".to_string())?
}

async fn accept_in_inner(
    ctx: &HandshakeCtx,
    stream: TcpStream,
    addr: SocketAddr,
) -> HsResult<Established> {
    configure_socket(&stream);
    let mut br = BufReader::new(stream);
    let line = timeout(IO_TIMEOUT, read_line_limited(&mut br, MAX_IDENTITY_LINE))
        .await
        .map_err(|_| "düz kimlik zaman aşımı")?
        .map_err(|e| format!("düz kimlik okunamadı: {e}"))?
        .ok_or("düz kimlik gelmeden kapandı")?;
    // Eşin TLS ClientHello'su için bekleyeceğiz; arabellekte fazladan veri olmamalı.
    if !br.buffer().is_empty() {
        return Err("düz kimlikten sonra beklenmeyen veri".into());
    }
    let stream = br.into_inner();
    let packet =
        NetworkPacket::parse(&line, MAX_IDENTITY_LINE).map_err(|e| format!("düz kimlik: {e}"))?;
    let plain =
        Identity::from_packet(&packet).map_err(|e| format!("düz kimlik geçersiz: {e:?}"))?;

    if plain.protocol_version < PROTOCOL_VERSION {
        return Err("eski protokol sürümü".into());
    }
    if plain.device_id == ctx.local.device_id {
        return Err("kendi kimliğimizle bağlantı".into());
    }
    if let Some(t) = &plain.target_device_id
        && *t != ctx.local.device_id
    {
        return Err("targetDeviceId bize ait değil".into());
    }
    if let Some(v) = plain.target_protocol_version
        && v != PROTOCOL_VERSION
    {
        return Err("targetProtocolVersion uyuşmuyor".into());
    }
    if is_downgrade(&ctx.trusted, &plain.device_id, plain.protocol_version) {
        return Err("protokol düşürme girişimi".into());
    }
    let pinned = ctx.trusted.get(&plain.device_id).map(|t| t.cert);
    let cfg = tls::client_config(&ctx.local, pinned)?;
    let tls = timeout(
        TLS_TIMEOUT,
        TlsConnector::from(cfg).connect(tls::dummy_server_name(), stream),
    )
    .await
    .map_err(|_| "TLS zaman aşımı")?
    .map_err(|e| format!("TLS el sıkışması başarısız: {e}"))?;
    finish(
        ctx,
        TlsStream::Client(tls),
        addr,
        Some(plain.device_id.clone()),
        plain.protocol_version,
        false,
    )
    .await
}

// ---------------------------------------------------------------------------------------------
// Kurulmuş bağlantının görevleri
// ---------------------------------------------------------------------------------------------

pub enum LinkEvent {
    Packet {
        device_id: String,
        generation: u64,
        packet: NetworkPacket,
    },
    Closed {
        device_id: String,
        generation: u64,
    },
}

pub struct LinkIo {
    pub tx: mpsc::Sender<NetworkPacket>,
    pub cancel: CancellationToken,
}

/// Okuma ve yazma görevlerini başlatır. `paired` doğruysa büyük (32 MiB) satırlar kabul edilir.
pub fn spawn_link(
    est: Established,
    device_id: String,
    generation: u64,
    paired: Arc<AtomicBool>,
    events: mpsc::Sender<LinkEvent>,
) -> LinkIo {
    let cancel = CancellationToken::new();
    let (tx, mut rx) = mpsc::channel::<NetworkPacket>(256);
    let Established {
        mut reader,
        mut writer,
        ..
    } = est;

    {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    pkt = rx.recv() => {
                        let Some(pkt) = pkt else { break };
                        let bytes = pkt.serialize();
                        if writer.write_all(&bytes).await.is_err() || writer.flush().await.is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = writer.shutdown().await;
            cancel.cancel();
        });
    }
    {
        let cancel = cancel.clone();
        let device_id = device_id.clone();
        tokio::spawn(async move {
            let mut bad_packets: u32 = 0;
            loop {
                let max = if paired.load(Ordering::Relaxed) {
                    MAX_PACKET_LINE
                } else {
                    MAX_UNPAIRED_LINE
                };
                let line = tokio::select! {
                    _ = cancel.cancelled() => break,
                    r = read_line_limited(&mut reader, max) => r,
                };
                match line {
                    Ok(Some(l)) => match NetworkPacket::parse(&l, max) {
                        Ok(packet) => {
                            let ev = LinkEvent::Packet {
                                device_id: device_id.clone(),
                                generation,
                                packet,
                            };
                            // Kuyruk doluysa bekle (geri basınç); iptal edilirse çık.
                            let sent = tokio::select! {
                                _ = cancel.cancelled() => break,
                                r = events.send(ev) => r,
                            };
                            if sent.is_err() {
                                break;
                            }
                        }
                        // Bozuk tek paket: içerik loglanmaz.
                        Err(e) => {
                            bad_packets = bad_packets.saturating_add(1);
                            if bad_packets == 1 {
                                log::warn!("bozuk paket atıldı: {e}");
                            }
                            if !paired.load(Ordering::Relaxed)
                                && bad_packets >= MAX_BAD_PACKETS_UNPAIRED
                            {
                                log::warn!("eşleşmemiş bağlantıda çok bozuk paket, kapatılıyor");
                                break;
                            }
                        }
                    },
                    Ok(None) => break,
                    Err(e) => {
                        log::info!("bağlantı okuma hatası: {:?}", e.kind());
                        break;
                    }
                }
            }
            cancel.cancel();
            let _ = events
                .send(LinkEvent::Closed {
                    device_id,
                    generation,
                })
                .await;
        });
    }
    LinkIo { tx, cancel }
}
