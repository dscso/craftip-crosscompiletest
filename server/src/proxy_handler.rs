use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{mpsc, Mutex};
use tokio::time::timeout;
use tokio_util::codec::Framed;

use shared::addressing::{DistributorError, Register, Tx};
use shared::distributor_error;
use shared::minecraft::MinecraftDataPacket;
use shared::packet_codec::PacketCodec;
use shared::proxy::{
    ProxyAuthenticator, ProxyClientDisconnectPacket, ProxyClientJoinPacket, ProxyConnectedResponse,
    ProxyDataPacket, ProxyHelloPacket,
};
use shared::socket_packet::{ClientToProxy, SocketPacket};

#[derive(Debug, Clone)]
pub struct MinecraftClient {
    tx: UnboundedSender<MinecraftDataPacket>,
    id: u16,
}
#[derive(Debug)]
pub struct ProxyClient {
    register: Arc<Mutex<Register>>,
    hostname: String,
}

impl ProxyClient {
    pub fn new(register: Arc<Mutex<Register>>, hostname: &str) -> Self {
        ProxyClient {
            register,
            hostname: hostname.to_string(),
        }
    }
    /// HANDLE PROXY CLIENT
    pub async fn handle(
        &mut self,
        framed: &mut Framed<TcpStream, PacketCodec>,
    ) -> Result<(), DistributorError> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut minecraft_clients_addr = HashMap::new();
        let mut minecraft_clients_id = HashMap::new();

        let mut current_clientid = 0;
        self.register
            .lock()
            .await
            .servers
            .insert(self.hostname.clone(), tx);

        // send connected
        let resp = SocketPacket::from(ProxyConnectedResponse { version: 123 });
        framed.send(resp).await?;
        loop {
            tokio::select! {
                // forward packets from the minecraft clients
                result = rx.recv() => {
                    let result = match result {
                        Some(result) => result,
                        None => {
                            tracing::info!("client channel closed {}", self.hostname);
                            break
                        }
                    };
                    match result {
                        ClientToProxy::Close => {
                            tracing::info!("closing channel for proxy client {}", self.hostname);
                            break
                        },
                        ClientToProxy::AddMinecraftClient(addr, tx) => {
                            framed.send(SocketPacket::from(ProxyClientJoinPacket { client_id: current_clientid })).await?;
                            minecraft_clients_addr.insert(addr, MinecraftClient {
                                tx,
                                id: current_clientid,
                            });
                            minecraft_clients_id.insert(current_clientid, addr);
                            current_clientid += 1;
                            tracing::info!("Added minecraft client: {:?} {:?}", addr, minecraft_clients_addr);
                        },
                        ClientToProxy::Packet(addr, pkg) => {
                            let mcserver = minecraft_clients_addr.get(&addr).unwrap().clone();
                            let client_id = mcserver.id;
                            let pkg = SocketPacket::ProxyData(ProxyDataPacket {
                                client_id,
                                data: pkg.data,
                            });
                            framed.send(pkg).await?;
                        },
                        ClientToProxy::RemoveMinecraftClient(addr) => {
                            if let Some(mc_server) = minecraft_clients_addr.get(&addr).cloned() {
                                minecraft_clients_addr.remove(&addr);
                                minecraft_clients_id.remove(&mc_server.id);
                                framed.send(SocketPacket::from(ProxyClientDisconnectPacket { client_id: mc_server.id })).await?;
                            }
                        }
                        _ => {}
                    }
                }
                // handle packets from the proxy client
                result = timeout(Duration::from_secs(60), framed.next()) => {
                    // catching timeout error
                    match result {
                        Ok(Some(Ok(packet))) => {
                            match packet {
                                SocketPacket::ProxyDisconnect(packet) => {
                                    if let Some(addr) = minecraft_clients_id.get(&packet.client_id) {
                                         minecraft_clients_addr.remove(&addr);
                                         minecraft_clients_id.remove(&packet.client_id);
                                    }
                                }
                                SocketPacket::ProxyData(packet) => {
                                    if let Some(addr) = minecraft_clients_id.get(&packet.client_id) {
                                        let mc_packet = MinecraftDataPacket::from(packet);
                                        minecraft_clients_addr.get(&addr).unwrap().tx.send(mc_packet).map_err(distributor_error!("could not send packet"))?;
                                    } else {
                                        tracing::error!("already disconnected! Packet will not be delivered {:?}", packet);
                                    }
                                },
                                SocketPacket::ProxyPing(packet) => {
                                    framed.send(SocketPacket::ProxyPong(packet)).await?
                                }
                                packet => {
                                    tracing::info!("Received proxy packet: {:?}", packet);
                                }
                            }
                        }
                        // either the channel was closed or the other side closed the channel or timeout
                        e => {
                            tracing::info!("Connection will be closed due to {:?}", e);
                            break
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub async fn close_connection(&mut self) {
        tracing::info!("removing proxy client {} from state", self.hostname);
        self.register.lock().await.servers.remove(&self.hostname);
    }
    pub async fn authenticate(
        &mut self,
        frames: &mut Framed<TcpStream, PacketCodec>,
        packet: &ProxyHelloPacket,
    ) -> Result<(), DistributorError> {
        match &packet.auth {
            ProxyAuthenticator::PublicKey(public_key) => {
                let challenge = public_key.create_challange();
                let auth_request = SocketPacket::ProxyAuthRequest(challenge);

                frames.send(auth_request).await?;

                let signature = match frames.next().await {
                    Some(Ok(SocketPacket::ProxyAuthResponse(signature))) => signature,
                    e => {
                        tracing::info!("Client did follow the auth procedure {:?}", e);
                        return Err(DistributorError::WrongPacket);
                    }
                };

                // verify if client posses the private key
                if public_key.verify(&challenge, &signature)
                    && public_key.get_hostname() == packet.hostname
                {
                    tracing::info!("Client {} authenticated successfully", packet.hostname);
                    return Ok(());
                }
            }
        }
        Err(DistributorError::AuthError)
    }
}
