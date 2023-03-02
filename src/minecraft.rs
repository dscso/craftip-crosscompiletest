use bytes::Buf;
use std::mem::size_of;

use crate::cursor::{CustomCursor, CustomCursorMethods};
use crate::datatypes::PacketError;

const OLD_MINECRAFT_START: [u8; 27] = [
    0xFE, 0x01, 0xFA, 0x00, 0x0B, 0x00, 0x4D, 0x00, 0x43, 0x00, 0x7C, 0x00, 0x50, 0x00, 0x69, 0x00,
    0x6E, 0x00, 0x67, 0x00, 0x48, 0x00, 0x6F, 0x00, 0x73, 0x00, 0x74,
];

#[derive(Debug)]
pub enum MinecraftPacket {
    MCHelloPacket(MinecraftHelloPacket),
    MCDataPacket(MinecraftDataPacket),
}

impl MinecraftPacket {
    pub fn new(buf: Vec<u8>, first_pkg: bool) -> Result<MinecraftPacket, PacketError> {
        if first_pkg {
            MinecraftHelloPacket::new(buf)
                .map(MinecraftPacket::MCHelloPacket)
        } else {
            MinecraftDataPacket::new(buf)
                .map(MinecraftPacket::MCDataPacket)
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct MinecraftHelloPacket {
    pub length: usize,
    pub id: i32,
    pub version: i32,
    pub hostname: String,
    pub port: u32,
    pub raw: Vec<u8>,
}

#[derive(Debug)]
pub struct MinecraftDataPacket {
    pub length: usize,
    pub data: Vec<u8>,
}

impl MinecraftDataPacket {
    pub fn new(buf: Vec<u8>) -> Result<MinecraftDataPacket, PacketError> {
        let length = buf.len();
        if length < 1 {
            return Err(PacketError::NotValid);
        }
        Ok(MinecraftDataPacket { length, data: buf })
    }
}

impl MinecraftHelloPacket {
    pub fn new(buf: Vec<u8>) -> Result<MinecraftHelloPacket, PacketError> {
        let mut cursor = CustomCursor::new(buf);

        match MinecraftHelloPacket::old_ping_pkg(cursor.clone()) {
            Ok(pkg) => return Ok(pkg),
            Err(PacketError::NotMatching) => {}
            Err(e) => return Err(e),
        }
        match MinecraftHelloPacket::old_connect_pkg(cursor.clone()) {
            Ok(pkg) => return Ok(pkg),
            Err(PacketError::NotMatching) => {}
            Err(e) => return Err(e),
        }
        match MinecraftHelloPacket::new_pkg(cursor.clone()) {
            Ok(pkg) => return Ok(pkg),
            Err(PacketError::NotMatching) => {}
            Err(e) => return Err(e),
        }

        Err(PacketError::NotMatching)
    }

    fn old_ping_pkg(mut cursor: CustomCursor) -> Result<MinecraftHelloPacket, PacketError> {
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

        return Ok(MinecraftHelloPacket {
            length: cursor.position() as usize,
            id: 0,
            version: version as i32,
            port,
            hostname,
            raw: cursor.get_ref()[..cursor.position() as usize].to_vec(),
        });
    }
    fn old_connect_pkg(mut cursor: CustomCursor) -> Result<MinecraftHelloPacket, PacketError> {
        if !cursor.match_bytes(&[0x02]) {
            return Err(PacketError::NotMatching);
        }
        // todo test if this is really the version!
        let version = cursor.get_u8();
        // wait for the packet to fully arrive
        let _username = cursor.get_utf16_string()?;
        let hostname = cursor.get_utf16_string()?;
        cursor.throw_error_if_smaller(size_of::<u32>())?;
        let port = cursor.get_u32();

        return Ok(MinecraftHelloPacket {
            length: cursor.position() as usize,
            id: 0,
            version: version as i32,
            port,
            hostname,
            raw: cursor.get_ref()[..cursor.position() as usize].to_vec(),
        });
    }

    fn new_pkg(mut cursor: CustomCursor) -> Result<MinecraftHelloPacket, PacketError> {
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
            raw: cursor.get_ref()[..cursor.position() as usize].to_vec(),
        })
    }
}
