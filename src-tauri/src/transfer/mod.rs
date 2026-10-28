//! Dosya aktarımı: arayüze giden `Transfer` görünümü ve dosya adı güvenliği.
//! Aktarım motoru `plugins/share.rs` içindedir.

pub mod names;
pub mod safety;

use serde::Serialize;

/// Arayüzdeki `Transfer` tipi (sözleşme: src/lib/types.ts).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Transfer {
    pub id: String,
    pub device_id: String,
    pub name: String,
    /// Toplam bayt (bilinmiyorsa 0).
    pub size: u64,
    /// "send" | "receive"
    pub direction: &'static str,
    pub progress: u64,
    /// "active" | "done" | "failed" | "cancelled"
    pub state: &'static str,
    /// Başlangıç zamanı (epoch ms).
    pub when: i64,
    /// state == "failed" ise: "connectionLost" | "other".
    pub error: Option<&'static str>,
}
