use std::collections::HashMap;
use std::net::SocketAddr;
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

#[derive(Debug, Default)]
pub struct Distribiutor {
    clients_addr: HashMap<SocketAddr, MinecraftClient>,
    clients_id: HashMap<u16, SocketAddr>,
}

impl Distribiutor {
    fn insert(&mut self, addr: SocketAddr, client: MinecraftClient) {
        self.clients_id.insert(client.id, addr);
        self.clients_addr.insert(addr, client);
    }
    fn remove_by_addr(&mut self, addr: &SocketAddr) {
        if let Some(client) = self.clients_addr.get(addr) {
            self.clients_id.remove(&client.id);
        }
        self.clients_addr.remove(addr);
    }
    fn remove_by_id(&mut self, id: u16) {
        if let Some(addr) = self.clients_id.get(&id) {
            self.clients_addr.remove(addr);
        }
        self.clients_id.remove(&id);
    }
    fn get_by_addr(&self, addr: &SocketAddr) -> Option<&MinecraftClient> {
        return self.clients_addr.get(addr)
    }
    fn get_by_id(&self, id: u16) -> Option<&MinecraftClient>{
        return self.clients_id.get(&id).and_then(|addr| self.clients_addr.get(addr))
    }
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
        let mut distributor = Distribiutor::default();

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
                            distributor.insert(addr, MinecraftClient { tx, id: current_clientid });
                            current_clientid += 1;
                            tracing::info!("Added minecraft client: {:?} {:?}", addr, distributor);
                        },
                        ClientToProxy::Packet(addr, pkg) => {
                            if let Some(client) = distributor.get_by_addr(&addr) {
                                let pkg = SocketPacket::from(ProxyDataPacket::new(pkg.data, client.id));
                                framed.send(pkg).await?;
                            } else {
                                break
                            }
                        },
                        ClientToProxy::RemoveMinecraftClient(addr) => {
                            if let Some(client) = distributor.get_by_addr(&addr) {
                                framed.send(SocketPacket::from(ProxyClientDisconnectPacket { client_id: client.id })).await?;
                            } else {
                                break
                            }
                            distributor.remove_by_addr(&addr);
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

                                }
                                SocketPacket::ProxyData(packet) => {
                                    if let Some(client) = distributor.get_by_id(packet.client_id) {
                                        let mc_packet = MinecraftDataPacket::from(packet);
                                        client.tx.send(mc_packet).map_err(distributor_error!("could not send packet"))?;
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
