use bytes::BytesMut;
use std::{ffi::CString, net::TcpListener};

use fakepostmaster::handler::server::TcpHandler;
use fakepostmaster::message::{ColumnData, ColumnDescription, PgType};
use libpq_serde_types::{
    Serialize,
    libpq_types::{Byte, Vec32},
};

fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind("192.168.121.1:9092").unwrap();
    println!("Listening on 192.168.121.1:9092");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                fn auth_func() -> bool {
                    true
                }
                fn executor(query: String) -> (Vec<ColumnDescription>, Vec<ColumnData>, String) {
                    let mut row_description = Vec::new();
                    row_description.push(
                        ColumnDescription::new(&String::from("Custom Field"), PgType::Text)
                            .expect("Dont care"),
                    );
                    let mut buffer = BytesMut::new();
                    let col_data =
                        CString::new(String::from("my data")).expect("No 0x00 in strings");
                    col_data.serialize(&mut buffer);
                    let col_data: Vec32<Byte> = buffer.to_vec().into();
                    let row_data = vec![col_data];

                    //let row_data = Vec::new();
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
