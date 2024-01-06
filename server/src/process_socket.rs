use std::sync::Arc;
use futures::SinkExt;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;
use shared::addressing::{DistributorError, Register};
use shared::distributor_error;
use shared::packet_codec::PacketCodec;
use shared::socket_packet::SocketPacket;
use crate::client_handler::MCClient;
use crate::proxy_handler::ProxyClient;

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
            let proxy_tx = proxy_tx.ok_or(DistributorError::ServerNotFound(packet.hostname.clone()))?;

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