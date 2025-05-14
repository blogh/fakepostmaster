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

        let raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
        match BackendMessageKind::try_from(&raw_message.kind)? {
            BackendMessageKind::AuthenticationMD5Password => {
                self.cli_tcp_writer.put_raw_message_and_flush(raw_message)?;
            }
            _ => return Err(anyhow!("AuthenticationMD5Password message expected")),
        }

        let raw_message = self.cli_tcp_reader.get_raw_frontend_message()?;
        match FrontendMessageKind::try_from(&raw_message.kind)? {
            FrontendMessageKind::ContextDependant => {
                self.srv_tcp_writer.put_raw_message_and_flush(raw_message)?
            }
            _ => return Err(anyhow!("Password message expected")),
        };

        let raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
        match BackendMessageKind::try_from(&raw_message.kind)? {
            BackendMessageKind::AuthenticationOk => {
                self.cli_tcp_writer.put_raw_message(raw_message)?;
            }
            _ => return Err(anyhow!("AuthenticationOk message expected")),
        };

        let mut raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
        while let BackendMessageKind::ParameterStatus =
            BackendMessageKind::try_from(&raw_message.kind)?
        {
            self.cli_tcp_writer.put_raw_message(raw_message)?;
            raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
        }

        match BackendMessageKind::try_from(&raw_message.kind)? {
            BackendMessageKind::BackendKeyData => {
                self.cli_tcp_writer.put_raw_message(raw_message)?;
            }
            _ => return Err(anyhow!("BackendKeyData message expected")),
        }

        let raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
        match BackendMessageKind::try_from(&raw_message.kind)? {
            BackendMessageKind::ReadyForQuery => {
                self.cli_tcp_writer.put_raw_message_and_flush(raw_message)?;
            }
            _ => return Err(anyhow!("ReadyForQuery message expected")),
        }

        Ok(())
    }

    pub fn simple_query_handler(&mut self) -> anyhow::Result<()> {
        //FIXME: See handler/client.rs
        let raw_message = self.cli_tcp_reader.get_raw_frontend_message()?;
        match FrontendMessageKind::try_from(&raw_message.kind)? {
            FrontendMessageKind::Query => {
                self.srv_tcp_writer.put_raw_message_and_flush(raw_message)?;
            }
            _ => {
                return Err(anyhow!(
                    "Query message expected, got {:?}",
                    raw_message.kind
                ));
            }
        };

        let mut raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
        // Regular queries have a RowDescription and DataRow(s) but commands
        // like VACUUM dont't
        if let BackendMessageKind::RowDescription = BackendMessageKind::try_from(&raw_message.kind)?
        {
            self.cli_tcp_writer.put_raw_message(raw_message)?;

            raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
            while let BackendMessageKind::DataRow = BackendMessageKind::try_from(&raw_message.kind)?
            {
                self.cli_tcp_writer.put_raw_message(raw_message)?;
                raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
            }
        }

        match BackendMessageKind::try_from(&raw_message.kind)? {
            BackendMessageKind::CommandComplete => {
                self.cli_tcp_writer.put_raw_message(raw_message)?;
            }
            _ => {
                return Err(anyhow!(
                    "Query message expected, got {:?}",
                    raw_message.kind
                ));
            }
        }

        let raw_message = self.srv_tcp_reader.get_raw_backend_message()?;
        match BackendMessageKind::try_from(&raw_message.kind)? {
            BackendMessageKind::ReadyForQuery => {
                self.cli_tcp_writer.put_raw_message_and_flush(raw_message)?;
            }
            _ => {
                return Err(anyhow!(
                    "Query message expected, got {:?}",
                    raw_message.kind
                ));
            }
        }

        Ok(())
    }
}
