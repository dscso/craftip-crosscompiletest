use std::io::BufRead;
use crate::client_handler::Protocol;
use crate::datatypes::PacketError;
use bytes::{Buf, BytesMut};
use tracing;
use crate::cursor::CustomCursor;
use serde::{Deserialize, Serialize};
use crate::datatypes::PacketError::TooSmall;

use crate::minecraft::{MinecraftDataPacket, MinecraftHelloPacket};
use crate::packet_codec::PacketCodecError;
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
        let packet: SocketPacket = serde_json::from_str(&line).map_err(|_| PacketError::NotValid)?;
        Ok(SocketPacket::from(packet))
    }
}

impl SocketPacket {
    pub fn parse_first_package(
        packet: &mut BytesMut,
    ) -> Result<Option<SocketPacket>, PacketCodecError> {
        // check if it is MC packet
        let hello_packet = match MinecraftHelloPacket::new(packet) {
            Ok(pkg) => { Ok(SocketPacket::from(pkg)) }
            Err(PacketError::NotValid) => { SocketPacket::decode_proxy(packet) }
            Err(PacketError::NotMatching) => { SocketPacket::decode_proxy(packet) }
            Err(TooSmall) => { Err(TooSmall) }
            Err(e) => { Err(e) }
        };
        match hello_packet {
            Ok(hello_packet) => {
                return Ok(Some(SocketPacket::from(hello_packet)));
            }
            Err(PacketError::TooSmall) => { Ok(None) }
            Err(e) => return Err(PacketCodecError::from(e)),
        }
    }
    /// gigantic match statement to determine the packet type
    pub fn parse_packet(
        buf: &mut BytesMut,
        protocol: Protocol,
    ) -> Result<Option<SocketPacket>, PacketCodecError> {
        let result = match protocol {
            Protocol::MC(_) => MinecraftDataPacket::new(buf)
                .map(SocketPacket::from),
            Protocol::Proxy(_) => SocketPacket::decode_proxy(buf),
            _ => {
                unimplemented!()
            }
        };
        match result {
            Ok(packet) => Ok(packet).map(Some),
            Err(PacketError::TooSmall) => Ok(None),
            Err(e) => Err(e),
        }.map_err(PacketCodecError::from)
    }
}
