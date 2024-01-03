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
use shared::proxy::{ProxyAuthenticator, ProxyConnectedResponse, ProxyHelloPacket};
use shared::socket_packet::{ChannelMessage, SocketPacket};

#[derive(Debug)]
pub struct ProxyClient {
    rx: Rx,
    distributor: Arc<Mutex<Distributor>>,
    addr: SocketAddr,
    hostname: String,
}

impl ProxyClient {
    pub async fn new(
        distributor: Arc<Mutex<Distributor>>,
        frames: &mut Framed<TcpStream, PacketCodec>,
        packet: ProxyHelloPacket,
    ) -> Result<Self, DistributorError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let addr = frames
            .get_ref()
            .peer_addr()
            .map_err(distributor_error!("could not get peer addr"))?;

        match packet.auth {
            ProxyAuthenticator::PublicKey(public_key) => {
                let challenge = public_key.create_challange();
                let auth_request = SocketPacket::ProxyAuthRequest(challenge);

                frames.send(auth_request).await?;

                let signature = match frames.next().await {
                    Some(Ok(SocketPacket::ProxyAuthResponse(signature))) => signature,
                    e => {
                        return Err(DistributorError::UnknownError(format!(
                            "Invalid auth response {:?}",
                            e
                        )))
                    }
                };

                if public_key.verify(&challenge, &signature) {
                    tracing::info!("Client {} authenticated successfully", packet.hostname);
                } else {
                    return Err(DistributorError::AuthError);
                }
            }
        }
        // add client to distributor
        distributor.lock().await.add_server(&packet.hostname, tx)?;

        // send connected
        let resp = SocketPacket::from(ProxyConnectedResponse { version: 123 });
        frames.send(resp).await?;

        Ok(ProxyClient {
            rx,
            addr,
            distributor,
            hostname: packet.hostname.to_string(),
        })
    }
    /// HANDLE PROXY CLIENT
    pub async fn handle(
        &mut self,
        framed: &mut Framed<TcpStream, PacketCodec>,
    ) -> Result<(), DistributorError> {
        loop {
            tokio::select! {
                result = self.rx.recv() => {
                    match result {
                        Some(ChannelMessage::Packet(pkg)) => {
                            framed.send(pkg).await?;
                        }
                        _ => {
                            tracing::info!("connection closed by another side of unbound channel");
                            break;
                        }
                    }
                }
                result = timeout(Duration::from_secs(60), framed.next()) => {
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
                                    framed.send(SocketPacket::ProxyPong(packet)).await?
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
