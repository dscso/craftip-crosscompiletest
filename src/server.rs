mod client_handler;
mod cursor;
mod datatypes;
mod minecraft;
mod packet_codec;
mod proxy;
mod socket_packet;
mod test;

use tokio::net::TcpListener;

use std::env;
use std::error::Error;
use std::sync::Arc;
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

    let state = Arc::new(Mutex::new(client_handler::Shared::new()));
    loop {
        let (mut socket, addr) = mc_listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            client_handler::process_socket_connection(socket, addr, state)
                .await
                .expect("TODO: panic message");
        });
    }
}
