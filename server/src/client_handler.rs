use std::net::SocketAddr;
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_util::codec::Framed;

use shared::addressing::{Distributor, DistributorError, Rx};
use shared::distributor_error;
use shared::minecraft::MinecraftHelloPacket;
use shared::packet_codec::PacketCodec;
use shared::proxy::{ProxyClientDisconnectPacket, ProxyClientJoinPacket, ProxyDataPacket};
use crate::proxy_handler::ProxyClient;
use shared::socket_packet::{ChannelMessage, SocketPacket};

#[derive(Debug)]
pub struct MCClient {
    frames: Framed<TcpStream, PacketCodec>,
    rx: Rx,
    distributor: Arc<Mutex<Distributor>>,
    addr: SocketAddr,
    id: u16,
    hostname: String,
}

impl MCClient {
    /// Create a new instance of `Peer`.
    async fn new(
        distributor: Arc<Mutex<Distributor>>,
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

        let id = distributor.lock().await.add_client(&addr, &hostname, tx)?;

        tracing::info!("added client with id: {}", id);

        // telling proxy client that there is a new client

        let client_join_packet = ProxyClientJoinPacket::new(id);
        if let Err(err) = distributor
            .lock()
            .await
            .send_to_server(&hostname, SocketPacket::from(client_join_packet)) {
            // this should never happen
            distributor.lock().await.remove_client(&addr)?;
            return Err(err);
        }


        let mut packet = ProxyDataPacket::from_mc_hello_packet(&hello_packet, id);
        packet.client_id = id;
        let packet = SocketPacket::ProxyData(packet);
        if let Err(err) = distributor.lock().await.send_to_server(&hostname, packet) {
            // this should never happen
            distributor.lock().await.remove_client(&addr)?;
            return Err(err);
        }


        Ok(MCClient {
            frames,
            rx,
            distributor,
            addr,
            id,
            hostname: hello_packet.hostname.to_string(),
        })
    }
    /// HANDLE MC CLIENT
    pub async fn handle(&mut self) -> Result<(), DistributorError> {
        loop {
            tokio::select! {
                res = self.rx.recv() => {
                    match res {
                        Some(ChannelMessage::Packet(pkg)) => {
                            self.frames.send(pkg).await.map_err(distributor_error!("could not send packet"))?;
                        }
                        _ => break,
                    }
                }
                result = self.frames.next() => match result {
                    Some(Ok(SocketPacket::MCData(packet))) => {
                        let packet = SocketPacket::from(ProxyDataPacket::from_mc_packet(packet, self.id));
                        self.distributor.lock()
                            .await
                            .send_to_server(&self.hostname, packet)?
                    }
                    // An error occurred.
                    Some(Err(e)) => {
                        tracing::error!("Error while receiving: {:?}", e);
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
        let packet = SocketPacket::from(ProxyClientDisconnectPacket::new(self.id));
        if let Err(err) = self
            .distributor
            .lock()
            .await
            .send_to_server(&self.hostname, packet)
        {
            tracing::debug!("could not send disconnect packet to proxy {}", err);
        }

        if let Err(e) = self.distributor.lock().await.remove_client(&self.addr) {
            tracing::debug!("Error while removing mc client {}", e);
        }
        Ok(())
    }
}

/// This function handles the connection to one client
/// it decides if the client is a minecraft client or a proxy client
/// forwards the traffic to the other side
/// encapsulates/decapsulates the packets
pub async fn process_socket_connection(
    socket: TcpStream,
    distributor: Arc<Mutex<Distributor>>,
) -> Result<(), DistributorError> {
    let mut frames = Framed::new(socket, PacketCodec::new(1024 * 8));
    // In a loop, read data from the socket and write the data back.
    let packet = frames.next().await.ok_or(DistributorError::UnknownError(
        "could not read first packet".to_string(),
    ))?;
    let packet = packet.map_err(distributor_error!("could not read packet"))?;

    match packet {
        SocketPacket::MCHello(packet) => {
            let mut client = MCClient::new(distributor.clone(), frames, packet.clone()).await?;
            tracing::info!("distributor: {}", distributor.lock().await);

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
            let mut client = ProxyClient::new(distributor.clone(), frames, packet).await?;

            let response = client.handle().await;
            client.close_connection().await;
            response?;
        }
        _ => {
            tracing::error!("Unknown protocol");
        }
    };

    Ok(())
}
