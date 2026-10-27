//! Cihaz keşfi: UDP 1716 (dinleme + yayın) ve mDNS (`_kdeconnect._udp`).
//! Keşif yalnızca "şu adreste bir cihaz var" olayını üretir; bağlantıyı yönetici açar.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;

use super::identity::{Identity, MAX_TCP_PORT, MIN_TCP_PORT, PROTOCOL_VERSION, is_valid_device_id};
use super::packet::{MAX_IDENTITY_LINE, NetworkPacket};

pub const MDNS_TYPE: &str = "_kdeconnect._udp.local.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Udp,
    Mdns,
}

#[derive(Debug, Clone)]
pub struct Discovered {
    pub device_id: String,
    pub name: Option<String>,
    pub device_type: Option<String>,
    pub protocol_version: i64,
    pub ip: IpAddr,
    pub tcp_port: u16,
    pub via: Via,
}

pub type Callback = Arc<dyn Fn(Discovered) + Send + Sync>;

/// IPv6'ya eşlenmiş IPv4'ü düz IPv4'e çevirir.
pub fn normalize_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(v6)),
        v4 => v4,
    }
}

/// UDP datagramını kimliğe çevirir ve keşif kurallarını uygular (saf fonksiyon, test edilebilir).
pub fn parse_udp_identity(
    data: &[u8],
    src: IpAddr,
    own_id: &str,
    allow_loopback: bool,
) -> Option<Discovered> {
    if data.len() > MAX_IDENTITY_LINE {
        return None;
    }
    let src = normalize_ip(src);
    if src.is_loopback() && !allow_loopback {
        return None;
    }
    let packet = NetworkPacket::parse(data, MAX_IDENTITY_LINE).ok()?;
    let id = Identity::from_packet(&packet).ok()?;
    if id.device_id == own_id || id.protocol_version < PROTOCOL_VERSION {
        return None;
    }
    let tcp_port = id.tcp_port?;
    Some(Discovered {
        device_id: id.device_id,
        name: Some(id.device_name),
        device_type: Some(id.device_type),
        protocol_version: id.protocol_version,
        ip: src,
        tcp_port,
        via: Via::Udp,
    })
}

fn bind_udp(port: u16) -> std::io::Result<UdpSocket> {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    // Aynı makinedeki başka bir KDE Connect ile bir arada çalışabilmek için.
    sock.set_reuse_address(true)?;
    #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
    sock.set_reuse_port(true)?;
    sock.set_broadcast(true)?;
    sock.set_nonblocking(true)?;
    sock.bind(&SocketAddr::from(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).into())?;
    UdpSocket::from_std(sock.into())
}

/// UDP dinleyici görevini başlatır.
pub fn start_udp_listener(
    port: u16,
    own_id: String,
    allow_loopback: bool,
    cb: Callback,
) -> std::io::Result<tokio::task::JoinHandle<()>> {
    let sock = bind_udp(port)?;
    Ok(tokio::spawn(async move {
        let mut buf = vec![0u8; 65536];
        loop {
            match sock.recv_from(&mut buf).await {
                Ok((n, src)) => {
                    if let Some(d) =
                        parse_udp_identity(&buf[..n], src.ip(), &own_id, allow_loopback)
                    {
                        cb(d);
                    }
                }
                Err(e) => {
                    log::warn!("UDP okuma hatası: {:?}", e.kind());
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }))
}

/// Yerel (loopback olmayan) IPv4 arayüzleri: (adres, yayın adresi).
pub fn local_ipv4_interfaces() -> Vec<(Ipv4Addr, Option<Ipv4Addr>)> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) => Some((v4.ip, v4.broadcast)),
            _ => None,
        })
        .collect()
}

/// Kimliği her arayüzün kendi adresinden `255.255.255.255:udp_port` (ve alt ağ yayın adresi) için
/// yayınlar. Engelleyen soket işi: `spawn_blocking` içinde çağrılmalı.
pub fn broadcast_blocking(datagram: &[u8], udp_port: u16) {
    for (ip, bcast) in local_ipv4_interfaces() {
        let Ok(sock) = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)) else {
            continue;
        };
        if sock.set_broadcast(true).is_err() {
            continue;
        }
        if sock
            .bind(&SocketAddr::from(SocketAddrV4::new(ip, 0)).into())
            .is_err()
        {
            continue;
        }
        let mut targets = vec![Ipv4Addr::BROADCAST];
        if let Some(b) = bcast
            && b != Ipv4Addr::BROADCAST
        {
            targets.push(b);
        }
        for t in targets {
            let _ = sock.send_to(
                datagram,
                &SocketAddr::from(SocketAddrV4::new(t, udp_port)).into(),
            );
        }
    }
}

/// Tek bir adrese UDP kimliği gönderir (elle ekleme / bağlantı kurtarma).
pub fn unicast_blocking(datagram: &[u8], target: SocketAddr) {
    let bind: SocketAddr = match target {
        SocketAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
        SocketAddr::V6(_) => (std::net::Ipv6Addr::UNSPECIFIED, 0).into(),
    };
    if let Ok(s) = std::net::UdpSocket::bind(bind) {
        let _ = s.send_to(datagram, target);
    }
}

// ---------------------------------------------------------------------------------------------
// mDNS
// ---------------------------------------------------------------------------------------------

pub struct Mdns {
    daemon: ServiceDaemon,
    own_id: String,
}

impl Mdns {
    /// Kendimizi duyurur (örnek adı = deviceId) ve başkalarını arar.
    pub fn start(
        identity: &Identity,
        tcp_port: u16,
        allow_loopback: bool,
        cb: Callback,
    ) -> Result<Mdns, String> {
        let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
        let m = Mdns {
            daemon,
            own_id: identity.device_id.clone(),
        };
        m.announce(identity, tcp_port)?;
        let rx = m.daemon.browse(MDNS_TYPE).map_err(|e| e.to_string())?;
        let own = m.own_id.clone();
        tokio::spawn(async move {
            while let Ok(ev) = rx.recv_async().await {
                let ServiceEvent::ServiceResolved(info) = ev else {
                    continue;
                };
                let fullname = info.get_fullname();
                let Some(instance) = fullname.strip_suffix(&format!(".{MDNS_TYPE}")) else {
                    continue;
                };
                if !is_valid_device_id(instance) || instance == own {
                    continue;
                }
                let proto = info
                    .txt_properties
                    .get_property_val_str("protocol")
                    .and_then(|s| s.trim().parse::<i64>().ok())
                    .unwrap_or(0);
                if proto < PROTOCOL_VERSION
                    || !(MIN_TCP_PORT..=MAX_TCP_PORT).contains(&info.get_port())
                {
                    continue;
                }
                let name = info
                    .txt_properties
                    .get_property_val_str("name")
                    .map(super::identity::filter_device_name)
                    .filter(|n| !n.is_empty());
                let dtype = info
                    .txt_properties
                    .get_property_val_str("type")
                    .map(str::to_string);
                // İlk geçerli IPv4 adresi.
                let Some(ip) = info
                    .get_addresses_v4()
                    .into_iter()
                    .find(|a| allow_loopback || !a.is_loopback())
                else {
                    continue;
                };
                cb(Discovered {
                    device_id: instance.to_string(),
                    name,
                    device_type: dtype,
                    protocol_version: proto,
                    ip: IpAddr::V4(ip),
                    tcp_port: info.get_port(),
                    via: Via::Mdns,
                });
            }
        });
        Ok(m)
    }

    /// (Yeniden) duyur; ad değişince çağrılır.
    pub fn announce(&self, identity: &Identity, tcp_port: u16) -> Result<(), String> {
        let host = format!("{}.local.", identity.device_id);
        let props = [
            ("id", identity.device_id.as_str()),
            ("name", identity.device_name.as_str()),
            ("type", identity.device_type.as_str()),
            ("protocol", "8"),
        ];
        let info = ServiceInfo::new(
            MDNS_TYPE,
            &identity.device_id,
            &host,
            "",
            tcp_port,
            &props[..],
        )
        .map_err(|e| e.to_string())?
        .enable_addr_auto();
        self.daemon.register(info).map_err(|e| e.to_string())
    }

    pub fn shutdown(&self) {
        let _ = self.daemon.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dgram(id: &str, port: u64, proto: u64) -> Vec<u8> {
        NetworkPacket::new(
            "kdeconnect.identity",
            json!({"deviceId": id, "deviceName": "Tel", "deviceType": "phone", "protocolVersion": proto, "tcpPort": port}),
        )
        .serialize()
    }
    const PEER: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const ME: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn udp_accepts_valid() {
        let d = parse_udp_identity(&dgram(PEER, 1716, 8), ip("192.168.1.9"), ME, false).unwrap();
        assert_eq!(d.tcp_port, 1716);
        assert_eq!(d.ip, ip("192.168.1.9"));
    }

    #[test]
    fn udp_rejections() {
        let lan = ip("192.168.1.9");
        assert!(
            parse_udp_identity(&dgram(ME, 1716, 8), lan, ME, false).is_none(),
            "kendimiz"
        );
        assert!(
            parse_udp_identity(&dgram(PEER, 1716, 7), lan, ME, false).is_none(),
            "eski protokol"
        );
        assert!(
            parse_udp_identity(&dgram(PEER, 80, 8), lan, ME, false).is_none(),
            "port aralığı"
        );
        assert!(
            parse_udp_identity(&dgram(PEER, 1716, 8), ip("127.0.0.1"), ME, false).is_none(),
            "loopback"
        );
        assert!(parse_udp_identity(&dgram(PEER, 1716, 8), ip("127.0.0.1"), ME, true).is_some());
        assert!(parse_udp_identity(b"{bozuk", lan, ME, false).is_none());
        assert!(parse_udp_identity(&vec![b'a'; 9000], lan, ME, false).is_none());
    }

    #[test]
    fn mapped_ipv6_is_normalised() {
        let mapped: IpAddr = "::ffff:192.168.1.9".parse().unwrap();
        assert_eq!(normalize_ip(mapped), ip("192.168.1.9"));
    }
}
