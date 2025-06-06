use clap::{ArgAction, Args, Parser, command};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use tracing::*;
use tracing_subscriber;

use fakepostmaster::handler::passthru::*;

// Example of a passthru application.

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
    #[clap(long, default_value = "127.0.0.1")]
    listen_host: String,
    #[clap(long, default_value = "9092")]
    listen_port: i32,
    #[clap(long, default_value = "127.0.0.1")]
    host: String,
    #[clap(long, default_value = "5432")]
    port: i32,
    #[clap(short, long, action = ArgAction::SetTrue, default_value_t = false)]
    debug: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_max_level(if cli.common_args.debug {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .with_thread_ids(true)
        .compact()
        .init();

    let listen_to = format!(
        "{}:{}",
        cli.common_args.listen_host, cli.common_args.listen_port
    );
    let listener = TcpListener::bind(&listen_to).unwrap();
    info!("Listening on {}", listen_to);

    let shared_conf = Arc::new(cli);
    for cli_stream in listener.incoming() {
        match cli_stream {
            Ok(cli_stream) => {
                info!("Accepted new connection from client",);
                let shared_conf = Arc::clone(&shared_conf);
                thread::spawn(move || {
                    let connect_to = format!(
                        "{}:{}",
                        shared_conf.common_args.host, shared_conf.common_args.port
                    );
                    info!("Connecting to server: {}...", connect_to);
                    match TcpStream::connect(connect_to) {
                        Ok(srv_stream) => {
                            info!("Connection established");
                            match PassThruMachine::connect(
                                srv_stream,
                                cli_stream,
                                shared_conf.common_args.anonymize,
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
