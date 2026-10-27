//! KDE Connect paketi: satır sonu ile ayrılmış JSON.
//! Ağdan gelen her şey güvenilmez veridir: boyut sınırı, sıkı doğrulama, hoşgörülü tür okuma.

use std::fmt;

use serde_json::{Map, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

/// Kimlik satırları için üst sınır (notlar §1.1).
pub const MAX_IDENTITY_LINE: usize = 8 * 1024;
/// Eşleşmemiş bir cihazdan gelen satırlar için üst sınır (yalnızca `kdeconnect.pair` kabul edilir).
pub const MAX_UNPAIRED_LINE: usize = 16 * 1024;
/// Eşleşmiş cihazlardan gelen normal paket satırı üst sınırı.
pub const MAX_PACKET_LINE: usize = 32 * 1024 * 1024;

pub const TYPE_IDENTITY: &str = "kdeconnect.identity";
pub const TYPE_PAIR: &str = "kdeconnect.pair";

#[derive(Debug, PartialEq, Eq)]
pub enum PacketError {
    TooLarge,
    Empty,
    NotJson,
    NotObject,
    MissingType,
    BadBody,
    BadPayloadSize,
}

impl fmt::Display for PacketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            PacketError::TooLarge => "paket çok büyük",
            PacketError::Empty => "boş paket",
            PacketError::NotJson => "geçersiz JSON",
            PacketError::NotObject => "paket bir nesne değil",
            PacketError::MissingType => "paket türü eksik",
            PacketError::BadBody => "paket gövdesi nesne değil",
            PacketError::BadPayloadSize => "geçersiz payloadSize",
        };
        f.write_str(s)
    }
}

impl std::error::Error for PacketError {}

#[derive(Debug, Clone, PartialEq)]
pub struct NetworkPacket {
    /// Gönderim anı, epoch ms. Alıcılar yok sayar.
    pub id: i64,
    pub packet_type: String,
    pub body: Value,
    pub payload_size: Option<i64>,
    pub payload_transfer_info: Option<Value>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Sayı ya da sayısal dize olarak yazılmış tamsayıyı okur (Android bazen dize gönderir).
pub fn lenient_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| {
            n.as_f64()
                .filter(|f| f.is_finite() && f.fract() == 0.0 && f.abs() < 9.0e15)
                .map(|f| f as i64)
        }),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// Bool ya da "true"/"false" dizesi olarak yazılmış değeri okur (ör. `isCancel`).
pub fn lenient_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

impl NetworkPacket {
    pub fn new(packet_type: &str, body: Value) -> Self {
        NetworkPacket {
            id: now_ms(),
            packet_type: packet_type.to_string(),
            body,
            payload_size: None,
            payload_transfer_info: None,
        }
    }

    /// Tek satırlık JSON (sonunda `\n`).
    pub fn serialize(&self) -> Vec<u8> {
        let mut m = Map::new();
        m.insert("id".into(), Value::from(self.id));
        m.insert("type".into(), Value::from(self.packet_type.clone()));
        m.insert("body".into(), self.body.clone());
        if let Some(sz) = self.payload_size {
            m.insert("payloadSize".into(), Value::from(sz));
        }
        if let Some(info) = &self.payload_transfer_info {
            m.insert("payloadTransferInfo".into(), info.clone());
        }
        // `Value` -> dize hiçbir zaman başarısız olmaz ve kompakt biçimde satır sonu içermez.
        let mut out = serde_json::to_vec(&Value::Object(m)).unwrap_or_default();
        out.push(b'\n');
        out
    }

    /// Bir satırı (sondaki `\n` olsa da olmasa da) çözer ve doğrular.
    pub fn parse(line: &[u8], max: usize) -> Result<NetworkPacket, PacketError> {
        let line = line.strip_suffix(b"\n").unwrap_or(line);
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.len() > max {
            return Err(PacketError::TooLarge);
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            return Err(PacketError::Empty);
        }
        let v: Value = serde_json::from_slice(line).map_err(|_| PacketError::NotJson)?;
        let Value::Object(mut obj) = v else {
            return Err(PacketError::NotObject);
        };
        let packet_type = match obj.remove("type") {
            Some(Value::String(s)) if !s.is_empty() && s.len() <= 128 => s,
            _ => return Err(PacketError::MissingType),
        };
        let id = obj.get("id").and_then(lenient_i64).unwrap_or(0);
        let body = match obj.remove("body") {
            None | Some(Value::Null) => Value::Object(Map::new()),
            Some(b @ Value::Object(_)) => b,
            Some(_) => return Err(PacketError::BadBody),
        };
        let payload_size = match obj.get("payloadSize") {
            None | Some(Value::Null) => None,
            Some(v) => Some(lenient_i64(v).ok_or(PacketError::BadPayloadSize)?),
        };
        if matches!(payload_size, Some(n) if n < -1) {
            return Err(PacketError::BadPayloadSize);
        }
        let payload_transfer_info = match obj.remove("payloadTransferInfo") {
            Some(v @ Value::Object(_)) => Some(v),
            _ => None,
        };
        Ok(NetworkPacket {
            id,
            packet_type,
            body,
            payload_size,
            payload_transfer_info,
        })
    }

    pub fn body_str(&self, key: &str) -> Option<&str> {
        self.body.get(key).and_then(Value::as_str)
    }
    pub fn body_i64(&self, key: &str) -> Option<i64> {
        self.body.get(key).and_then(lenient_i64)
    }
    pub fn body_bool(&self, key: &str) -> Option<bool> {
        self.body.get(key).and_then(lenient_bool)
    }
}

/// Üst sınırı aşmadan bir satır okur. `Ok(None)` = akış temiz kapandı.
/// Sınır aşılırsa `InvalidData` döner (bağlantı kesilmeli).
pub async fn read_line_limited<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    max: usize,
) -> std::io::Result<Option<Vec<u8>>> {
    let mut line: Vec<u8> = Vec::new();
    loop {
        let (done, used) = {
            let buf = reader.fill_buf().await?;
            if buf.is_empty() {
                return if line.is_empty() {
                    Ok(None)
                } else {
                    Err(std::io::ErrorKind::UnexpectedEof.into())
                };
            }
            match buf.iter().position(|&b| b == b'\n') {
                Some(i) => {
                    if line.len() + i > max {
                        return Err(std::io::ErrorKind::InvalidData.into());
                    }
                    line.extend_from_slice(&buf[..i]);
                    (true, i + 1)
                }
                None => {
                    if line.len() + buf.len() > max {
                        return Err(std::io::ErrorKind::InvalidData.into());
                    }
                    line.extend_from_slice(buf);
                    (false, buf.len())
                }
            }
        };
        reader.consume(used);
        if done {
            return Ok(Some(line));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roundtrip() {
        let mut p = NetworkPacket::new("kdeconnect.ping", json!({"message": "selam"}));
        p.payload_size = Some(42);
        p.payload_transfer_info = Some(json!({"port": 1739}));
        let bytes = p.serialize();
        assert_eq!(bytes.last(), Some(&b'\n'));
        assert_eq!(bytes.iter().filter(|&&b| b == b'\n').count(), 1);
        let q = NetworkPacket::parse(&bytes, MAX_PACKET_LINE).unwrap();
        assert_eq!(p, q);
    }

    #[test]
    fn id_is_number_and_no_nulls() {
        let p = NetworkPacket::new("kdeconnect.ping", json!({}));
        let v: Value = serde_json::from_slice(&p.serialize()).unwrap();
        assert!(v["id"].is_i64());
        assert!(v.get("payloadSize").is_none());
        assert!(v.get("payloadTransferInfo").is_none());
    }

    #[test]
    fn oversized_rejected() {
        let big = format!(
            r#"{{"id":1,"type":"kdeconnect.ping","body":{{"x":"{}"}}}}"#,
            "a".repeat(10_000)
        );
        assert_eq!(
            NetworkPacket::parse(big.as_bytes(), MAX_IDENTITY_LINE),
            Err(PacketError::TooLarge)
        );
        assert!(NetworkPacket::parse(big.as_bytes(), MAX_PACKET_LINE).is_ok());
    }

    #[test]
    fn malformed_rejected() {
        let lim = MAX_PACKET_LINE;
        assert_eq!(NetworkPacket::parse(b"", lim), Err(PacketError::Empty));
        assert_eq!(NetworkPacket::parse(b"  \n", lim), Err(PacketError::Empty));
        assert_eq!(
            NetworkPacket::parse(b"{nope", lim),
            Err(PacketError::NotJson)
        );
        assert_eq!(
            NetworkPacket::parse(b"[1,2]", lim),
            Err(PacketError::NotObject)
        );
        assert_eq!(
            NetworkPacket::parse(b"\"x\"", lim),
            Err(PacketError::NotObject)
        );
        assert_eq!(
            NetworkPacket::parse(b"{\"id\":1}", lim),
            Err(PacketError::MissingType)
        );
        assert_eq!(
            NetworkPacket::parse(b"{\"type\":\"\"}", lim),
            Err(PacketError::MissingType)
        );
        assert_eq!(
            NetworkPacket::parse(b"{\"type\":5}", lim),
            Err(PacketError::MissingType)
        );
        assert_eq!(
            NetworkPacket::parse(b"{\"type\":\"a\",\"body\":[1]}", lim),
            Err(PacketError::BadBody)
        );
        assert_eq!(
            NetworkPacket::parse(b"{\"type\":\"a\",\"payloadSize\":\"x\"}", lim),
            Err(PacketError::BadPayloadSize)
        );
        assert_eq!(
            NetworkPacket::parse(b"{\"type\":\"a\",\"payloadSize\":-5}", lim),
            Err(PacketError::BadPayloadSize)
        );
    }

    #[test]
    fn lenient_fields() {
        let p = NetworkPacket::parse(
            br#"{"id":"123","type":"kdeconnect.telephony","body":{"isCancel":"true","n":"7","m":8.0},"payloadSize":"10"}"#,
            MAX_PACKET_LINE,
        )
        .unwrap();
        assert_eq!(p.id, 123);
        assert_eq!(p.payload_size, Some(10));
        assert_eq!(p.body_bool("isCancel"), Some(true));
        assert_eq!(p.body_i64("n"), Some(7));
        assert_eq!(p.body_i64("m"), Some(8));
        assert_eq!(p.body_bool("n"), None);
    }

    #[test]
    fn missing_body_is_empty_object() {
        let p = NetworkPacket::parse(br#"{"type":"kdeconnect.ping"}"#, 1000).unwrap();
        assert_eq!(p.body, json!({}));
        assert_eq!(p.id, 0);
    }

    #[tokio::test]
    async fn line_reader_limits() {
        let data = b"abc\ndefghijkl\nlast";
        let mut r = tokio::io::BufReader::new(&data[..]);
        assert_eq!(
            read_line_limited(&mut r, 5).await.unwrap(),
            Some(b"abc".to_vec())
        );
        // ikinci satır 9 bayt > 5
        assert!(read_line_limited(&mut r, 5).await.is_err());
        let mut r = tokio::io::BufReader::new(&b"x\n"[..]);
        assert_eq!(
            read_line_limited(&mut r, 5).await.unwrap(),
            Some(b"x".to_vec())
        );
        assert_eq!(read_line_limited(&mut r, 5).await.unwrap(), None);
    }
}
