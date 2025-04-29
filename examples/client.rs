use bytes::BytesMut;
use std::net::TcpStream;
use std::{ffi::CString, net::TcpListener};

use fakepostmaster::handler::client::TcpHandler;
use fakepostmaster::message::{ColumnData, ColumnDescription, PgType};
use libpq_serde_types::{
    Serialize,
    libpq_types::{Byte, Vec32},
};

fn main() -> anyhow::Result<()> {
    let stream = TcpStream::connect("pgsrv:5435");
    println!("Connecting to pgsrv:5435");

    match stream {
        Ok(stream) => {
            println!("Connection established");
            let mut handler = TcpHandler::new(stream)?;

            handler.md5_authentication_handler()?;
        }
        Err(e) => {
            println!("error: {}", e);
        }
    }
    Ok(())
}
