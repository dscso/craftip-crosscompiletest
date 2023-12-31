use std::io::Write;
use std::mem::size_of;

use bytes::{Buf, BytesMut};
use serde::{Deserialize, Serialize};

use crate::cursor::{CustomCursor, CustomCursorMethods};
use crate::datatypes::PacketError;
use crate::datatypes::Protocol;
use crate::minecraft::{MinecraftDataPacket, MinecraftHelloPacket};
use crate::proxy::{
    ProxyClientDisconnectPacket, ProxyClientJoinPacket, ProxyDataPacket, ProxyHelloPacket,
    ProxyHelloResponsePacket,
};

pub type PingPacket = u16;

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum SocketPacket {
    MCHello(MinecraftHelloPacket),
    MCData(MinecraftDataPacket),
    ProxyHello(ProxyHelloPacket),
    ProxyHelloResponse(ProxyHelloResponsePacket),
    ProxyJoin(ProxyClientJoinPacket),
    ProxyDisconnect(ProxyClientDisconnectPacket),
    ProxyData(ProxyDataPacket),
    ProxyPing(PingPacket),
    ProxyPong(PingPacket),
    Unknown,
}

impl From<MinecraftHelloPacket> for SocketPacket {
    fn from(packet: MinecraftHelloPacket) -> Self {
        SocketPacket::MCHello(packet)
    }
}

impl From<MinecraftDataPacket> for SocketPacket {
    fn from(packet: MinecraftDataPacket) -> Self {
        SocketPacket::MCData(packet)
    }
}

impl From<ProxyHelloPacket> for SocketPacket {
    fn from(packet: ProxyHelloPacket) -> Self {
        SocketPacket::ProxyHello(packet)
    }
}
impl From<ProxyHelloResponsePacket> for SocketPacket {
    fn from(packet: ProxyHelloResponsePacket) -> Self {
        SocketPacket::ProxyHelloResponse(packet)
    }
}

impl From<ProxyClientJoinPacket> for SocketPacket {
    fn from(packet: ProxyClientJoinPacket) -> Self {
        SocketPacket::ProxyJoin(packet)
    }
}

impl From<ProxyClientDisconnectPacket> for SocketPacket {
    fn from(packet: ProxyClientDisconnectPacket) -> Self {
        SocketPacket::ProxyDisconnect(packet)
    }
}

impl From<ProxyDataPacket> for SocketPacket {
    fn from(packet: ProxyDataPacket) -> Self {
        SocketPacket::ProxyData(packet)
    }
}

impl SocketPacket {
    pub fn encode(&self) -> Result<Vec<u8>, PacketError> {
        let mut cursor = CustomCursor::new(vec![]);
        let packet = bincode::serialize(self).map_err(|_| PacketError::EncodingError)?;
        let packet_length = packet.len() as u16;
        cursor
            .write_all(&packet_length.to_be_bytes())
            .expect("encoding error in write_all function");
        cursor
            .write_all(&packet)
            .expect("encoding error in write_all function");
        Ok(cursor.get_ref()[..cursor.position() as usize].to_vec())
    }
}

impl SocketPacket {
    pub fn decode_proxy(buf: &mut BytesMut) -> Result<SocketPacket, PacketError> {
        let mut cursor = CustomCursor::new(buf.to_vec());
        cursor.throw_error_if_smaller(size_of::<u16>())?;
        let length = cursor.get_u16();
        cursor.throw_error_if_smaller(length as usize)?;
        let result = bincode::deserialize::<SocketPacket>(
            &cursor.get_ref()
                [cursor.position() as usize..cursor.position() as usize + length as usize],
        )
        .map_err(|_| PacketError::NotValid)?;
        buf.advance(cursor.position() as usize + length as usize);
        // decode bincode packet
        Ok(result)
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

/// Custom packet type for tokio channels to be able to close the client socket by the proxy
/// uses Packet type as a generic type
/// or Close to close the socket
#[derive(Debug, PartialEq)]
pub enum ChannelMessage<T> {
    Packet(T),
    Close,
}
