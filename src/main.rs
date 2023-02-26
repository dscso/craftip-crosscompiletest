mod util;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use std::{env, result};
use std::error::Error;
use thiserror::Error;

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

#[derive(Debug, Error)]
pub enum PacketError {
    #[error("Packet is too small, missing Bytes")]
    TooSmall,
    #[error("Packet is not valid")]
    NotValid,
}

#[derive(Debug)]
struct HelloPacket {
    length: usize,
    id: i32,
    version: i32,
    hostname: String,
}

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
    pub fn get_u16(&self, start: usize) -> Result<u16, PacketError> {
        if self.data.len() <= start + 1 {
            return Err(PacketError::TooSmall);
        }
        Ok(u16::from_be_bytes([
            self.data[start],
            self.data[start + 1],
        ]))
    }
    pub fn get_utf16_string(&self, start: usize) -> Result<String, PacketError> {
        //assert!(2*size <= slice.len());
        let size = self.get_u16(start)? as usize;
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
    pub fn new(buf: Vec<u8>, size: usize) -> Result<HelloPacket, PacketError> {
        let mut reader: usize = 0;

        let Ok(pkg_length) = VarInt::new(&buf, reader) else {
            return Err(PacketError::TooSmall);
        };
        reader += pkg_length.size;
        let Ok(id) = VarInt::new(&buf, reader) else {
            return Err(PacketError::TooSmall);
        };
        reader += id.size;
        let Ok(version) = VarInt::new(&buf, reader) else {
            return Err(PacketError::TooSmall);
        };
        // if not handshake packet return
        if id.value != 0 {
            return Err(PacketError::NotValid);
        }
        reader += version.size;
        let Ok(hostname_len) = VarInt::new(&buf, reader) else {
            return Err(PacketError::TooSmall);
        };
        reader += hostname_len.size;
        let hostname =
            String::from_utf8(buf[reader..reader + hostname_len.value as usize].to_vec())
                .unwrap_or_else(|_| "INVALID HOSTNAME!".to_string());
        // if packet not completely received yet
        if size < reader {
            return Err(PacketError::TooSmall);
        }
        Ok(HelloPacket {
            length: pkg_length.value as usize,
            id: id.value,
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
                    "{:?}",
                    String::from_utf8_lossy(&packet.data[..packet.length])
                );

                if !first_packet {
                    if packet.get_byte(0) == Some(0xFE) && packet.get_byte(1) == Some(0x01) {
                        println!("old minecaft protocol");
                        // ping packet
                        if packet.length < OLD_MINECRAFT_START.len() {
                            continue;
                        }
                        if packet.data[0..OLD_MINECRAFT_START.len()].eq(&OLD_MINECRAFT_START) {
                            if let Ok(hostname) = packet.get_utf16_string(30) {
                                println!("HOSTNAME {}", hostname);
                            } else {
                                continue;
                            }

                            packet.flush_total();
                            first_packet = true;

                        }
                        continue;
                    } else if packet.get_byte(0) == Some(0x02) && packet.get_byte(1) == Some(0x49) {
                        let username_length = packet.get_byte(3); //.ok_or(PacketError::TooSmall)?;
                        if username_length == None {
                            continue;
                        }

                        let username = packet.get_utf16_string(2).unwrap();
                        println!("USERNAME: {}", username);
                        let hostname = packet.get_utf16_string(username.len() * 2 + 4).unwrap();
                        println!("hostname: {}", hostname);

                        packet.flush_total();
                        first_packet = true;
                        continue;
                    }
                }
                let hello = HelloPacket::new(packet.data.to_vec(), packet.length);
                if hello.is_ok() {
                    println!("{:?}", hello);
                    //packet = packet[hello.unwrap().length..packet_length].to_vec();
                }

                println!("{:?}", &buf[0..n]);
                println!("{:?}", String::from_utf8_lossy(&buf[0..n]));
                /*
                //convert buffer to string
                println!(
                    "{} bytes received, {:?}",
                    n,
                    String::from_utf8_lossy(&buf[0..n])
                );
                if !first_packet {
                    //let packet = Packet::new(&buf);
                    let mut buffer_pos: usize = 0;
                    let length = VarInt::new(&buf).unwrap();
                    buffer_pos += length.size;
                    println!("length: {} bytes with value {} ", length.size, length.value);
                    let packet = VarInt::new(&buf[buffer_pos..]).unwrap();
                    buffer_pos += packet.size;
                    println!("packet: {} bytes with value {} ", packet.size, packet.value);
                    if packet.value == 0 {
                        println!("Handshake-packet received!");
                        let version = VarInt::new(&buf[buffer_pos..]).unwrap();
                        buffer_pos += version.size;
                        let hostname_size = VarInt::new(&buf[buffer_pos..]).unwrap();
                        buffer_pos += hostname_size.size;
                        let hostname = String::from_utf8(
                            buf[buffer_pos..buffer_pos + hostname_size.value as usize].to_vec(),
                        )
                        .unwrap();
                        println!(
                            "packet: version: {},  {} bytes with value {} HOSTNAME: {} ",
                            version.value, hostname_size.size, hostname_size.value, hostname
                        );
                    }
                    first_packet = true;
                } else {
                    println!("got new packet: {:?}", &buf[0..n]);
                }
                /*println!("packet: len {} of version {} with content {:?}",n , version.value,  &buf[0..length.value as usize]);
                println!("packet: len {} {:?}",n , &buf[0..n]);*/

                /*socket
                .write_all(&buf[0..n])
                .await
                .expect("failed to write data to socket");*/

                 */
            }
        });
    }
}
