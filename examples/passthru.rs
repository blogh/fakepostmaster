use clap::{ArgAction, Args, Parser, command};
use std::net::{TcpListener, TcpStream};
use std::thread;
use tracing::*;
use tracing_subscriber;

use fakepostmaster::handler::passthru::*;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    common_args: CommonArgs,
}

#[derive(Args, Debug)]
struct CommonArgs {
    #[clap(short, long, action = ArgAction::SetTrue, default_value_t = false)]
    anonymize: bool,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_thread_ids(true)
        .compact()
        .init();

    let cli = Cli::parse();
    let listener = TcpListener::bind("192.168.121.1:9092").unwrap();
    info!("Listening on 192.168.121.1:9092");

    for cli_stream in listener.incoming() {
        match cli_stream {
            Ok(cli_stream) => {
                info!("Accepted new connection from client");
                thread::spawn(move || {
                    info!("Connecting to server: pgsrv:5432...");
                    match TcpStream::connect("pgsrv:5432") {
                        Ok(srv_stream) => {
                            info!("Connection established");
                            match PassThruMachine::connect(
                                srv_stream,
                                cli_stream,
                                cli.common_args.anonymize,
                            ) {
                                Ok(mut handler) => loop {
                                    match handler.next() {
                                        Ok(Context::Disconnected) => break,
                                        Ok(_) => (),
                                        Err(e) => {
                                            error!("error in handler: {e}");
                                            break;
                                        }
                                    }
                                },
                                Err(e) => {
                                    error!("error during handler initialization: {e}");
                                }
                            };
                        }
                        Err(e) => {
                            error!("error while connecting to pgsrv:5432: {e}");
                        }
                    }
                });
            }
            Err(e) => {
                error!("error while waiting for connection: {e}");
            }
        }
        info!("Request processed");
    }
    Ok(())
}
