//! `kdeconnect.findmyphone.request` (yalnızca giden): telefonu çaldırır.

use super::{DeviceHandle, Plugin};
use crate::protocol::packet::NetworkPacket;

pub const TYPE_FINDMYPHONE: &str = "kdeconnect.findmyphone.request";

pub struct FindMyPhonePlugin;

pub fn ring_packet() -> NetworkPacket {
    NetworkPacket::new(TYPE_FINDMYPHONE, serde_json::json!({}))
}

impl Plugin for FindMyPhonePlugin {
    fn name(&self) -> &'static str {
        "findmyphone"
    }
    fn incoming_types(&self) -> &'static [&'static str] {
        &[]
    }
    fn outgoing_types(&self) -> &'static [&'static str] {
        &[TYPE_FINDMYPHONE]
    }
    fn handle(&self, _device: &DeviceHandle, _packet: &NetworkPacket) {}
}
