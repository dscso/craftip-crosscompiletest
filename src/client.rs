use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use futures::SinkExt;
use tokio::net::TcpStream;
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

mod cursor;
mod datatypes;
mod minecraft;
mod packet_codec;
mod proxy;
mod socket_packet;

use crate::minecraft::MinecraftHelloPacket;
use packet_codec::PacketCodec;
use socket_packet::SocketPacket;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let pkg = SocketPacket::from(MinecraftHelloPacket {
        length: 73,
        id: 99,
        version: 66,
        hostname: "00000".to_string(),
        port: 123,
        data: vec![1, 2, 3, 4],
    });

    let result = bincode::serialize(&pkg).unwrap();
    println!("{:?}", result);
    // Connect to server 1
    let server1_addr = "127.0.0.1:25565";
    let mut proxy_stream = TcpStream::connect(server1_addr).await?;

    // Connect to server 2
    let server2_addr = "127.0.0.1:25564";
    let mut mc_server = TcpStream::connect(server2_addr).await?;
    let mut proxy = Framed::new(proxy_stream, PacketCodec::new(1024 * 8));
    // Create a buffer to store received messages
    //let mut buf = [0; 1024];
    let mut buf2 = [0; 1024];
    let hello = SocketPacket::from(SocketPacket::ProxyHelloPacket(proxy::ProxyHelloPacket {
        length: 0,
        version: 123,
        hostname: "localhost".to_string(),
    }));
    proxy.send(hello).await?;
    println!("Sent hello packet");
    //mc_server.write_all(&[16, 0, 249, 5, 9, 108, 111, 99, 97, 108, 104, 111, 115, 116, 99, 221]).await?;
    //mc_server.write_all(&[2, 30, 0, 11, 80, 101, 110, 110, 101, 114, 81, 117, 101, 101, 110, 1]).await?;
    loop {
        // Read from server 1
        tokio::select! {
            result = proxy.next() => match result {
                // A message was received from the current user, we should
                // broadcast this message to the other users.
                Some(Ok(msg)) => {
                    println!("received message from server 1: {:?}", msg);
                    // Forward the message to server 2
                    /*let mut bytes = BytesMut::from(msg.as_bytes());
                    let packet = SocketPacket::parse_packet(&mut bytes, &Protocol::Proxy(1)).unwrap();
                    //println!("Server 1: {:?}", packet);
                    // Forward the message to server 2
                    */
                    if let SocketPacket::ProxyDataPacket(packet) = msg {
                        mc_server.write_all(&packet.data[..]).await?;
                    }
                }
                // An error occurred.
                Some(Err(e)) => {
                    tracing::error!(
                        "an error occurred while processing messages error = {:?}",
                        e
                    );
                }
                // The stream has been exhausted.
                None => {
                    println!("Proxy has closed the connection");
                    break
                },
            },

            n = mc_server.read(&mut buf2) => {
                let n = n?;
                if n == 0 {
                    println!("MC has closed the connection");
                    break; // server 2 has closed the connection
                }
                println!("received message from server 2: {:?}", &buf2[0..n]);
                // Forward the message to server 1
                let packet = SocketPacket::from(proxy::ProxyDataPacket {
                    data: buf2[0..n].to_vec(),
                    client_id: 0,
                    length: n as usize,
                });
                // bytes to st
                //let json = String::from_utf8_lossy(&packet.encode().unwrap()).to_string();
                //println!("Server 2: {} content: {:?}",n,  packet);
                //println!("sendign json: {}", json);
                println!("sendign {:?}", packet);
                proxy.send(packet).await?;
            }
        }
    }

    Ok(())
}
