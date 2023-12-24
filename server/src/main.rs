use std::env;
use std::error::Error;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::Mutex;

use shared::addressing::{Distributor, DistributorError};

mod client_handler;

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

    let distributor = Arc::new(Mutex::new(Distributor::default()));
    loop {
        let (socket, _addr) = mc_listener.accept().await?;
        let distributor = Arc::clone(&distributor);
        tokio::spawn(async move {
            match client_handler::process_socket_connection(socket, distributor).await {
                Ok(_) => tracing::info!("client disconnected"),
                Err(DistributorError::UnknownError(err)) => {
                    tracing::error!("client error: {}", err)
                }
                Err(e) => {
                    tracing::info!("client error: {:?}", e);
                }
            }
        });
    }
}
