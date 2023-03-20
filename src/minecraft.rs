use bytes::{Buf, BytesMut};
use serde::{Deserialize, Serialize};
use std::mem::size_of;

use crate::cursor::{CustomCursor, CustomCursorMethods};
use crate::datatypes::PacketError;
use crate::proxy::ProxyDataPacket;

const OLD_MINECRAFT_START: [u8; 27] = [
    0xFE, 0x01, 0xFA, 0x00, 0x0B, 0x00, 0x4D, 0x00, 0x43, 0x00, 0x7C, 0x00, 0x50, 0x00, 0x69, 0x00,
    0x6E, 0x00, 0x67, 0x00, 0x48, 0x00, 0x6F, 0x00, 0x73, 0x00, 0x74,
];

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, Clone)]
pub struct MinecraftHelloPacket {
    pub length: usize,
    pub id: i32,
    pub version: i32,
    pub hostname: String,
    pub port: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct MinecraftDataPacket {
    pub length: usize,
    pub data: Vec<u8>,
}

impl From<ProxyDataPacket> for MinecraftDataPacket {
    fn from(packet: ProxyDataPacket) -> Self {
        MinecraftDataPacket {
            length: packet.length,
            data: packet.data,
        }
    }
}

impl MinecraftDataPacket {
    pub fn new(buf: &mut BytesMut) -> Result<MinecraftDataPacket, PacketError> {
        Ok(MinecraftDataPacket {
            length: buf.len(),
            data: buf.split_to(buf.len()).to_vec(),
        })
    }
}

impl MinecraftHelloPacket {
    pub fn new(buf: &mut BytesMut) -> Result<MinecraftHelloPacket, PacketError> {
        match MinecraftHelloPacket::old_ping_pkg(buf) {
            Ok(pkg) => return Ok(pkg),
            Err(PacketError::NotMatching) => {}
            result => {
                return result;
            }
        }
        match MinecraftHelloPacket::old_connect_pkg(buf) {
            Ok(pkg) => return Ok(pkg),
            Err(PacketError::NotMatching) => {}
            result => {
                return result;
            }
        }
        match MinecraftHelloPacket::new_pkg(buf) {
            Ok(pkg) => return Ok(pkg),
            Err(PacketError::NotMatching) => {}
            result => {
                return result;
            }
        }

        Err(PacketError::NotMatching)
    }

    fn old_ping_pkg(buf: &mut BytesMut) -> Result<MinecraftHelloPacket, PacketError> {
        let mut cursor = CustomCursor::new(buf.to_vec());
        if !cursor.match_bytes(&[0xFE, 0x01]) {
            return Err(PacketError::NotMatching);
        }
        // wait for the packet to fully arrive
        cursor.throw_error_if_smaller(32)?;
        // check if the beginning is correct
        if !cursor.match_bytes(&OLD_MINECRAFT_START[cursor.position() as usize..]) {
            return Err(PacketError::NotValid);
        }
        // at pos 27 in buffer
        let rest_data = cursor.get_u16() as usize;
        let version = cursor.get_u8();
        // at pos 30
        let hostname = cursor.get_utf16_string()?;

        if 7 + hostname.len() * 2 != rest_data {
            return Err(PacketError::NotValid);
        }
        cursor.throw_error_if_smaller(size_of::<u32>())?;
        let port = cursor.get_u32();

        Ok(MinecraftHelloPacket {
            length: cursor.position() as usize,
            id: 0,
            version: version as i32,
            port,
            hostname,
            data: buf.split_to(cursor.position() as usize).to_vec(),
        })
    }
    fn old_connect_pkg(buf: &mut BytesMut) -> Result<MinecraftHelloPacket, PacketError> {
        let mut cursor = CustomCursor::new(buf.to_vec());
        if !cursor.match_bytes(&[0x02]) {
            return Err(PacketError::NotMatching);
        }
        // todo test if this is really the version!
        cursor.throw_error_if_smaller(size_of::<u8>())?;
        let version = cursor.get_u8();
        // wait for the packet to fully arrive
        let _username = cursor.get_utf16_string()?;
        let hostname = cursor.get_utf16_string()?;
        cursor.throw_error_if_smaller(size_of::<u32>())?;
        let port = cursor.get_u32();

        Ok(MinecraftHelloPacket {
            length: cursor.position() as usize,
            id: 0,
            version: version as i32,
            port,
            hostname,
            data: buf.split_to(cursor.position() as usize).to_vec(),
        })
    }

    fn new_pkg(buf: &mut BytesMut) -> Result<MinecraftHelloPacket, PacketError> {
        let mut cursor = CustomCursor::new(buf.to_vec());
        let pkg_length = cursor.get_varint()?;
        let pkg_id = cursor.get_varint()?;
        if pkg_id != 0 {
            return Err(PacketError::NotMatching);
        }
        let version = cursor.get_varint()?;
        let hostname = cursor.get_utf8_string()?;
        cursor.throw_error_if_smaller(size_of::<u16>())?;
        let port = cursor.get_u16();
        if cursor.position() as usize != pkg_length as usize {
            return Err(PacketError::NotValid);
        }

        Ok(MinecraftHelloPacket {
            length: cursor.position() as usize,
            id: pkg_id,
            port: port as u32,
            version,
            hostname,
            data: buf.split_to(cursor.position() as usize).to_vec(),
        })
    }
}
