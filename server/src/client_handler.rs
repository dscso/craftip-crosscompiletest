use std::net::SocketAddr;
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::{mpsc, Mutex};
use tokio_util::codec::Framed;

use crate::proxy_handler::ProxyClient;
use shared::addressing::{Distributor, DistributorError, Register, Rx, Tx};
use shared::distributor_error;
use shared::minecraft::{MinecraftDataPacket, MinecraftHelloPacket};
use shared::packet_codec::PacketCodec;
use shared::proxy::{ProxyClientDisconnectPacket, ProxyClientJoinPacket, ProxyDataPacket};
use shared::socket_packet::{ClientToProxy, SocketPacket};

#[derive(Debug)]
pub struct MCClient {
    frames: Framed<TcpStream, PacketCodec>,
    rx: UnboundedReceiver<MinecraftDataPacket>,
    addr: SocketAddr,
    proxy_tx: Tx,
}

impl MCClient {
    /// Create a new instance of `Peer`.
    async fn new(
        proxy_tx: Tx,
        frames: Framed<TcpStream, PacketCodec>,
        hello_packet: MinecraftHelloPacket,
    ) -> Result<Self, DistributorError> {
        // Get the client socket address
        let addr = frames
            .get_ref()
            .peer_addr()
            .map_err(distributor_error!("could not get peer address"))?;
        let hostname = hello_packet.hostname.clone();
        let (tx, rx) = mpsc::unbounded_channel();
        tracing::info!("sending client tx to proxy client {}", hostname);
        proxy_tx
            .send(ClientToProxy::AddMinecraftClient(addr, tx))
            .map_err(|_| {
                DistributorError::UnknownError("could not add minecraft client".to_string())
            })?;
        proxy_tx
            .send(ClientToProxy::Packet(
                addr,
                MinecraftDataPacket {
                    data: hello_packet.data,
                },
            ))
            .map_err(|_| {
                DistributorError::UnknownError("could not add minecraft client".to_string())
            })?;

        Ok(MCClient {
            frames,
            rx,
            proxy_tx,
            addr,
        })
    }
    /// HANDLE MC CLIENT
    pub async fn handle(&mut self) -> Result<(), DistributorError> {
        loop {
            tokio::select! {
                res = self.rx.recv() => {
                    match res {
                        Some(pkg) => {
                            self.frames.send(SocketPacket::from(pkg)).await.map_err(distributor_error!("could not send packet"))?;
                        }
                        _ => break,
                    }
                }
                result = self.frames.next() => match result {
                    Some(Ok(SocketPacket::MCData(packet))) => {
                        self.proxy_tx.send(ClientToProxy::Packet(self.addr, packet)).map_err(distributor_error!("could not send packet"))?;
                    }
                    // An error occurred.
                    Some(Err(e)) => {
                        tracing::error!("Error while receiving: {:?}", e);
                        break;
                    }
                    // The stream has been exhausted.
                    None => break,
                    obj => {
                        tracing::error!("received unknown packet from client {:?}", obj);
                    }
                },
            }
        }
        Ok(())
    }

    pub async fn close_connection(&mut self) -> Result<(), DistributorError> {
        tracing::info!("removing Minecraft client {} from state", self.addr);
        self.proxy_tx
            .send(ClientToProxy::RemoveMinecraftClient(self.addr))
            .map_err(distributor_error!("error closing connection"))?;
        Ok(())
    }
}

/// This function handles the connection to one client
/// it decides if the client is a minecraft client or a proxy client
/// forwards the traffic to the other side
/// encapsulates/decapsulates the packets
pub async fn process_socket_connection(
    socket: TcpStream,
    register: Arc<Mutex<Register>>,
) -> Result<(), DistributorError> {
    let mut frames = Framed::new(socket, PacketCodec::new(1024 * 8));
    // In a loop, read data from the socket and write the data back.
    let packet = frames.next().await.ok_or(DistributorError::UnknownError(
        "could not read first packet".to_string(),
    ))?;
    let packet = packet.map_err(distributor_error!("could not read packet"))?;

    match packet {
        SocketPacket::MCHello(packet) => {
            let proxy_tx = register.lock().await.servers.get(&packet.hostname).cloned();
            println!("proxy tx HAS TO ?BE TRUE! {:?}", proxy_tx.is_some());
            let proxy_tx = proxy_tx.ok_or(DistributorError::UnknownError(format!(
                "could not find proxy client for {}",
                packet.hostname
            )))?;
            let mut client = MCClient::new(proxy_tx.clone(), frames, packet).await?;

            let response = client.handle().await;
            client.close_connection().await?;
            response?;
        }
        SocketPacket::ProxyHello(packet) => {
            tracing::info!(
                "Proxy client connected for {} from {}",
                packet.hostname,
                frames
                    .get_ref()
                    .peer_addr()
                    .map_err(distributor_error!("could not get peer addr"))?
            );
            let mut client = ProxyClient::new(register.clone(), &packet.hostname);
            match client.authenticate(&mut frames, &packet).await {
                Ok(client) => client,
                Err(e) => {
                    tracing::warn!("could not add proxy client: {}", e);
                    frames
                        .send(SocketPacket::ProxyError(format!("Error {e}")))
                        .await?;
                    return Err(e);
                }
            };

            let response = client.handle(&mut frames).await;
            client.close_connection().await;
            println!("client closed connection {:?}", response);
            response?;
        }
        _ => {
            tracing::error!("Unknown protocol");
        }
    };

    Ok(())
}
