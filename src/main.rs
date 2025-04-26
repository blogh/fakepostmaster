#![allow(unused_imports)]

use anyhow::anyhow;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, BufWriter, Read, Seek, Write},
    net::TcpListener,
    thread,
};

mod backend;
mod bytes;
mod frontend;
mod handler;

use crate::backend::BackendMessage;
use crate::frontend::FrontendMessage;
use crate::handler::TcpHandler;

fn send_message<R>(tcp_writer: &mut BufWriter<R>, message: BackendMessage) -> anyhow::Result<()>
where
    R: Write,
{
    println!("Send: {message:?}");
    let buf = message.compose()?;
    tcp_writer.write(&buf)?;
    tcp_writer.flush()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind("192.168.121.1:9092").unwrap();
    println!("Listening on 192.168.121.1:9092");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                fn auth_func() -> bool {
                    true
                }

                println!("accepted new connection");
                let mut handler = TcpHandler::new(stream)?;
                let _connection_parameters = handler.md5_authentication_handler(&auth_func)?;

                // Query?
                let q = FrontendMessage::parse_query(&mut handler.tcp_reader)?;
                println!("Received: {q:#?}");

                // Tell the client the commadn tag
                send_message(
                    &mut handler.tcp_writer,
                    BackendMessage::CommmandComplete {
                        command_tag: String::from("SELECT 1"),
                    },
                )?;

                // Tell the client he can continue
                send_message(&mut handler.tcp_writer, BackendMessage::ReadyForQuery)?;
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
        println!("Request processed");
    }
    Ok(())
}
