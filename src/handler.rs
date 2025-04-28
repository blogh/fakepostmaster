use anyhow::anyhow;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, BufWriter, Read, Seek, Write},
    net::TcpStream,
    thread,
};

use crate::backend::{BackendMessage, ErrorMessage, FieldData, FieldDescription};
use crate::frontend::FrontendMessage;

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

pub struct TcpHandler {
    pub tcp_reader: BufReader<TcpStream>,
    pub tcp_writer: BufWriter<TcpStream>,
}

impl TcpHandler {
    pub fn new(stream: TcpStream) -> anyhow::Result<Self> {
        Ok(Self {
            tcp_reader: BufReader::new(stream.try_clone().expect("Failed to clone TcpStream")),
            tcp_writer: BufWriter::new(stream),
        })
    }

    pub fn md5_authentication_handler(
        &mut self,
        auth_function: &dyn Fn() -> bool,
    ) -> anyhow::Result<HashMap<String, String>> {
        // StartupMessage (ssl_mode ) prefer => Text Auth
        let sm = FrontendMessage::parse_startup_message(&mut self.tcp_reader)?;
        println!("Received: {sm:#?}");
        let parameters = match sm {
            FrontendMessage::StartupMessage {
                length,
                protocol_version,
                parameters,
            } => parameters,
            _ => unreachable!("Something went horribly wrong here .."),
        };

        // Ask for the Password
        send_message(
            &mut self.tcp_writer,
            BackendMessage::AuthenticationMD5Password { salt: [1, 2, 3, 4] },
        )?;

        // PasswordMessage
        let pm = FrontendMessage::parse_password_message(&mut self.tcp_reader)?;
        println!("Received: {pm:#?}");

        if auth_function() {
            // Validate the authentication
            send_message(&mut self.tcp_writer, BackendMessage::AuthenticationOk)?;

            // Validate the authentication
            //FIXME: There should me much mode parameters to send back to the client..
            send_message(
                &mut self.tcp_writer,
                BackendMessage::ParameterStatus {
                    parameter: String::from("server_version"),
                    value: String::from("0.1 (fakepostmaster)"),
                },
            )?;

            // Tell the client he can continue
            send_message(&mut self.tcp_writer, BackendMessage::ReadyForQuery)?;

            Ok(parameters)
        } else {
            // Error out
            send_message(
                &mut self.tcp_writer,
                BackendMessage::ErrorResponse {
                    messages: vec![ErrorMessage {
                        code: 'M',
                        message: String::from("Incorrect password or user"),
                    }],
                },
            )?;

            Err(anyhow!("Auth failed"))
        }
    }

    pub fn simple_query_handler(
        &mut self,
        executor: &dyn Fn(String) -> (Vec<FieldDescription>, Vec<FieldData>, String),
    ) -> anyhow::Result<()> {
        // Query?
        let q = FrontendMessage::parse_query(&mut self.tcp_reader)?;
        println!("Received: {q:#?}");
        let query = match q {
            FrontendMessage::Query {
                kind,
                length,
                query,
            } => query,
            _ => unreachable!("Something went horribly wrong here .."),
        };

        // execute query
        let (column_desc, column_data, command_tag) = executor(query);

        // row description
        send_message(
            &mut self.tcp_writer,
            BackendMessage::RowDescription {
                columns: column_desc,
            },
        )?;

        // data row
        if column_data.len() > 0 {
            send_message(
                &mut self.tcp_writer,
                BackendMessage::DataRow {
                    columns: column_data,
                },
            )?;
        }

        // Tell the client the commadn tag
        send_message(
            &mut self.tcp_writer,
            BackendMessage::CommmandComplete { command_tag },
        )?;

        // Tell the client he can continue
        send_message(&mut self.tcp_writer, BackendMessage::ReadyForQuery)?;

        Ok(())
    }
}
