//! Kimlik paketi (`kdeconnect.identity`), cihaz kimliği ve ad temizleme.

use serde_json::{Map, Value, json};

use super::packet::{NetworkPacket, TYPE_IDENTITY, lenient_i64};

pub const PROTOCOL_VERSION: i64 = 8;
pub const MIN_TCP_PORT: u16 = 1716;
pub const MAX_TCP_PORT: u16 = 1764;
pub const MAX_NAME_UNITS: usize = 32;
const MAX_CAPABILITIES: usize = 256;
const MAX_CAPABILITY_LEN: usize = 128;

/// Telefonların `incomingCapabilities` listesinden ayıklanıp tutulacak türler için
/// merkezi liste: v1'de Tatar'ın SUNMAYI planladığı yetenekler (notlar §8).
/// Gerçekte ilan edilen liste, eklenti kayıt defterinden gelir (`plugins::Registry`);
/// her eklenti aşaması kendi türlerini orada kaydeder. Bu sabit, aşamalar bitince
/// kayıt defterinin neyi toplaması gerektiğini gösteren tek referanstır.
pub const PLANNED_INCOMING: &[&str] = &[
    "kdeconnect.ping",
    "kdeconnect.battery",
    "kdeconnect.clipboard",
    "kdeconnect.clipboard.connect",
    "kdeconnect.share.request",
    "kdeconnect.share.request.update",
    "kdeconnect.telephony",
];
pub const PLANNED_OUTGOING: &[&str] = &[
    "kdeconnect.ping",
    "kdeconnect.findmyphone.request",
    "kdeconnect.clipboard",
    "kdeconnect.clipboard.connect",
    "kdeconnect.share.request",
    "kdeconnect.share.request.update",
    "kdeconnect.battery.request",
];

/// Cihaz kimliği: `^[a-zA-Z0-9_-]{32,38}$`.
pub fn is_valid_device_id(id: &str) -> bool {
    (32..=38).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// 32 küçük harfli onaltılık karakter (UUID, tire yok).
pub fn generate_device_id() -> String {
    let bytes: [u8; 16] = rand::random();
    hex::encode(bytes)
}

/// Ağdan gelen/ağa giden ad filtresi: `" ' , ; : . ! ? ( ) [ ] < >` silinir,
/// kenar boşlukları atılır, 32 UTF-16 birimine kısaltılır. Kontrol karakterleri de atılır.
pub fn filter_device_name(name: &str) -> String {
    const BAD: &str = "\"',;:.!?()[]<>";
    let cleaned: String = name
        .chars()
        .filter(|c| !BAD.contains(*c) && !c.is_control())
        .collect();
    let mut out = String::new();
    let mut units = 0;
    for c in cleaned.trim().chars() {
        units += c.len_utf16();
        if units > MAX_NAME_UNITS {
            break;
        }
        out.push(c);
    }
    out.trim().to_string()
}

/// "Tatar / <bilgisayar adı>" biçiminde varsayılan ad.
pub fn default_device_name() -> String {
    let host = gethostname::gethostname().to_string_lossy().to_string();
    let host = host.split('.').next().unwrap_or("").trim().to_string();
    let host = filter_device_name(&host);
    let host: String = if host.is_empty() {
        "Bilgisayar".into()
    } else {
        host
    };
    let name = filter_device_name(&format!("Tatar / {host}"));
    if name.is_empty() {
        "Tatar".into()
    } else {
        name
    }
}

pub fn default_device_type() -> &'static str {
    if cfg!(target_os = "macos") {
        "laptop"
    } else {
        "desktop"
    }
}

pub fn normalize_device_type(t: &str) -> &'static str {
    match t {
        "desktop" => "desktop",
        "laptop" => "laptop",
        "phone" => "phone",
        "tablet" => "tablet",
        "tv" => "tv",
        _ => "desktop",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub device_id: String,
    pub device_name: String,
    pub device_type: String,
    pub protocol_version: i64,
    pub incoming: Vec<String>,
    pub outgoing: Vec<String>,
    pub tcp_port: Option<u16>,
    pub target_device_id: Option<String>,
    pub target_protocol_version: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum IdentityError {
    WrongType,
    BadDeviceId,
    BadName,
    BadProtocolVersion,
}

fn string_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= MAX_CAPABILITY_LEN)
            .take(MAX_CAPABILITIES)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

impl Identity {
    /// Paketten çözer ve doğrular (tür, deviceId, filtrelenmiş ad boş değil, protokol sürümü).
    pub fn from_packet(p: &NetworkPacket) -> Result<Identity, IdentityError> {
        if p.packet_type != TYPE_IDENTITY {
            return Err(IdentityError::WrongType);
        }
        let device_id = p.body_str("deviceId").unwrap_or("");
        if !is_valid_device_id(device_id) {
            return Err(IdentityError::BadDeviceId);
        }
        let device_name = filter_device_name(p.body_str("deviceName").unwrap_or(""));
        if device_name.is_empty() {
            return Err(IdentityError::BadName);
        }
        // Eksik protokol sürümü geçersizdir (eski sürümleri zaten istemiyoruz).
        let protocol_version = p
            .body
            .get("protocolVersion")
            .and_then(lenient_i64)
            .ok_or(IdentityError::BadProtocolVersion)?;
        let tcp_port = p
            .body_i64("tcpPort")
            .and_then(|n| u16::try_from(n).ok())
            .filter(|n| (MIN_TCP_PORT..=MAX_TCP_PORT).contains(n));
        Ok(Identity {
            device_id: device_id.to_string(),
            device_name,
            device_type: normalize_device_type(p.body_str("deviceType").unwrap_or("")).to_string(),
            protocol_version,
            incoming: string_list(p.body.get("incomingCapabilities")),
            outgoing: string_list(p.body.get("outgoingCapabilities")),
            tcp_port,
            target_device_id: p.body_str("targetDeviceId").map(str::to_string),
            target_protocol_version: p.body.get("targetProtocolVersion").and_then(lenient_i64),
        })
    }

    pub fn supports_incoming(&self, packet_type: &str) -> bool {
        self.incoming.iter().any(|t| t == packet_type)
    }

    fn base_body(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("deviceId".into(), json!(self.device_id));
        m.insert("deviceName".into(), json!(self.device_name));
        m.insert("deviceType".into(), json!(self.device_type));
        m.insert("protocolVersion".into(), json!(self.protocol_version));
        m
    }

    /// TLS içinde gönderilen tam kimlik (tcpPort ve target* yok).
    pub fn to_secure_packet(&self) -> NetworkPacket {
        let mut m = self.base_body();
        m.insert("incomingCapabilities".into(), json!(self.incoming));
        m.insert("outgoingCapabilities".into(), json!(self.outgoing));
        NetworkPacket::new(TYPE_IDENTITY, Value::Object(m))
    }

    /// UDP yayını: küçük tutulur (parçalanma yok); `deviceName` Android için zorunlu.
    pub fn to_udp_packet(&self, tcp_port: u16) -> NetworkPacket {
        let mut m = self.base_body();
        m.insert("tcpPort".into(), json!(tcp_port));
        NetworkPacket::new(TYPE_IDENTITY, Value::Object(m))
    }

    /// Düz TCP kimliği: bağlantıyı AÇAN taraf gönderir. `targetDeviceId` (biliniyorsa)
    /// ve `targetProtocolVersion` (karşıdan görülen sürüm) KDE için gereklidir.
    pub fn to_plain_tcp_packet(
        &self,
        target_device_id: Option<&str>,
        target_protocol_version: i64,
    ) -> NetworkPacket {
        let mut m = self.base_body();
        m.insert("incomingCapabilities".into(), json!(self.incoming));
        m.insert("outgoingCapabilities".into(), json!(self.outgoing));
        if let Some(t) = target_device_id {
            m.insert("targetDeviceId".into(), json!(t));
        }
        m.insert(
            "targetProtocolVersion".into(),
            json!(target_protocol_version),
        );
        NetworkPacket::new(TYPE_IDENTITY, Value::Object(m))
    }
}

/// CVE-2025-66270 savunması: TLS sonrası kimlik, TLS öncesi görülenle ve sertifika CN'iyle
/// birebir uyuşmak zorunda. Saf fonksiyon; tüm hata durumlarında bağlantı kesilmeli.
#[derive(Debug, PartialEq, Eq)]
pub enum MismatchError {
    DeviceId,
    ProtocolVersion,
    CertCommonName,
    CertMissing,
}

pub fn verify_secure_identity(
    pre_device_id: Option<&str>,
    pre_protocol_version: i64,
    secure: &Identity,
    cert_common_name: Option<&str>,
) -> Result<(), MismatchError> {
    if let Some(pre) = pre_device_id
        && pre != secure.device_id
    {
        return Err(MismatchError::DeviceId);
    }
    if pre_protocol_version != secure.protocol_version {
        return Err(MismatchError::ProtocolVersion);
    }
    match cert_common_name {
        None => Err(MismatchError::CertMissing),
        Some(cn) if cn != secure.device_id => Err(MismatchError::CertCommonName),
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(id: &str) -> Identity {
        Identity {
            device_id: id.into(),
            device_name: "Telefon".into(),
            device_type: "phone".into(),
            protocol_version: 8,
            incoming: vec![],
            outgoing: vec![],
            tcp_port: None,
            target_device_id: None,
            target_protocol_version: None,
        }
    }

    #[test]
    fn device_id_validation() {
        assert!(is_valid_device_id(&"a".repeat(32)));
        assert!(is_valid_device_id(&"a".repeat(38)));
        assert!(is_valid_device_id("ee061a75_e403_4ecc_9261_5ffe2722f698"));
        assert!(is_valid_device_id("ee061a75-e403-4ecc-9261-5ffe2722f698"));
        assert!(!is_valid_device_id(&"a".repeat(31)));
        assert!(!is_valid_device_id(&"a".repeat(39)));
        assert!(!is_valid_device_id(&format!("{}/", "a".repeat(31))));
        assert!(!is_valid_device_id(&format!("{}\n", "a".repeat(31))));
        assert!(!is_valid_device_id(&format!("{}ü", "a".repeat(31))));
        assert!(!is_valid_device_id(""));
    }

    #[test]
    fn generated_id_format() {
        let id = generate_device_id();
        assert_eq!(id.len(), 32);
        assert!(
            id.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert!(is_valid_device_id(&id));
        assert_ne!(id, generate_device_id());
    }

    #[test]
    fn name_filter() {
        assert_eq!(filter_device_name("Tatar / MacBook"), "Tatar / MacBook");
        assert_eq!(
            filter_device_name("A\"B'C,D;E:F.G!H?I(J)K[L]M<N>O"),
            "ABCDEFGHIJKLMNO"
        );
        assert_eq!(filter_device_name("  boşluk  "), "boşluk");
        assert_eq!(filter_device_name(".,;"), "");
        assert_eq!(filter_device_name("a\nb\u{0}c"), "abc");
        assert_eq!(filter_device_name(&"x".repeat(100)).len(), 32);
        // surrogat çifti ortadan bölünmez
        let long = "😀".repeat(20);
        let f = filter_device_name(&long);
        assert_eq!(f.chars().count(), 16);
    }

    #[test]
    fn default_name_is_valid() {
        let n = default_device_name();
        assert!(!n.is_empty());
        assert!(n.encode_utf16().count() <= 32);
        assert_eq!(filter_device_name(&n), n);
    }

    #[test]
    fn identity_from_packet() {
        let p = NetworkPacket::new(
            TYPE_IDENTITY,
            json!({
                "deviceId": "ee061a75_e403_4ecc_9261_5ffe2722f698",
                "deviceName": "Pixel (7)",
                "deviceType": "phone",
                "protocolVersion": 8,
                "tcpPort": 1716,
                "targetProtocolVersion": "8",
                "incomingCapabilities": ["kdeconnect.ping", 3, ""],
                "outgoingCapabilities": ["kdeconnect.battery"],
            }),
        );
        let i = Identity::from_packet(&p).unwrap();
        assert_eq!(i.device_name, "Pixel 7");
        assert_eq!(i.tcp_port, Some(1716));
        assert_eq!(i.target_protocol_version, Some(8));
        assert_eq!(i.incoming, vec!["kdeconnect.ping"]);
        assert!(i.supports_incoming("kdeconnect.ping"));
    }

    #[test]
    fn identity_rejections() {
        let good = json!({"deviceId": "a".repeat(32), "deviceName": "x", "protocolVersion": 8});
        assert!(Identity::from_packet(&NetworkPacket::new(TYPE_IDENTITY, good.clone())).is_ok());
        assert_eq!(
            Identity::from_packet(&NetworkPacket::new("kdeconnect.pair", good.clone())),
            Err(IdentityError::WrongType)
        );
        let mut b = good.clone();
        b["deviceId"] = json!("kisa");
        assert_eq!(
            Identity::from_packet(&NetworkPacket::new(TYPE_IDENTITY, b)),
            Err(IdentityError::BadDeviceId)
        );
        let mut b = good.clone();
        b["deviceName"] = json!("...");
        assert_eq!(
            Identity::from_packet(&NetworkPacket::new(TYPE_IDENTITY, b)),
            Err(IdentityError::BadName)
        );
        let mut b = good.clone();
        b.as_object_mut().unwrap().remove("protocolVersion");
        assert_eq!(
            Identity::from_packet(&NetworkPacket::new(TYPE_IDENTITY, b)),
            Err(IdentityError::BadProtocolVersion)
        );
        // geçersiz tcpPort yok sayılır
        let mut b = good;
        b["tcpPort"] = json!(80);
        assert_eq!(
            Identity::from_packet(&NetworkPacket::new(TYPE_IDENTITY, b))
                .unwrap()
                .tcp_port,
            None
        );
    }

    #[test]
    fn identity_packet_variants() {
        let mut me = ident(&"b".repeat(32));
        me.incoming = vec!["kdeconnect.ping".into()];
        let udp = me.to_udp_packet(1716);
        assert_eq!(udp.body["tcpPort"], 1716);
        assert!(udp.body.get("incomingCapabilities").is_none());
        let plain = me.to_plain_tcp_packet(Some(&"c".repeat(32)), 8);
        assert_eq!(plain.body["targetDeviceId"], "c".repeat(32));
        assert_eq!(plain.body["targetProtocolVersion"], 8);
        let sec = me.to_secure_packet();
        assert!(sec.body.get("targetDeviceId").is_none());
        assert!(sec.body.get("tcpPort").is_none());
        assert_eq!(sec.body["incomingCapabilities"][0], "kdeconnect.ping");
        assert!(Identity::from_packet(&sec).is_ok());
    }

    #[test]
    fn cve_2025_66270_checks() {
        let id = "a".repeat(32);
        let other = "b".repeat(32);
        let s = ident(&id);
        assert_eq!(verify_secure_identity(Some(&id), 8, &s, Some(&id)), Ok(()));
        assert_eq!(verify_secure_identity(None, 8, &s, Some(&id)), Ok(()));
        assert_eq!(
            verify_secure_identity(Some(&other), 8, &s, Some(&id)),
            Err(MismatchError::DeviceId)
        );
        assert_eq!(
            verify_secure_identity(Some(&id), 7, &s, Some(&id)),
            Err(MismatchError::ProtocolVersion)
        );
        assert_eq!(
            verify_secure_identity(Some(&id), 8, &s, Some(&other)),
            Err(MismatchError::CertCommonName)
        );
        assert_eq!(
            verify_secure_identity(Some(&id), 8, &s, None),
            Err(MismatchError::CertMissing)
        );
    }

    #[test]
    fn planned_capabilities_match_notes() {
        assert_eq!(PLANNED_INCOMING.len(), 7);
        assert_eq!(PLANNED_OUTGOING.len(), 7);
        assert!(!PLANNED_INCOMING.contains(&"kdeconnect.findmyphone.request"));
    }
}
