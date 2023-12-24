use std::error::Error;

mod client;
mod cursor;
mod datatypes;
mod minecraft;
mod packet_codec;
mod proxy;
mod socket_packet;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    /*let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(false)
        .with_target(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    // Connect to server 1
    let server1_addr = "127.0.0.1:25565";

    // Connect to server 2
    let mc_server_addr = "127.0.0.1:25564";*/
    println!("hello world");
    Ok(())
}
