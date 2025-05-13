use std::net::{TcpListener, TcpStream};
use tracing::*;
use tracing_subscriber;

use fakepostmaster::handler::passthru::TcpHandler;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .compact()
        .init();

    let listener = TcpListener::bind("192.168.121.1:9092").unwrap();
    info!("Listening on 192.168.121.1:9092");

    for cli_stream in listener.incoming() {
        match cli_stream {
            Ok(cli_stream) => {
                info!("Accepted new connection from client");

                info!("Connecting to server: pgsrv:5435...");
                match TcpStream::connect("pgsrv:5435") {
                    Ok(srv_stream) => {
                        info!("Connection established");
                        let mut handler = TcpHandler::new(srv_stream, cli_stream)?;
                        let _connection_parameters = handler.md5_authentication_handler()?;
                        loop {
                            //FIXME: with thiserror exit when appropriate
                            if handler.simple_query_handler().is_err() {
                                break;
                            }
                        }
                        info!("Connection ended");
                    }
                    Err(e) => {
                        println!("error: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("error: {}", e);
            }
        }
        info!("Request processed");
    }
    Ok(())
}
