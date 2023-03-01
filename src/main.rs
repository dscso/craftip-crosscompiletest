mod client_handler;
mod datatypes;
mod test;
mod minecraft_versions;
mod cursor;

use tokio::net::TcpListener;

use std::env;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::{Mutex};


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:25565".to_string());

    let mc_listener = TcpListener::bind(&addr).await?;
    println!("Listening on: {}", addr);

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
