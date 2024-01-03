use serde::{Deserialize, Serialize};
use crate::crypto::{ChallengeDataType, SignatureDataType, ServerPublicKey};
use serde_big_array::BigArray;

use crate::minecraft::{MinecraftDataPacket, MinecraftHelloPacket};

/// ProxyHelloPacket is the first packet sent by the client to the proxy.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct ProxyHelloPacket {
    pub version: i32,
    pub hostname: String,
    pub auth: ProxyAuthenticator
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum ProxyAuthenticator {
    PublicKey(ServerPublicKey),
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum ProxyHandshakeResponse {
    ConnectionSuccessful(),
    Err(String),
}
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum ProxyAuthRequestPacket {
    #[serde(with = "BigArray")]
    PublicKey(ChallengeDataType),
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum ProxyAuthResponePacket {
    #[serde(with = "BigArray")]
    PublicKey(SignatureDataType),
}


#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct ProxyHelloResponsePacket {
    pub version: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct ProxyClientJoinPacket {
    pub client_id: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct ProxyClientDisconnectPacket {
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
    pub fn new(data: Vec<u8>, length: usize, client_id: u16) -> Self {
        Self {
            length,
            client_id,
            data,
        }
    }
}

impl ProxyDataPacket {
    pub fn from_mc_hello_packet(packet: &MinecraftHelloPacket, client_id: u16) -> Self {
        ProxyDataPacket {
            length: packet.length,
            client_id,
            data: packet.data.clone(),
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

/// ProxyClientJoinPacket constructor
impl ProxyClientJoinPacket {
    pub fn new(client_id: u16) -> Self {
        ProxyClientJoinPacket { client_id }
    }
}

/// ProxyClientDisconnectPacket constructor
impl ProxyClientDisconnectPacket {
    pub fn new(client_id: u16) -> Self {
        ProxyClientDisconnectPacket { client_id }
    }
}
