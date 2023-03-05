use crate::client_handler::Protocol;
use crate::cursor::CustomCursor;
use crate::datatypes::PacketError;
use bytes::{Buf, BytesMut};
use serde::{Deserialize, Serialize};
use std::io::BufRead;
use tracing;

use crate::minecraft::{MinecraftDataPacket, MinecraftHelloPacket};
use crate::proxy::{ProxyClientJoinPacket, ProxyDataPacket, ProxyHelloPacket};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum SocketPacket {
    MCHelloPacket(MinecraftHelloPacket),
    MCDataPacket(MinecraftDataPacket),
    ProxyHelloPacket(ProxyHelloPacket),
    ProxyJoinPacket(ProxyClientJoinPacket),
    ProxyDataPacket(ProxyDataPacket),
    UnknownPacket,
}

impl From<MinecraftHelloPacket> for SocketPacket {
    fn from(packet: MinecraftHelloPacket) -> Self {
        SocketPacket::MCHelloPacket(packet)
    }
}

impl From<MinecraftDataPacket> for SocketPacket {
    fn from(packet: MinecraftDataPacket) -> Self {
        SocketPacket::MCDataPacket(packet)
    }
}

impl From<ProxyHelloPacket> for SocketPacket {
    fn from(packet: ProxyHelloPacket) -> Self {
        SocketPacket::ProxyHelloPacket(packet)
    }
}

impl From<ProxyClientJoinPacket> for SocketPacket {
    fn from(packet: ProxyClientJoinPacket) -> Self {
        SocketPacket::ProxyJoinPacket(packet)
    }
}

impl From<ProxyDataPacket> for SocketPacket {
    fn from(packet: ProxyDataPacket) -> Self {
        SocketPacket::ProxyDataPacket(packet)
    }
}

impl SocketPacket {
    pub fn new(buf: &mut BytesMut, first_pkg: bool) -> Result<SocketPacket, PacketError> {
        if first_pkg {
            MinecraftHelloPacket::new(buf).map(SocketPacket::MCHelloPacket)
        } else {
            MinecraftDataPacket::new(buf).map(SocketPacket::MCDataPacket)
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>, PacketError> {
        let packet = serde_json::to_string(self).map_err(|_| PacketError::NotValid)?;
        Ok(packet.as_bytes().to_vec())
    }
}

impl SocketPacket {
    pub fn decode_proxy(buf: &mut BytesMut) -> Result<SocketPacket, PacketError> {
        let mut cursor = CustomCursor::new(buf.to_vec());
        // create new empty string
        let mut line = String::new();
        // read a line into the string if there is one advance buffer if not return error
        if cursor.read_line(&mut line).is_err() {
            return Err(PacketError::TooSmall);
        }
        buf.advance(cursor.position() as usize);
        let packet: SocketPacket =
            serde_json::from_str(&line).map_err(|_| PacketError::NotValid)?;
        Ok(SocketPacket::from(packet))
    }
}

impl SocketPacket {
    pub fn parse_first_package(packet: &mut BytesMut) -> Result<SocketPacket, PacketError> {
        match MinecraftHelloPacket::new(packet) {
            Ok(pkg) => Ok(SocketPacket::from(pkg)),
            Err(PacketError::NotValid) => SocketPacket::decode_proxy(packet),
            Err(PacketError::NotMatching) => SocketPacket::decode_proxy(packet),
            Err(e) => Err(e),
        }
    }
    /// gigantic match statement to determine the packet type
    pub fn parse_packet(
        buf: &mut BytesMut,
        protocol: Protocol,
    ) -> Result<SocketPacket, PacketError> {
        match protocol {
            Protocol::MC(_) => MinecraftDataPacket::new(buf).map(SocketPacket::from),
            Protocol::Proxy(_) => SocketPacket::decode_proxy(buf),
            _ => {
                unimplemented!()
            }
        }
    }
}
