mod test;

use tokio::io::{AsyncReadExt};
use tokio::net::TcpListener;

use std::{env};
use std::error::Error;
use thiserror::Error;

#[derive(Debug, PartialEq, Eq)]
struct VarInt {
    value: i32,
    size: usize,
}

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
    #[error("UTF-16 String is not valid")]
    NotValidUTF16,
}

#[derive(Debug, Eq, PartialEq)]
struct HelloPacket {
    length: usize,
    id: i32,
    version: i32,
    hostname: String,
    port: u32,
}
#[derive(Debug, Clone)]
struct Packet {
    length: usize,
    data: Vec<u8>,
}

impl Packet {
    pub fn new() -> Packet {
        Packet {
            length: 0,
            data: Vec::new(),
        }
    }

    pub fn add_data(&mut self, data: &[u8], size: usize) {
        self.length += size;
        self.data.extend_from_slice(data[..size].as_ref());
    }
    pub fn get_varint(&self, start: usize) -> Result<VarInt, PacketError> {
        return VarInt::new(&self.data, start).map_err(|_| PacketError::TooSmall);
    }
    pub fn get_u16(&self, start: usize) -> Option<u16> {
        if self.data.len() <= start + 1 {
            return None;
        }
        Some(u16::from_be_bytes([
            self.data[start],
            self.data[start + 1],
        ]))
    }
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

        let result = std::char::decode_utf16(iter)
            .collect::<Result<String, _>>();

        match result {
            Ok(s) => Ok(s),
            Err(_) => Err(PacketError::NotValid),
        }
    }
    pub fn get_byte(&self, index: usize) -> Option<u8> {
        if index >= self.data.len() {
            return None;
        }
        Some(self.data[index])
    }
    pub fn flush_packet(&mut self, size: usize) {
        self.length -= size;
        self.data = self.data[size..].to_vec();
    }
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
                let port = packet.get_u32(30 + 2 + hostname.len() * 2).ok_or(PacketError::TooSmall)?;

                return Ok(HelloPacket {
                    length: 30 + 2 /* hostnamesize */ + hostname.len() * 2 /* utf16 */ + 4 /* port */,
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
        let Ok(pkg_length) = packet.get_varint(reader) else {
            return Err(PacketError::TooSmall);
        };
        reader += pkg_length.size;
        let Ok(id) = packet.get_varint(reader) else {
            return Err(PacketError::TooSmall);
        };
        reader += id.size;
        let Ok(version) = packet.get_varint(reader) else {
            return Err(PacketError::TooSmall);
        };
        // if not handshake packet return
        if id.value != 0 {
            return Err(PacketError::NotValid);
        }
        reader += version.size;
        let Ok(hostname_len) = packet.get_varint(reader) else {
            return Err(PacketError::TooSmall);
        };
        reader += hostname_len.size;
        let hostname =
            String::from_utf8(packet.data[reader..reader + hostname_len.value as usize].to_vec())
                .unwrap_or_else(|_| "INVALID HOSTNAME!".to_string());
        // if packet not completely received yet
        if packet.length < reader {
            return Err(PacketError::TooSmall);
        }
        Ok(HelloPacket {
            length: pkg_length.value as usize,
            id: id.value,
            port: 123,
            version: version.value,
            hostname,
        })
    }

    /*pub fn add_data(&mut self, data: &[u8]) {
        if self.length.is_none() {
            let length = VarInt::new(data);
            self.length = Some(length.value);
        } else if self.id.is_none() {
            let id = VarInt::new(data);
            self.id = Some(id.value);
        } else {
            self.data = Some(data.to_vec());
        }
        let length = VarInt::new(&buf);
        let id = VarInt::new(&buf[length.size..]);
        let data = buf[length.size + id.size..].to_vec();
    }*/
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:25565".to_string());

    // Next up we create a TCP listener which will listen for incoming
    // connections. This TCP listener is bound to the address we determined
    // above and must be associated with an event loop.
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on: {}", addr);

    loop {
        // Asynchronously wait for an inbound socket.
        let (mut socket, _) = listener.accept().await?;

        // And this is where much of the magic of this server happens. We
        // crucially want all clients to make progress concurrently, rather than
        // blocking one on completion of another. To achieve this we use the
        // `tokio::spawn` function to execute the work in the background.
        //
        // Essentially here we're executing a new task to run concurrently,
        // which will allow all of our clients to be processed concurrently.

        tokio::spawn(async move {
            let mut packet = Packet::new();
            let mut first_packet = false;

            // In a loop, read data from the socket and write the data back.
            loop {
                let mut buf: Vec<u8> = vec![0; 1024];
                let n = socket
                    .read(&mut buf)
                    .await
                    .expect("failed to read data from socket");

                if n == 0 {
                    return;
                }

                packet.add_data(&buf, n); // adding frame to packet buffer

                println!(
                    "len: {} {:?}", packet.length,
                    &packet.data[..packet.length]
                );

                if !first_packet {
                        let hello_packet = HelloPacket::new(packet.clone());
                        match hello_packet {
                            Ok(hello_packet) => {
                                println!("hello packet: {:?}", hello_packet);
                                packet.flush_packet(hello_packet.length);
                                first_packet = true;
                            }
                            Err(e) => {
                                if e == PacketError::TooSmall {
                                    continue;
                                }
                                println!("error: {:?}", e);
                            }
                        }
                }
                }
        });
    }
}
