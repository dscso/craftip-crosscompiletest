use std::io::{Cursor};
use std::mem::{size_of};
use bytes::{Buf, BytesMut};
use thiserror::Error;

const OLD_MINECRAFT_START: [u8; 27] = [
    0xFE, 0x01, 0xFA, 0x00, 0x0B, 0x00, 0x4D, 0x00, 0x43, 0x00, 0x7C, 0x00, 0x50, 0x00, 0x69, 0x00,
    0x6E, 0x00, 0x67, 0x00, 0x48, 0x00, 0x6F, 0x00, 0x73, 0x00, 0x74,
];

pub fn get_varint(buf: &[u8], start: usize) -> Result<(i32, usize), PacketError> {
    let mut value: i32 = 0;
    let mut position = 0;

    let mut size: usize = 0;

    loop {
        if size >= 5 {
            return Err(PacketError::NotValid);
        }
        if size + start >= buf.len() {
            return Err(PacketError::NotValid);
        }
        let current_byte = buf[size + start];

        value |= ((current_byte & 0x7F) as i32) << position;

        position += 7;
        size += 1;
        if (current_byte & 0x80) == 0 {
            return Ok((value, size));
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PacketError {
    #[error("Packet is too small, missing Bytes")]
    TooSmall,
    #[error("Packet is not valid")]
    NotValid,
    #[error("String encoding is not valid")]
    NotValidStringEncoding,
    #[error("Packet is not matching to decoder, do not recognize packet")]
    NotMatching,
}


#[derive(Debug, Clone)]
pub struct PacketFrame {
    pub length: usize,
    pub reader: usize,
    pub data: Vec<u8>,
}

pub type CustomCursor = Cursor<Vec<u8>>;

trait CustomCursorMethods {
    fn new(buf: Vec<u8>) -> Self;
    fn get_varint(&mut self) -> Result<i32, PacketError>;
    fn see_varint(&mut self, start: usize) -> Result<i32, PacketError>;
    fn get_utf8_string(&mut self) -> Result<String, PacketError>;
    fn throw_error_if_smaller(&mut self, size: usize) -> Result<(), PacketError>;
    fn get_utf16_string(&mut self) -> Result<String, PacketError>;
    fn match_bytes(&mut self, bytes: &[u8]) -> bool;
}

impl CustomCursorMethods for CustomCursor {
    fn new(buf: Vec<u8>) -> Self {
        Self::new(buf)
    }
    /// get the varint form buffer and advance cursor
    fn get_varint(&mut self) -> Result<i32, PacketError> {
        let (value, size) = get_varint(self.get_ref(), self.position() as usize)?;
        self.set_position(self.position() + size as u64);
        Ok(value)
    }
    /// just get the varint from the buffer without advancing the cursor
    fn see_varint(&mut self, start: usize) -> Result<i32, PacketError> {
        let (value, _) = get_varint(self.get_ref(), start)?;
        Ok(value)
    }
    /// Returns the string and the size of the string (including the size) in bytes
    fn get_utf8_string(&mut self) -> Result<String, PacketError> {
        let start = self.position() as usize;
        let string_len = self.see_varint(start)?;
        let size = string_len as usize;
        if self.get_ref().len() <= start + size {
            return Err(PacketError::TooSmall);
        }
        let result = String::from_utf8(self.get_ref()[start + 1..start + 1 + size].to_owned());
        match result {
            Ok(s) => {
                self.set_position(string_len as u64 + string_len as u64 + self.position());
                Ok(s)
            }
            Err(_) => Err(PacketError::NotValidStringEncoding),
        }
    }
    /// reads length and string from the buffer
    fn get_utf16_string(&mut self) -> Result<String, PacketError> {
        let courser_start = self.position();
        //assert!(2*size <= slice.len());
        self.throw_error_if_smaller(size_of::<u16>())?;
        let size = self.get_u16() as usize;
        if self.remaining() <= size * 2 {
            return Err(PacketError::TooSmall);
        }
        self.throw_error_if_smaller(size * 2)?;
        let iter = (0..size).map(|_| {
            self.get_u16()
        });

        let result = std::char::decode_utf16(iter).collect::<Result<String, _>>();

        match result {
            Ok(s) => {
                Ok(s)
            }
            Err(_) => {
                self.set_position(courser_start);
                Err(PacketError::NotValidStringEncoding)
            }
        }
    }
    fn throw_error_if_smaller(&mut self, size: usize) -> Result<(), PacketError> {
        if self.remaining() < size {
            return Err(PacketError::TooSmall);
        }
        Ok(())
    }
    /// matches the bytes in the buffer with the given bytes and if they match advance cursor
    fn match_bytes(&mut self, bytes: &[u8]) -> bool {
        if self.remaining() < bytes.len() {
            return false;
        }
        for (i, byte) in bytes.iter().enumerate() {
            if self.get_ref()[self.position() as usize + i] != *byte {
                return false;
            }
        }
        self.set_position(self.position() + bytes.len() as u64);
        return true;
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct MCHelloPacket {
    pub length: usize,
    pub id: i32,
    pub version: i32,
    pub hostname: String,
    pub port: u32,
}

impl MCHelloPacket {
    pub fn new(buf: Vec<u8>) -> Result<MCHelloPacket, PacketError> {
        let mut cursor = CustomCursor::new(buf);

        match MCHelloPacket::old_ping_pkg(cursor.clone()) {
            Ok(pkg) => return Ok(pkg),
            Err(PacketError::NotMatching) => {
                println!("Not matching old ping pkg")
            }
            Err(e) => return Err(e),
        }
        match MCHelloPacket::old_connect_pkg(cursor.clone()) {
            Ok(pkg) => return Ok(pkg),
            Err(PacketError::NotMatching) => {
                println!("Not matching old conn pkg")
            }
            Err(e) => return Err(e),
        }
        match MCHelloPacket::new_pkg(cursor.clone()) {
            Ok(pkg) => return Ok(pkg),
            Err(PacketError::NotMatching) => {}
            Err(e) => return Err(e),
        }

        Err(PacketError::NotMatching)
    }
    pub fn old_ping_pkg(mut cursor: CustomCursor) -> Result<MCHelloPacket, PacketError> {
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

        return Ok(MCHelloPacket {
            length: cursor.position() as usize,
            id: 0,
            version: version as i32,
            port,
            hostname,
        });
    }
    pub fn old_connect_pkg(mut cursor: CustomCursor) -> Result<MCHelloPacket, PacketError> {
        println!(" courser pos {:?}", String::from_utf8_lossy(cursor.get_ref()));
        if !cursor.match_bytes(&[0x02, 0x49]) {
            return Err(PacketError::NotMatching);
        }
        // wait for the packet to fully arrive
        let _username = cursor.get_utf16_string()?;
        let hostname = cursor.get_utf16_string()?;
        cursor.throw_error_if_smaller(size_of::<u32>())?;
        let port = cursor.get_u32();

        return Ok(MCHelloPacket {
            length: cursor.position() as usize,
            id: 0,
            version: 0,
            port,
            hostname,
        });
    }

    pub fn new_pkg(mut cursor: CustomCursor) -> Result<MCHelloPacket, PacketError> {
        let pkg_length = cursor.get_varint()?;
        let pkg_type = cursor.get_varint()?;
        if pkg_type != 0 {
            return Err(PacketError::NotMatching);
        }
        let version = cursor.get_varint()?;
        let hostname = cursor.get_utf8_string()?;
        cursor.throw_error_if_smaller(size_of::<u16>())?;
        let port = cursor.get_u16();
        Ok(MCHelloPacket {
            length: pkg_length as usize,
            id: pkg_type,
            port: port as u32,
            version: version,
            hostname,
        })
    }
}
