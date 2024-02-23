use std::collections::HashMap;
use std::time::Duration;
use std::time::SystemTime;

use anyhow::{bail, Context, Result};
use futures::SinkExt;
use shared::config::PROTOCOL_VERSION;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use shared::packet_codec::PacketCodec;
use shared::proxy::{ProxyAuthenticator, ProxyDataPacket, ProxyHelloPacket};
use shared::socket_packet::SocketPacket;

use crate::connection_handler::ClientConnection;
use crate::structs::{
    ClientError, ClientToProxy, Control, ControlRx, ProxyToClient, ProxyToClientTx, Server,
    ServerAuthentication, Stats, StatsTx,
};

pub struct Client {
    state: State,
    stats_tx: StatsTx,
    proxy: Option<Framed<TcpStream, PacketCodec>>,
    control_rx: ControlRx,
    server: Server,
}

#[derive(Default)]
pub struct State {
    connections: HashMap<u16, ProxyToClientTx>,
    stats_tx: Option<StatsTx>,
}

impl State {
    pub fn set_stats_tx(&mut self, tx: StatsTx) {
        self.stats_tx = Some(tx);
    }
    pub fn add_connection(&mut self, id: u16, tx: ProxyToClientTx) {
        self.connections.insert(id, tx);
        if let Some(tx) = &self.stats_tx {
            tx.send(Stats::ClientsConnected(self.connections.len() as u16))
                .unwrap();
        }
    }
    pub fn remove_connection(&mut self, id: u16) {
        self.connections.remove(&id);
        if let Some(tx) = &self.stats_tx {
            tx.send(Stats::ClientsConnected(self.connections.len() as u16))
                .unwrap();
        }
    }
    pub fn send_to(&mut self, id: u16, msg: ProxyToClient) -> Result<()> {
        let channel = self
            .connections
            .get_mut(&id)
            .context(format!("could not find client id {}, {:?}", id, msg))?;
        channel.send(msg).unwrap_or_else(|_| {
            self.connections.remove(&id);
        });
        Ok(())
    }
}

impl Client {
    pub async fn new(server: Server, stats_tx: StatsTx, control_rx: ControlRx) -> Self {
        let mut state = State::default();
        state.set_stats_tx(stats_tx.clone());
        Client {
            server,
            stats_tx,
            state,
            control_rx,
            proxy: None,
        }
    }
}

impl Client {
    pub async fn connect(&mut self) -> Result<(), ClientError> {
        // test connection to minecraft server
        TcpStream::connect(&self.server.local)
            .await
            .map_err(|_| ClientError::MinecraftServerNotFound)?;
        // connect to proxy
        let proxy_stream = TcpStream::connect(format!("{}:25565", &self.server.server)).await?;
        let mut proxy = Framed::new(proxy_stream, PacketCodec::new(1024 * 4));

        let hello = SocketPacket::from(ProxyHelloPacket {
            version: PROTOCOL_VERSION,
            hostname: self.server.server.clone(),
            auth: match &mut self.server.auth {
                ServerAuthentication::Key(private_key) => {
                    ProxyAuthenticator::PublicKey(private_key.get_public_key())
                }
            },
        });

        proxy.send(hello).await?;
        let challenge = match timeout(Duration::from_secs(10), proxy.next()).await {
            Ok(Some(Ok(SocketPacket::ProxyAuthRequest(pkg)))) => pkg,
            Err(_) => return Err(ClientError::Timeout),
            Ok(e) => return Err(ClientError::UnexpectedPacket(format!("{:?}", e))),
        };

        match &mut self.server.auth {
            ServerAuthentication::Key(private_key) => {
                let signature = private_key.sign(&challenge);
                proxy
                    .send(SocketPacket::ProxyAuthResponse(signature))
                    .await?;
            }
        }

        tokio::select! {
            res = proxy.next() => match res {
                Some(Ok(SocketPacket::ProxyHelloResponse(_hello_response))) => Ok(()),
                Some(Ok(SocketPacket::ProxyError(e))) => Err(ClientError::ProxyError(e)),
                None => Err(ClientError::ProxyClosedConnection),
                Some(Err(e)) => Err(ClientError::ProtocolError(e)),
                e => return Err(ClientError::UnexpectedPacket(format!("{:?}", e))),
            }?,
            res = self.control_rx.recv() => match res {
                Some(Control::Disconnect) | None => {
                    return Err(ClientError::UserClosedConnection)
                }
            }
        }
        tracing::info!("Connected to proxy server!");
        self.stats_tx
            .send(Stats::Connected)
            .map_err(|e| ClientError::Other(e.into()))?;
        self.proxy = Some(proxy);
        Ok(())
    }
    pub async fn handle(&mut self) -> Result<()> {
        let (to_proxy_tx, mut to_proxy_rx) = mpsc::unbounded_channel();
        let proxy = self.proxy.as_mut().unwrap();
        loop {
            tokio::select! {
                // process control messages e.g. form gui
                result = self.control_rx.recv() => {
                    match result {
                        Some(Control::Disconnect) | None => {
                            return Ok(());
                        }
                    }
                }
                // send packets to proxy
               Some(pkg) = to_proxy_rx.recv() => {
                    //tracing::info!("Sending packet to client: {:?}", pkg);
                    match pkg {
                        ClientToProxy::Packet(id, pkg) => {
                            proxy.send(SocketPacket::from(ProxyDataPacket::new(pkg, id))).await?;
                        },
                        ClientToProxy::RemoveMinecraftClient(id) => {
                            proxy.send(SocketPacket::ProxyDisconnect(id)).await?;
                            self.state.remove_connection(id);
                        },
                        ClientToProxy::Death(msg) => {
                            bail!(msg);
                        }
                    }
                }
                // receive proxy packets
                result = proxy.next() => {
                    match result {
                        Some(Ok(msg)) => {
                            match msg {
                                SocketPacket::ProxyJoin(client_id) => {
                                    let (mut client_connection, client_tx) = ClientConnection::new(to_proxy_tx.clone(), self.server.local.clone(), client_id).await;
                                    self.state.add_connection(client_id, client_tx);
                                    tokio::spawn(async move {
                                        if let Err(e) = client_connection.handle_client().await {
                                            tracing::error!("An Error occurred in the handle_client function: {}", e);
                                            // sometimes handle_client closes after gui, errors can occur
                                            client_connection.set_death(e.to_string());
                                        }
                                    });
                                }
                                SocketPacket::ProxyData(packet) => {
                                    self.state.send_to(packet.client_id, packet.packet)?;
                                }
                                SocketPacket::ProxyDisconnect(client_id) => {
                                    // this can fail if the client is already disconnected
                                    self.state.remove_connection(client_id);
                                }
                                SocketPacket::ProxyPong(ping) => {
                                    let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u16;
                                    let ping = time.saturating_sub(ping);
                                    self.stats_tx.send(Stats::Ping(ping))?;
                                }
                                _ => unimplemented!("Message not implemented!")
                            }
                        }
                        // An error occurred.
                        Some(Err(e)) => bail!("an error occurred while processing messages error = {:?}", e),
                        // The stream has been exhausted.
                        None => bail!("Proxy has closed the connection")
                    }
                },
                // ensure constant traffic so tcp connection does not close
                _ = sleep(Duration::from_secs(1)) => {
                    let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u16;
                    proxy.send(SocketPacket::ProxyPing(time)).await?;
                    continue;
                }
            }
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        tracing::info!("Proxy client dropped");
    }
}
