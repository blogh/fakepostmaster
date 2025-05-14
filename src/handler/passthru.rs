use anyhow::anyhow;
use std::{
    io::{BufReader, BufWriter},
    net::TcpStream,
};
use tracing::*;

use crate::handler::{LibPqReader, LibPqWriter};
use crate::message::*;

pub struct TcpHandler {
    // connection to the server
    pub srv_tcp_reader: BufReader<TcpStream>,
    pub srv_tcp_writer: BufWriter<TcpStream>,
    // connection from the client
    pub cli_tcp_reader: BufReader<TcpStream>,
    pub cli_tcp_writer: BufWriter<TcpStream>,
}

impl TcpHandler {
    pub fn new(srv_stream: TcpStream, cli_stream: TcpStream) -> anyhow::Result<Self> {
        Ok(Self {
            srv_tcp_reader: BufReader::new(
                srv_stream.try_clone().expect("Failed to clone TcpStream"),
            ),
            srv_tcp_writer: BufWriter::new(srv_stream),
            cli_tcp_reader: BufReader::new(
                cli_stream.try_clone().expect("Failed to clone TcpStream"),
            ),
            cli_tcp_writer: BufWriter::new(cli_stream),
        })
    }

    pub fn md5_authentication_handler(&mut self) -> anyhow::Result<()> {
        let sm = StartupMessage::try_from(&mut RawRequest::get(&mut self.cli_tcp_reader)?)?;
        debug!("cli (rcv & resent): {sm:?}");
        self.srv_tcp_writer.put_request(sm)?;

        let mut raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
        match AuthenticationMD5Password::try_from(&mut raw_message) {
            Ok(message) => {
                debug!("srv (rcv): {message:?}");
                self.cli_tcp_writer.put_message_and_flush(message)?;
            }
            Err(_) => return Err(anyhow!("AuthenticationMD5Password message expected")),
        }

        let mut raw_message = self.cli_tcp_reader.get_raw_frontend_message()?;
        let _password_message = match PasswordMessage::try_from(&mut raw_message) {
            Ok(message) => {
                debug!("cli rcv: {message:?}");
                self.srv_tcp_writer.put_message_and_flush(message)?;
            }
            _ => return Err(anyhow!("Password message expected")),
        };

        let mut raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
        match AuthenticationOk::try_from(&mut raw_message) {
            Ok(message) => {
                debug!("srv rcv: {message:?}");
                self.cli_tcp_writer.put_message(message)?;
            }
            _ => return Err(anyhow!("AuthenticationOk message expected")),
        };

        let mut raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
        while let Some(BackendMessageKind::ParameterStatus) = raw_message.get_message_kind() {
            let message = ParameterStatus::try_from(&mut raw_message)?;
            debug!("srv rcv: {:?}", message);
            self.cli_tcp_writer.put_message(message)?;

            raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
        }

        match BackendKeyData::try_from(&mut raw_message) {
            Ok(message) => {
                debug!("srv rcv: {message:?}");
                self.cli_tcp_writer.put_message(message)?;
            }
            _ => return Err(anyhow!("BackendKeyData message expected")),
        }

        let mut raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
        match ReadyForQuery::try_from(&mut raw_message) {
            Ok(message) => {
                debug!("srv rcv: {message:?}");
                self.cli_tcp_writer.put_message_and_flush(message)?;
            }
            _ => return Err(anyhow!("ReadyForQuery message expected")),
        }

        Ok(())
    }

    pub fn simple_query_handler(&mut self) -> anyhow::Result<()> {
        //FIXME: See handler/client.rs
        //NOTE: As is the perfs are as ugly as can be (+3/5) because we open
        // all the messages before repackaging them and sending them. This is not
        // necessary. It was just a trial.
        let mut raw_message = self.cli_tcp_reader.get_raw_frontend_message()?;
        match Query::try_from(&mut raw_message) {
            Ok(message) => {
                debug!("cli rcv: {message:?}");
                self.srv_tcp_writer.put_message_and_flush(message)?;
            }
            _ => {
                return Err(anyhow!(
                    "Query message expected, got {:?}",
                    raw_message.get_message_kind()
                ));
            }
        };

        let mut raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
        // Regular queries have a RowDescription and DataRow(s) but commands
        // like VACUUM dont't
        if let Ok(message) = RowDescription::try_from(&mut raw_message) {
            debug!("srv rcv: {message:?}");
            self.cli_tcp_writer.put_message(message)?;

            raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
            while let Some(BackendMessageKind::DataRow) = raw_message.get_message_kind() {
                let message = DataRow::try_from(&mut raw_message).expect("Must be a DataRow");
                debug!("srv rcv: {message:?}");
                self.cli_tcp_writer.put_message(message)?;

                raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
            }
        }

        match CommandComplete::try_from(&mut raw_message) {
            Ok(message) => {
                debug!("srv rcv: {message:?}");
                self.cli_tcp_writer.put_message(message)?;
            }
            _ => {
                return Err(anyhow!(
                    "Query message expected, got {:?}",
                    raw_message.get_message_kind()
                ));
            }
        }

        let mut raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
        match ReadyForQuery::try_from(&mut raw_message) {
            Ok(message) => {
                debug!("rcv: {message:?}");
                self.cli_tcp_writer.put_message_and_flush(message)?;
            }
            _ => {
                return Err(anyhow!(
                    "Query message expected, got {:?}",
                    raw_message.get_message_kind()
                ));
            }
        }

        Ok(())
    }
}
