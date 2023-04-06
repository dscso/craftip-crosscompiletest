mod addressing;
mod client_handler;
mod cursor;
mod datatypes;
mod minecraft;
mod packet_codec;
mod proxy;
mod socket_packet;
mod test;

use tokio::net::TcpListener;

use crate::addressing::Distributor;
use std::error::Error;
use std::sync::Arc;
use std::{env, result};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(false)
        .with_target(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:25565".to_string());

    let mc_listener = TcpListener::bind(&addr).await?;
    tracing::info!("server running on {:?}", mc_listener.local_addr()?);

    let distributor = Arc::new(Mutex::new(Distributor::new()));
    loop {
        let (socket, _addr) = mc_listener.accept().await?;
        let distributor = Arc::clone(&distributor);
        tokio::spawn(async move {
            client_handler::process_socket_connection(socket, distributor)
                .await
                .expect("TODO: panic message");
        });
    }
}
