use std::net::TcpStream;
use tracing::*;
use tracing_subscriber;

use fakepostmaster::handler::client::TcpHandler;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .compact()
        .init();

    let stream = TcpStream::connect("pgsrv:5435");
    info!("Connecting to pgsrv:5435...");

    match stream {
        Ok(stream) => {
            info!("Connection established");
            let mut handler = TcpHandler::new(stream)?;

            handler.md5_authentication_handler()?;
            handler.simple_query_handler()?;
            info!("Connection ended");
        }
        Err(e) => {
            println!("error: {}", e);
        }
    }
    Ok(())
}
