use crate::minecraft::{MinecraftDataPacket, MinecraftHelloPacket};
use serde::{Deserialize, Serialize};

/// ProxyHelloPacket is the first packet sent by the client to the proxy.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProxyHelloPacket {
    pub length: usize,
    pub version: i32,
    pub hostname: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProxyClientJoinPacket {
    pub length: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProxyDataPacket {
    pub length: usize,
    pub client_id: u16,
    pub data: Vec<u8>,
}

impl From<MinecraftHelloPacket> for ProxyDataPacket {
    fn from(packet: MinecraftHelloPacket) -> Self {
        ProxyDataPacket {
            length: packet.length,
            client_id: 0,
            data: packet.data,
        }
    }
}

impl From<MinecraftDataPacket> for ProxyDataPacket {
    fn from(packet: MinecraftDataPacket) -> Self {
        ProxyDataPacket {
            length: packet.length,
            client_id: 0,
            data: packet.data,
        }
    }
}
