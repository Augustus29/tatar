//! "Bağlantı sorunlarını gider" denetimleri. Ağ olgularını toplayan kısım (`manager`, `platform`)
//! ile sonucu üreten saf kısım (`assemble`) ayrıdır; ikincisi birim testlidir.
//!
//! Sonuç sırası ve kimlikleri arayüzle sabittir: wifi, localNetwork, firewall, discovery, phoneApp.

use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use serde::Serialize;

use crate::protocol::discovery::Via;

/// "Yakın zamanda" sayılan pencere (ms): telefonlar yayını yalnızca açılışta/bağlanırken yapar.
pub const RECENT_MS: i64 = 120_000;
/// Eşleşmiş cihaz bu kadar yakın zamanda görüldüyse "yavaş yanıt veriyor" sayılır.
pub const RECENTLY_SEEN_MS: i64 = 10 * 60_000;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Yöneticinin tuttuğu ağ sayaçları (yalnızca zaman damgaları ve bayraklar; içerik yok).
#[derive(Default)]
pub struct NetStats {
    pub udp_bound: AtomicBool,
    pub mdns_up: AtomicBool,
    last_udp_ms: AtomicI64,
    last_mdns_ms: AtomicI64,
    last_tcp_in_ms: AtomicI64,
}

impl NetStats {
    /// Geçerli bir keşif (UDP kimliği ya da mDNS çözümlemesi) alındı.
    pub fn mark_discovery(&self, via: Via) {
        let slot = match via {
            Via::Udp => &self.last_udp_ms,
            Via::Mdns => &self.last_mdns_ms,
        };
        slot.store(now_ms(), Ordering::Relaxed);
    }

    /// Dışarıdan gelen bir TCP bağlantısı kabul edildi.
    pub fn mark_tcp_in(&self) {
        self.last_tcp_in_ms.store(now_ms(), Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> NetSnapshot {
        NetSnapshot {
            udp_bound: self.udp_bound.load(Ordering::Relaxed),
            mdns_up: self.mdns_up.load(Ordering::Relaxed),
            last_udp_ms: self.last_udp_ms.load(Ordering::Relaxed),
            last_mdns_ms: self.last_mdns_ms.load(Ordering::Relaxed),
            last_tcp_in_ms: self.last_tcp_in_ms.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NetSnapshot {
    pub udp_bound: bool,
    pub mdns_up: bool,
    /// 0 = hiç alınmadı
    pub last_udp_ms: i64,
    pub last_mdns_ms: i64,
    pub last_tcp_in_ms: i64,
}

impl NetSnapshot {
    fn last_inbound(&self) -> i64 {
        self.last_udp_ms
            .max(self.last_mdns_ms)
            .max(self.last_tcp_in_ms)
    }
    fn ever_inbound(&self) -> bool {
        self.last_inbound() > 0
    }
    fn recent_inbound(&self, now: i64) -> bool {
        self.ever_inbound() && now - self.last_inbound() <= RECENT_MS
    }
}

/// IPv4 adresinin ağ içindeki rolü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ipv4Class {
    /// 10/8, 172.16/12, 192.168/16: ev/ofis yerel ağı
    Private,
    /// 169.254/16: DHCP'den adres alınamamış
    LinkLocal,
    /// Diğer (ortak, CGNAT vb.)
    Other,
}

pub fn classify_ipv4(ip: Ipv4Addr) -> Ipv4Class {
    if ip.is_link_local() {
        Ipv4Class::LinkLocal
    } else if ip.is_private() {
        Ipv4Class::Private
    } else {
        Ipv4Class::Other
    }
}

/// Wi‑Fi/LAN denetimi için girdi.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WifiFacts {
    pub has_private_ipv4: bool,
    /// Link-local dışında herhangi bir IPv4 var mı
    pub has_usable_ipv4: bool,
    pub ssid: Option<String>,
}

impl WifiFacts {
    pub fn from_addrs(addrs: &[Ipv4Addr], ssid: Option<String>) -> WifiFacts {
        let classes: Vec<_> = addrs.iter().map(|a| classify_ipv4(*a)).collect();
        WifiFacts {
            has_private_ipv4: classes.contains(&Ipv4Class::Private),
            has_usable_ipv4: classes.iter().any(|c| *c != Ipv4Class::LinkLocal),
            ssid,
        }
    }
}

/// macOS "Yerel Ağ" izni.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalNet {
    /// Bu platformda izin gerekmez (Windows/Linux).
    NotApplicable,
    /// Yerel adrese gönderim açıkça reddedildi (EHOSTUNREACH / EPERM).
    Denied,
    /// Belirsiz: yöntem doğrudan sorgulama sunmuyor.
    Unknown,
}

/// Güvenlik duvarı kuralı durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Firewall {
    /// Duvar kapalı ya da bu platformda uygulanamaz.
    Open,
    /// Uygulama için açık "izin ver" kuralı var.
    Allowed,
    /// Uygulama için açık "engelle" kuralı var.
    Blocked,
    /// Duvar açık ama uygulamaya özel kural bulunamadı.
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DeviceFacts {
    /// Bağlantısı canlı herhangi bir cihaz (eşleşmiş ya da değil)
    pub any_reachable: bool,
    pub paired_total: usize,
    /// Eşleşmiş ve son 10 dakikada görülmüş ama şu an bağlı olmayan cihaz var mı
    pub paired_recently_seen_offline: bool,
}

impl DeviceFacts {
    /// `(eşleşmiş, bağlı, son görülme)` üçlülerinden olguları çıkarır.
    pub fn from_devices(devices: &[(bool, bool, Option<i64>)], now: i64) -> DeviceFacts {
        DeviceFacts {
            any_reachable: devices.iter().any(|d| d.1),
            paired_total: devices.iter().filter(|d| d.0).count(),
            paired_recently_seen_offline: devices
                .iter()
                .any(|d| d.0 && !d.1 && d.2.is_some_and(|t| now - t <= RECENTLY_SEEN_MS)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Inputs {
    pub wifi: WifiFacts,
    pub net: NetSnapshot,
    pub tcp_listening: bool,
    pub local_net: LocalNet,
    pub firewall: Firewall,
    pub devices: DeviceFacts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CheckResult {
    pub id: &'static str,
    pub status: Status,
    pub info: Option<String>,
}

fn check(id: &'static str, status: Status) -> CheckResult {
    CheckResult {
        id,
        status,
        info: None,
    }
}

/// Girdilerden 5 sonucu (sabit sırayla) üretir.
///
/// Sezgiler:
///  - wifi: özel IPv4 var → ok; yalnızca ortak/CGNAT IPv4 → warn; hiç kullanılabilir IPv4 yok → fail.
///  - localNetwork (macOS): işletim sistemi izni doğrudan sorgulatmaz. Yerel adrese UDP gönderimi
///    reddedildiyse fail; ağdan hiç paket gelmediyse/bağlantı yoksa belirsiz (warn); gelen keşif/TCP
///    ya da canlı bağlantı varsa izin verilmiş demektir (ok).
///  - firewall: dinleyici (TCP) açılamadıysa fail; "engelle" kuralı fail; duvar açık ve kural yok ise
///    içeriden gelen bir bağlantı görüldüyse ok, görülmediyse warn.
///  - discovery: ne UDP ne mDNS çalışıyorsa fail; son 2 dk'da keşif/gelen bağlantı ya da canlı cihaz
///    varsa ok; daha önce görüldüyse warn; hiç görülmediyse fail.
///  - phoneApp: canlı cihaz → ok; yakın zamanda görülen eşleşmiş cihaz şu an yok → warn;
///    eşleşmiş cihaz var ama ulaşılamıyor / hiç cihaz yok → fail.
pub fn assemble(i: &Inputs, now: i64) -> Vec<CheckResult> {
    let evidence = i.net.ever_inbound() || i.devices.any_reachable;

    let wifi = CheckResult {
        id: "wifi",
        status: if i.wifi.has_private_ipv4 {
            Status::Ok
        } else if i.wifi.has_usable_ipv4 {
            Status::Warn
        } else {
            Status::Fail
        },
        info: if i.wifi.has_private_ipv4 {
            i.wifi.ssid.clone()
        } else {
            None
        },
    };

    let local = check(
        "localNetwork",
        match i.local_net {
            LocalNet::NotApplicable => Status::Ok,
            LocalNet::Denied => Status::Fail,
            LocalNet::Unknown if evidence => Status::Ok,
            LocalNet::Unknown => Status::Warn,
        },
    );

    let firewall = check(
        "firewall",
        if !i.tcp_listening {
            Status::Fail
        } else {
            match i.firewall {
                Firewall::Blocked => Status::Fail,
                Firewall::Open | Firewall::Allowed => Status::Ok,
                Firewall::Unknown if i.net.last_tcp_in_ms > 0 || i.devices.any_reachable => {
                    Status::Ok
                }
                Firewall::Unknown => Status::Warn,
            }
        },
    );

    let discovery = check(
        "discovery",
        if !i.net.udp_bound && !i.net.mdns_up {
            Status::Fail
        } else if i.net.recent_inbound(now) || i.devices.any_reachable {
            Status::Ok
        } else if i.net.ever_inbound() {
            Status::Warn
        } else {
            Status::Fail
        },
    );

    let phone = check(
        "phoneApp",
        if i.devices.any_reachable {
            Status::Ok
        } else if i.devices.paired_recently_seen_offline {
            Status::Warn
        } else {
            Status::Fail
        },
    );

    vec![wifi, local, firewall, discovery, phone]
}

// ---------------------------------------------------------------------------------------------
// Platform çıktılarının saf ayrıştırıcıları (her platformda derlenir ve test edilir)
// ---------------------------------------------------------------------------------------------

/// `networksetup -getairportnetwork enX` çıktısından ağ adı.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn parse_networksetup_ssid(out: &str) -> Option<String> {
    let rest = out.trim().strip_prefix("Current Wi-Fi Network:")?;
    let name = rest.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// `netsh wlan show interfaces` çıktısından ağ adı (BSSID satırı değil).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn parse_netsh_ssid(out: &str) -> Option<String> {
    out.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        (k.trim() == "SSID" && !v.trim().is_empty()).then(|| v.trim().to_string())
    })
}

/// `socketfilterfw --getglobalstate`: Some(true) = duvar açık.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn parse_alf_global_state(out: &str) -> Option<bool> {
    if out.contains("State = 0") {
        Some(false)
    } else if out.contains("State = 1") || out.contains("State = 2") {
        Some(true)
    } else {
        None
    }
}

/// `socketfilterfw --listapps` çıktısında uygulamanın kuralı: Some(true) izin, Some(false) engel.
/// Listede uygulamanın paket yolu (`/Applications/Tatar.app`) bulunur; `exe` bunun altındadır.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn parse_alf_listapps(out: &str, exe: &str) -> Option<bool> {
    let mut lines = out.lines();
    while let Some(l) = lines.next() {
        let Some((_, path)) = l.split_once(" : ") else {
            continue;
        };
        let path = path.trim();
        if path.is_empty() || !exe.starts_with(path) {
            continue;
        }
        for next in lines.by_ref() {
            let n = next.trim();
            if n.is_empty() {
                continue;
            }
            return if n.contains("Block") {
                Some(false)
            } else if n.contains("Allow") {
                Some(true)
            } else {
                None
            };
        }
    }
    None
}

/// `netsh advfirewall firewall show rule name=all dir=in verbose` çıktısında bu program için
/// kural: Some(true) izin, Some(false) engel. Anahtar adları yerelleştirilmiş olabileceğinden
/// yalnızca program yolu ve "Allow"/"Block" değerlerine bakılır (başka dillerde None dönebilir).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn parse_netsh_rules(out: &str, exe: &str) -> Option<bool> {
    let exe = exe.to_lowercase();
    let mut allowed = None;
    for block in out.split("\n\n").flat_map(|b| b.split("\r\n\r\n")) {
        let mut mine = false;
        let mut action = None;
        let mut disabled = false;
        for l in block.lines() {
            let Some((k, v)) = l.split_once(':') else {
                continue;
            };
            let v = v.trim();
            if v.to_lowercase() == exe {
                mine = true;
            }
            match (k.trim(), v) {
                ("Enabled", "No") => disabled = true,
                ("Action", "Allow") => action = Some(true),
                ("Action", "Block") => action = Some(false),
                _ => {}
            }
        }
        if mine && !disabled {
            match action {
                // Engelleme kuralı her zaman baskın.
                Some(false) => return Some(false),
                Some(true) => allowed = Some(true),
                None => {}
            }
        }
    }
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> Ipv4Addr {
        s.parse().unwrap()
    }

    #[test]
    fn ipv4_classes() {
        assert_eq!(classify_ipv4(ip("192.168.1.5")), Ipv4Class::Private);
        assert_eq!(classify_ipv4(ip("10.0.0.2")), Ipv4Class::Private);
        assert_eq!(classify_ipv4(ip("172.16.4.1")), Ipv4Class::Private);
        assert_eq!(classify_ipv4(ip("172.32.0.1")), Ipv4Class::Other);
        assert_eq!(classify_ipv4(ip("169.254.9.9")), Ipv4Class::LinkLocal);
        assert_eq!(classify_ipv4(ip("100.64.0.1")), Ipv4Class::Other);
        assert_eq!(classify_ipv4(ip("8.8.8.8")), Ipv4Class::Other);
    }

    #[test]
    fn wifi_facts_from_addresses() {
        let f = WifiFacts::from_addrs(&[ip("169.254.1.1"), ip("192.168.0.4")], Some("Ev".into()));
        assert!(f.has_private_ipv4 && f.has_usable_ipv4);
        let f = WifiFacts::from_addrs(&[ip("169.254.1.1")], None);
        assert!(!f.has_private_ipv4 && !f.has_usable_ipv4);
        let f = WifiFacts::from_addrs(&[ip("100.70.0.3")], None);
        assert!(!f.has_private_ipv4 && f.has_usable_ipv4);
    }

    const NOW: i64 = 10_000_000;

    fn healthy() -> Inputs {
        Inputs {
            wifi: WifiFacts {
                has_private_ipv4: true,
                has_usable_ipv4: true,
                ssid: Some("Ev".into()),
            },
            net: NetSnapshot {
                udp_bound: true,
                mdns_up: true,
                last_udp_ms: NOW - 1000,
                last_mdns_ms: 0,
                last_tcp_in_ms: NOW - 500,
            },
            tcp_listening: true,
            local_net: LocalNet::Unknown,
            firewall: Firewall::Unknown,
            devices: DeviceFacts {
                any_reachable: true,
                paired_total: 1,
                paired_recently_seen_offline: false,
            },
        }
    }

    fn statuses(i: &Inputs) -> Vec<Status> {
        assemble(i, NOW).iter().map(|r| r.status).collect()
    }

    #[test]
    fn healthy_setup_passes_in_fixed_order() {
        let r = assemble(&healthy(), NOW);
        let ids: Vec<_> = r.iter().map(|x| x.id).collect();
        assert_eq!(
            ids,
            ["wifi", "localNetwork", "firewall", "discovery", "phoneApp"]
        );
        assert!(r.iter().all(|x| x.status == Status::Ok));
        assert_eq!(r[0].info.as_deref(), Some("Ev"));
    }

    #[test]
    fn no_network_fails_wifi_and_hides_ssid() {
        let mut i = healthy();
        i.wifi = WifiFacts::default();
        let r = assemble(&i, NOW);
        assert_eq!(r[0].status, Status::Fail);
        assert!(r[0].info.is_none());
        i.wifi.has_usable_ipv4 = true;
        assert_eq!(assemble(&i, NOW)[0].status, Status::Warn);
    }

    #[test]
    fn local_network_heuristics() {
        let mut i = healthy();
        i.local_net = LocalNet::Denied;
        assert_eq!(statuses(&i)[1], Status::Fail);
        // Hiç kanıt yok: belirsiz.
        let mut i = healthy();
        i.net = NetSnapshot {
            udp_bound: true,
            mdns_up: true,
            ..Default::default()
        };
        i.devices.any_reachable = false;
        assert_eq!(statuses(&i)[1], Status::Warn);
        i.local_net = LocalNet::NotApplicable;
        assert_eq!(statuses(&i)[1], Status::Ok);
    }

    #[test]
    fn firewall_rules() {
        let mut i = healthy();
        i.firewall = Firewall::Blocked;
        assert_eq!(statuses(&i)[2], Status::Fail);
        i.firewall = Firewall::Unknown;
        i.net.last_tcp_in_ms = 0;
        i.devices.any_reachable = false;
        assert_eq!(statuses(&i)[2], Status::Warn);
        i.firewall = Firewall::Allowed;
        assert_eq!(statuses(&i)[2], Status::Ok);
        i.tcp_listening = false;
        assert_eq!(statuses(&i)[2], Status::Fail);
    }

    #[test]
    fn discovery_levels() {
        let mut i = healthy();
        i.devices.any_reachable = false;
        assert_eq!(statuses(&i)[3], Status::Ok);
        i.net.last_udp_ms = NOW - RECENT_MS - 1;
        i.net.last_tcp_in_ms = 0;
        assert_eq!(statuses(&i)[3], Status::Warn);
        i.net.last_udp_ms = 0;
        assert_eq!(statuses(&i)[3], Status::Fail);
        i.net.udp_bound = false;
        i.net.mdns_up = false;
        i.devices.any_reachable = true;
        assert_eq!(statuses(&i)[3], Status::Fail);
    }

    #[test]
    fn phone_app_levels() {
        let mut i = healthy();
        i.devices.any_reachable = false;
        i.devices.paired_recently_seen_offline = true;
        assert_eq!(statuses(&i)[4], Status::Warn);
        i.devices.paired_recently_seen_offline = false;
        assert_eq!(statuses(&i)[4], Status::Fail);
    }

    #[test]
    fn device_facts_from_list() {
        let f =
            DeviceFacts::from_devices(&[(true, false, Some(NOW - 1000)), (false, true, None)], NOW);
        assert!(f.any_reachable && f.paired_recently_seen_offline);
        assert_eq!(f.paired_total, 1);
        let f = DeviceFacts::from_devices(&[(true, false, Some(NOW - RECENTLY_SEEN_MS - 1))], NOW);
        assert!(!f.any_reachable && !f.paired_recently_seen_offline);
        assert_eq!(DeviceFacts::from_devices(&[], NOW), DeviceFacts::default());
    }

    #[test]
    fn result_serialises_for_ui() {
        let v = serde_json::to_value(check("wifi", Status::Warn)).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"id": "wifi", "status": "warn", "info": null})
        );
    }

    #[test]
    fn ssid_parsers() {
        assert_eq!(
            parse_networksetup_ssid("Current Wi-Fi Network: Ev Ağı\n").as_deref(),
            Some("Ev Ağı")
        );
        assert!(
            parse_networksetup_ssid("You are not associated with an AirPort network.").is_none()
        );
        let netsh = "    Name                   : Wi-Fi\n    SSID                   : Ofis\n    BSSID                  : aa:bb\n";
        assert_eq!(parse_netsh_ssid(netsh).as_deref(), Some("Ofis"));
        assert!(parse_netsh_ssid("    BSSID : x").is_none());
    }

    #[test]
    fn alf_parsers() {
        assert_eq!(
            parse_alf_global_state("Firewall is enabled. (State = 1)"),
            Some(true)
        );
        assert_eq!(
            parse_alf_global_state("Firewall is disabled. (State = 0)"),
            Some(false)
        );
        assert_eq!(parse_alf_global_state("???"), None);
        let list = "ALF: total number of apps = 2 \n\n1 :  /Applications/Foo.app\n\t ( Allow incoming connections ) \n\n2 :  /Applications/Tatar.app\n\t ( Block incoming connections ) \n";
        let exe = "/Applications/Tatar.app/Contents/MacOS/tatar";
        assert_eq!(parse_alf_listapps(list, exe), Some(false));
        assert_eq!(
            parse_alf_listapps(list, "/Applications/Foo.app/Contents/MacOS/foo"),
            Some(true)
        );
        assert_eq!(parse_alf_listapps(list, "/Applications/Bar.app/x"), None);
    }

    #[test]
    fn netsh_rules() {
        let out = "Rule Name:                            Tatar\n----------\nEnabled:                              Yes\nDirection:                            In\nProgram:                              C:\\Program Files\\Tatar\\tatar.exe\nAction:                               Allow\n\nRule Name:                            Other\nEnabled:                              Yes\nProgram:                              C:\\x\\y.exe\nAction:                               Block\n";
        assert_eq!(
            parse_netsh_rules(out, "C:\\Program Files\\Tatar\\tatar.exe"),
            Some(true)
        );
        assert_eq!(parse_netsh_rules(out, "C:\\x\\y.exe"), Some(false));
        assert_eq!(parse_netsh_rules(out, "C:\\none.exe"), None);
        let disabled = out.replace(
            "Enabled:                              Yes\nDirection",
            "Enabled:                              No\nDirection",
        );
        assert_eq!(
            parse_netsh_rules(&disabled, "C:\\Program Files\\Tatar\\tatar.exe"),
            None
        );
    }
}
