use std::net::{TcpListener, TcpStream};
use std::thread;
use tracing::*;
use tracing_subscriber;

use fakepostmaster::handler::passthru::*;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_thread_ids(true)
        .compact()
        .init();

    let listener = TcpListener::bind("192.168.121.1:9092").unwrap();
    info!("Listening on 192.168.121.1:9092");

    for cli_stream in listener.incoming() {
        match cli_stream {
            Ok(cli_stream) => {
                info!("Accepted new connection from client");
                thread::spawn(|| {
                    info!("Connecting to server: pgsrv:5432...");
                    match TcpStream::connect("pgsrv:5432") {
                        Ok(srv_stream) => {
                            info!("Connection established");
                            match PassThruMachine::connect(srv_stream, cli_stream) {
                                Ok(mut handler) => loop {
                                    match handler.next() {
                                        Ok(Context::Disconnected) => break,
                                        Ok(_) => (),
                                        Err(e) => {
                                            error!("{e}");
                                            break;
                                        }
                                    }
                                },
                                Err(e) => {
                                    error!("{e}");
                                }
                            };
                        }
                        Err(e) => {
                            error!("error: {}", e);
                        }
                    }
                });
            }
            Err(e) => {
                error!("error: {}", e);
            }
        }
        info!("Request processed");
    }
    Ok(())
}
