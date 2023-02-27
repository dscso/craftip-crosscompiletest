use std::error::Error;
use thiserror::Error;

const OLD_MINECRAFT_START: [u8; 27] = [
    0xFE, 0x01, 0xFA, 0x00, 0x0B, 0x00, 0x4D, 0x00, 0x43, 0x00, 0x7C, 0x00, 0x50, 0x00, 0x69, 0x00,
    0x6E, 0x00, 0x67, 0x00, 0x48, 0x00, 0x6F, 0x00, 0x73, 0x00, 0x74,
];

#[derive(Debug, Error)]
pub enum VarIntError {
    #[error("VarInt is too big")]
    TooBig,
    #[error("VarInt is too small")]
    TooSmall,
    #[error("VarInt is not valid")]
    NotValid,
}



#[derive(Debug, PartialEq, Eq)]
pub struct VarInt {
    pub value: i32,
    pub size: usize,
}

impl VarInt {
    pub fn new(buf: &[u8], start: usize) -> Result<VarInt, VarIntError> {
        let mut value: i32 = 0;
        let mut position = 0;

        let mut size: usize = 0;

        for i in 0..4 {
            if i + start >= buf.len() {
                return Err(VarIntError::NotValid);
            }
            size += 1;
            let current_byte = buf[i + start];
            value |= ((current_byte & 0x7F) << position) as i32;

            if (current_byte & 0x80) == 0 {
                break;
            }
            position += 7;
        }

        Ok(VarInt { value, size })
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
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct HelloPacket {
    pub length: usize,
    pub id: i32,
    pub version: i32,
    pub hostname: String,
    pub port: u32,
}
#[derive(Debug, Clone)]
pub struct Packet {
    pub length: usize,
    pub data: Vec<u8>,
}

impl Packet {
    pub fn new() -> Packet {
        Packet {
            length: 0,
            data: Vec::new(),
        }
    }
    /// appends data to the buffer
    pub fn add_data(&mut self, data: &[u8], size: usize) {
        self.length += size;
        self.data.extend_from_slice(data[..size].as_ref());
    }
    /// gets a varint from buffer and returns the value and the size of the varint
    pub fn get_varint(&self, start: usize) -> Result<VarInt, PacketError> {
        return VarInt::new(&self.data, start).map_err(|_| PacketError::TooSmall);
    }
    /// assembles a u16
    pub fn get_u16(&self, start: usize) -> Option<u16> {
        if self.data.len() <= start + 1 {
            return None;
        }
        Some(u16::from_be_bytes([self.data[start], self.data[start + 1]]))
    }
    /// assembles a u32 from 4 bytes
    pub fn get_u32(&self, start: usize) -> Option<u32> {
        if self.data.len() <= start + 3 {
            return None;
        }
        Some(u32::from_be_bytes([
            self.data[start],
            self.data[start + 1],
            self.data[start + 2],
            self.data[start + 3],
        ]))
    }
    /// reads length and string from the buffer
    pub fn get_utf16_string(&self, start: usize) -> Result<String, PacketError> {
        //assert!(2*size <= slice.len());
        let size = self.get_u16(start).ok_or(PacketError::TooSmall)? as usize;
        if self.data.len() <= start + 2 + size * 2 {
            return Err(PacketError::TooSmall);
        }
        let iter = (0..size).map(|i| {
            u16::from_be_bytes([
                self.data[(start + 2) + 2 * i],
                self.data[(start + 2) + 2 * i + 1],
            ])
        });

        let result = std::char::decode_utf16(iter).collect::<Result<String, _>>();

        match result {
            Ok(s) => Ok(s),
            Err(_) => Err(PacketError::NotValidStringEncoding),
        }
    }
    /// Returns the string and the size of the string (including the size) in bytes
    pub fn get_utf8_string(&self, start: usize) -> Result<(String, usize), PacketError> {
        let string_len = self.get_varint(start)?;
        let size = string_len.value as usize;
        if self.data.len() <= start + size {
            return Err(PacketError::TooSmall);
        }
        let result = String::from_utf8(self.data[start + 1..start + 1 + size].to_vec());
        match result {
            Ok(s) => Ok((s, string_len.value as usize + string_len.size)),
            Err(_) => Err(PacketError::NotValidStringEncoding),
        }
    }
    /// get option on a byte of the buffer
    pub fn get_byte(&self, index: usize) -> Option<u8> {
        if index >= self.data.len() {
            return None;
        }
        Some(self.data[index])
    }
    /// remove the bytes that are already processed/send
    pub fn flush_packet(&mut self, size: usize) {
        if self.length < size {
            panic!("flushing more than available");
        }
        self.length -= size;
        self.data = self.data[size..].to_vec();
    }
    /// compleatly flush the buffer
    pub fn flush_total(&mut self) {
        self.length = 0;
        self.data = Vec::new();
    }
}

impl HelloPacket {
    pub fn new(packet: Packet) -> Result<HelloPacket, PacketError> {
        let mut reader: usize = 0;
        if packet.get_byte(0) == Some(0xFE) && packet.get_byte(1) == Some(0x01) {
            // ping packet
            if packet.length < OLD_MINECRAFT_START.len() {
                return Err(PacketError::TooSmall);
            }
            if packet.data[0..OLD_MINECRAFT_START.len()].eq(&OLD_MINECRAFT_START) {
                let version = packet.get_byte(29).ok_or(PacketError::TooSmall)?;
                let rest_data = packet.get_u16(27).ok_or(PacketError::TooSmall)? as usize;
                let hostname = packet.get_utf16_string(30)?;

                if 7 + hostname.len() * 2 != rest_data {
                    return Err(PacketError::NotValid);
                }
                let port = packet
                    .get_u32(30 + 2 + hostname.len() * 2)
                    .ok_or(PacketError::TooSmall)?;

                return Ok(HelloPacket {
                    length: 30 + 2 /* hostnamesize */ + hostname.len() * 2 /* utf16 */ + 4, /* port */
                    id: 0,
                    version: version as i32,
                    port,
                    hostname,
                });
            }
        } else if packet.get_byte(0) == Some(0x02) && packet.get_byte(1) == Some(0x49) {
            // connect request old protocol
            let mut reader = 2;
            let username = packet.get_utf16_string(reader)?;
            reader += 2 + username.len() * 2;
            let hostname = packet.get_utf16_string(reader)?;
            reader += 2 + hostname.len() * 2;
            let port = packet.get_u32(reader).ok_or(PacketError::TooSmall)?;
            reader += 4;
            return Ok(HelloPacket {
                length: reader,
                id: 0,
                version: 0,
                port,
                hostname,
            });
        }
        let pkg_length = packet.get_varint(reader)?;
        reader += pkg_length.size;
        let id = packet.get_varint(reader)?;
        reader += id.size;
        let version = packet.get_varint(reader)?;
        // if not handshake packet return
        if id.value != 0
        /* handshake packet id */
        {
            return Err(PacketError::NotValid);
        }
        reader += version.size;

        let (hostname, hostname_len) = packet.get_utf8_string(reader)?;
        // if packet not completely received yet
        reader += hostname_len;

        let port = packet.get_u16(reader).ok_or(PacketError::TooSmall)?;
        reader += 2;
        let next_state = packet.get_varint(reader)?;
        reader += next_state.size;

        Ok(HelloPacket {
            length: reader,
            id: id.value,
            port: port as u32,
            version: version.value,
            hostname,
        })
    }
}