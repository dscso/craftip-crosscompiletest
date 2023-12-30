use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio::time::timeout;
use tokio_util::codec::Framed;

use shared::addressing::{Distributor, DistributorError, Rx};
use shared::distributor_error;
use shared::minecraft::MinecraftDataPacket;
use shared::packet_codec::PacketCodec;
use shared::proxy::{ProxyHandshakeResponse, ProxyHelloPacket, ProxyHelloResponsePacket};
use shared::socket_packet::{ChannelMessage, SocketPacket};

#[derive(Debug)]
pub struct ProxyClient {
    frames: Framed<TcpStream, PacketCodec>,
    rx: Rx,
    distributor: Arc<Mutex<Distributor>>,
    addr: SocketAddr,
    hostname: String,
}

impl ProxyClient {
    pub async fn new(
        distributor: Arc<Mutex<Distributor>>,
        mut frames: Framed<TcpStream, PacketCodec>,
        packet: ProxyHelloPacket,
    ) -> Result<Self, DistributorError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let addr = frames
            .get_ref()
            .peer_addr()
            .map_err(distributor_error!("could not get peer addr"))?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        distributor.lock().await.add_server(&packet.hostname, tx)?;
        // send response to hello packet
        let response = ProxyHelloResponsePacket {
            version: 123,
            status: ProxyHandshakeResponse::ConnectionSuccessful(),
        };
        frames.send(SocketPacket::from(response)).await
            .map_err(distributor_error!("could not send packet"))?;

        Ok(ProxyClient {
            frames,
            rx,
            addr,
            distributor,
            hostname: packet.hostname.to_string(),
        })
    }
    /// HANDLE PROXY CLIENT
    pub async fn handle(&mut self) -> Result<(), DistributorError> {
        loop {
            tokio::select! {
                result = self.rx.recv() => {
                    match result {
                        Some(ChannelMessage::Packet(pkg)) => {
                            self.frames.send(pkg).await.map_err(distributor_error!("could not send packet"))?;
                        }
                        _ => {
                            tracing::info!("connection closed by another side of unbound channel");
                            break;
                        }
                    }
                }
                result = timeout(Duration::from_secs(60), self.frames.next()) => {
                    // catching timeout error
                    let result = match result {
                        Ok(result) => result,
                        Err(e) => {
                            tracing::info!("connection to {} timed out {e}", self.addr);
                            break;
                        }
                    };
                    match result {
                        Some(Ok(packet)) => {
                            match packet {
                                SocketPacket::ProxyDisconnect(packet) => {
                                    match self.distributor.lock().await.get_client(
                                        &self.hostname,
                                        packet.client_id,
                                    ) {
                                        Ok(client) => {
                                            client.send(ChannelMessage::Close)
                                                .map_err(distributor_error!("could not send packet"))?;
                                        }
                                        // do nothing if client already disconnected
                                        Err(DistributorError::ClientNotFound) => {}
                                        res => {
                                            res.map_err(distributor_error!("could not send packet"))?;
                                            break;
                                        }
                                    }
                                }
                                SocketPacket::ProxyData(packet) => {
                                    let client_id = packet.client_id;
                                    let mc_packet = SocketPacket::MCData(MinecraftDataPacket::from(packet));
                                    let host = &self.hostname;
                                    if let Err(err) = self.distributor
                                        .lock()
                                        .await
                                        .send_to_client(host, client_id, &mc_packet) {
                                            tracing::warn!("could not send to client {}, maybe already disconnected?", err);
                                        }
                                }
                                SocketPacket::ProxyPing(packet) => {
                                    self.frames.send(SocketPacket::ProxyPong(packet)).await
                                        .map_err(distributor_error!("could not send packet"))?
                                }
                                packet => {
                                    tracing::info!("Received proxy packet: {:?}", packet);
                                }
                            }
                        }
                        // either the channel was closed or the other side closed the channel
                        _ => break
                    }
                }
            }
        }
        Ok(())
    }
    pub async fn close_connection(&mut self) {
        tracing::info!("removing proxy client {} from state", self.hostname);
        if let Err(e) = self.distributor.lock().await.remove_server(&self.hostname) {
            tracing::error!("Error while removing proxy client {}", e);
        };
    }
}
