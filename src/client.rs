use std::collections::HashMap;
use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use futures::SinkExt;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

mod cursor;
mod datatypes;
mod minecraft;
mod packet_codec;
mod proxy;
mod socket_packet;

use packet_codec::PacketCodec;
use socket_packet::ChannelMessage;
use socket_packet::SocketPacket;

pub type Tx = mpsc::UnboundedSender<ChannelMessage<SocketPacket>>;
pub type Rx = mpsc::UnboundedReceiver<ChannelMessage<SocketPacket>>;

struct Shared {
    connections: HashMap<u16, mpsc::UnboundedSender<ChannelMessage<Vec<u8>>>>,
}

impl Shared {
    /// Create a new, empty, instance of `Shared`.
    pub fn new() -> Self {
        Shared {
            connections: HashMap::new(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Connect to server 1
    let server1_addr = "127.0.0.1:25565";
    let mut proxy_stream = TcpStream::connect(server1_addr).await?;

    // Connect to server 2
    let mc_server_addr = "127.0.0.1:25564";
    let mut proxy = Framed::new(proxy_stream, PacketCodec::new(1024 * 8));

    let hello = SocketPacket::from(SocketPacket::ProxyHelloPacket(proxy::ProxyHelloPacket {
        length: 0,
        version: 123,
        hostname: "localhost".to_string(),
    }));
    proxy.send(hello).await?;
    println!("Sent hello packet");
    let (tx, mut rx) = mpsc::unbounded_channel();
    let state = Arc::new(Mutex::new(Shared::new()));
    loop {
        // Read from server 1
        tokio::select! {
            Some(pkg) = rx.recv() => {
                //tracing::info!("Sending packet to client: {:?}", pkg);
                match pkg {
                    ChannelMessage::Packet(pkg) => {
                        proxy.send(pkg).await?;
                    }
                    ChannelMessage::Close => {
                        break;
                    }
                }

            }
            result = proxy.next() => match result {
                Some(Ok(msg)) => {
                    println!("received message from server 1: {:?}", msg);
                    match msg {
                        SocketPacket::ProxyJoinPacket(packet) => {
                            let (client_tx, client_rx) = mpsc::unbounded_channel();
                            {
                                state.lock().await.connections.insert(packet.client_id, client_tx);
                            }
                            let tx_clone = tx.clone();
                            let state_clone = state.clone();
                            tokio::spawn(async move {
                                handle_client(tx_clone, client_rx, mc_server_addr, packet.client_id, state_clone).await.expect("TODO: panic message");
                            });
                        }
                        SocketPacket::ProxyDataPacket(packet) => {
                            match state.lock().await.connections.get(&packet.client_id) {
                                Some(tx) => {
                                    tx.send(ChannelMessage::Packet(packet.data.to_vec()))?;
                                }
                                None => {
                                    println!("error could not minecraft client")
                                }

                            }
                        }
                        _ => {
                            unimplemented!("Message not implemented!")
                        }
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
        }
    }

    Ok(())
}

async fn handle_client(
    tx: Tx,
    mut rx: mpsc::UnboundedReceiver<ChannelMessage<Vec<u8>>>,
    mc_server_addr: &str,
    client_id: u16,
    state: Arc<Mutex<Shared>>,
) -> Result<(), Box<dyn Error>> {
    println!("opening new client with id {}", client_id);
    // connect to server
    let mut buf = [0; 1024];
    let mut mc_server = TcpStream::connect(mc_server_addr).await?;
    loop {
        tokio::select! {
            Some(pkg) = rx.recv() => {
                //tracing::info!("Sending packet to client: {:?}", pkg);
                match pkg {
                    ChannelMessage::Packet(data) => {
                        mc_server.write_all(&data).await?;
                    }
                    ChannelMessage::Close => {
                        break;
                    }
                }
            }
            n = mc_server.read(&mut buf) => {
                let n = n?;
                if n == 0 {
                    println!("MC has closed the connection");
                    break; // server 2 has closed the connection
                }
                println!("received message from server 2: {:?}", &buf[0..n]);
                // encapsulate in ProxyDataPacket
                let packet = SocketPacket::from(proxy::ProxyDataPacket {
                    data: buf[0..n].to_vec(),
                    client_id: client_id,
                    length: n as usize,
                });

                println!("sending {:?}", packet);
                tx.send(ChannelMessage::Packet(packet))?;
            }
        }
        println!("clonsing new client with id {}", client_id);
    }
    rx.close();

    let packet = SocketPacket::from(proxy::ProxyClientDisconnectPacket {
        length: 0,
        client_id,
    });
    tx.send(ChannelMessage::Packet(packet))?;

    state.lock().await.connections.remove(&client_id);
    Ok(())
}
