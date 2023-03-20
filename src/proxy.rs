use crate::minecraft::{MinecraftDataPacket, MinecraftHelloPacket};
use serde::{Deserialize, Serialize};

/// ProxyHelloPacket is the first packet sent by the client to the proxy.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct ProxyHelloPacket {
    pub length: usize,
    pub version: i32,
    pub hostname: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct ProxyClientJoinPacket {
    pub length: usize,
    pub client_id: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct ProxyClientDisconnectPacket {
    pub length: usize,
    pub client_id: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct ProxyDataPacket {
    pub length: usize,
    pub client_id: u16,
    pub data: Vec<u8>,
}

impl ProxyDataPacket {
    pub fn from_mc_packet(packet: MinecraftDataPacket, client_id: u16) -> Self {
        ProxyDataPacket {
            length: packet.length,
            client_id,
            data: packet.data,
        }
    }
}

impl ProxyDataPacket {
    pub fn from_mc_hello_packet(packet: MinecraftHelloPacket, client_id: u16) -> Self {
        ProxyDataPacket {
            length: packet.length,
            client_id,
            data: packet.data,
        }
    }
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
