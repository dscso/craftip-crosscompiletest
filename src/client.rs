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

use packet_codec::PacketCodec;
use socket_packet::SocketPacket;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Connect to server 1
    let server1_addr = "127.0.0.1:25565";
    let mut proxy_stream = TcpStream::connect(server1_addr).await?;

    // Connect to server 2
    let mc_server_addr = "127.0.0.1:25564";
    let mut mc_server = TcpStream::connect(mc_server_addr).await?;
    let mut proxy = Framed::new(proxy_stream, PacketCodec::new(1024 * 8));

    let mut buf2 = [0; 1024];
    let hello = SocketPacket::from(SocketPacket::ProxyHelloPacket(proxy::ProxyHelloPacket {
        length: 0,
        version: 123,
        hostname: "localhost".to_string(),
    }));
    proxy.send(hello).await?;
    println!("Sent hello packet");

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
                println!("sending {:?}", packet);
                proxy.send(packet).await?;
            }
        }
    }

    Ok(())
}
