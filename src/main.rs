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

use crate::backend::{BackendMessage, FieldData, FieldDescription, PgType};
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
                fn executor(query: String) -> (Vec<FieldDescription>, Vec<FieldData>, String) {
                    let row_description = vec![FieldDescription::new(
                        String::from("Custom Field"),
                        PgType::Text,
                    )];
                    let row_data = vec![FieldData::new_text(&String::from("my data"))];
                    let row_data = Vec::new();
                    let command_tag = String::from("SELECT 0");

                    (row_description, row_data, command_tag)
                }

                println!("accepted new connection");
                let mut handler = TcpHandler::new(stream)?;
                let _connection_parameters = handler.md5_authentication_handler(&auth_func)?;

                loop {
                    handler.simple_query_handler(&executor)?;
                }
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
        println!("Request processed");
    }
    Ok(())
}
