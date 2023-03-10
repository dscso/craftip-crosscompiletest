use crate::datatypes::Protocol;
use crate::cursor::{CustomCursor, CustomCursorMethods};
use crate::datatypes::PacketError;
use bytes::{Buf, BytesMut};
use serde::{Deserialize, Serialize};
use std::mem::size_of;
use std::io::{Cursor, Write};
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
        let mut cursor = CustomCursor::new(vec![]);
        let packet = bincode::serialize(self).map_err(|_| PacketError::EncodingError)?;
        let packet_length = packet.len() as u16;
        cursor.write_all(&packet_length.to_be_bytes()).expect("encoding error in write_all function");
        cursor.write_all(&packet).expect("encoding error in write_all function");
        Ok(cursor.get_ref()[..cursor.position() as usize].to_vec())
    }
}

impl SocketPacket {
    pub fn decode_proxy(buf: &mut BytesMut) -> Result<SocketPacket, PacketError> {
        let mut cursor = CustomCursor::new(buf.to_vec());
        cursor.throw_error_if_smaller(size_of::<u16>())?;
        let length = cursor.get_u16();
        cursor.throw_error_if_smaller(length as usize)?;
        let result = bincode::deserialize::<SocketPacket>(&cursor.get_ref()[cursor.position() as usize..cursor.position() as usize + length as usize])
            .map_err(|_| PacketError::NotValid)?;
        buf.advance(cursor.position() as usize + length as usize);
        // decode bincode packet
        return Ok(result);
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
        protocol: &Protocol,
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
