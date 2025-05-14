use anyhow::anyhow;
use std::{
    io::{BufReader, BufWriter},
    net::TcpStream,
};
use tracing::*;

use crate::handler::{LibPqReader, LibPqWriter};
use crate::message::*;

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

    pub fn md5_authentication_handler(&mut self) -> anyhow::Result<()> {
        // StartupMessage (ssl_mode ) prefer => Text Auth
        self.tcp_writer.put_request(StartupMessage::new(
            ProtocolVersion { major: 3, minor: 0 },
            vec![
                ParameterStatus::new(&(String::from("user")), &(String::from("md5user")))?,
                ParameterStatus::new(&(String::from("database")), &(String::from("postgres")))?,
                ParameterStatus::new(
                    &(String::from("application_name")),
                    &(String::from("pgfake")),
                )?,
                ParameterStatus::new(&(String::from("client_encoding")), &(String::from("utf8")))?,
            ],
        ))?;

        let mut raw_message = self.tcp_reader.get_raw_backend_message()?;
        match AuthenticationMD5Password::try_from(&mut raw_message) {
            Ok(message) => {
                debug!("rcv: {message:?}");
                self.tcp_writer
                    .put_message_and_flush(PasswordMessage::new_from_user_password(
                        &"md5user".to_string(),
                        &"md5pass".to_string(),
                        &message.salt,
                    )?)?;
            }
            Err(_) => return Err(anyhow!("AuthenticationMD5Password message expected")),
        }

        let mut raw_message = self.tcp_reader.get_raw_backend_message()?;
        match AuthenticationOk::try_from(&mut raw_message) {
            Ok(message) => debug!("rcv: {message:?}"),
            _ => return Err(anyhow!("AuthenticationOk message expected")),
        };

        let mut raw_message = self.tcp_reader.get_raw_backend_message()?;
        while let BackendMessageKind::ParameterStatus =
            BackendMessageKind::try_from(&raw_message.kind)?
        {
            debug!("rcv: {:?}", ParameterStatus::try_from(&mut raw_message)?);

            raw_message = self.tcp_reader.get_raw_backend_message()?;
        }

        match BackendKeyData::try_from(&mut raw_message) {
            Ok(message) => debug!("rcv: {message:?}"),
            _ => return Err(anyhow!("BackendKeyData message expected")),
        }

        let mut raw_message = self.tcp_reader.get_raw_backend_message()?;
        match ReadyForQuery::try_from(&mut raw_message) {
            Ok(message) => debug!("rcv: {message:?}"),
            _ => return Err(anyhow!("ReadyForQuery message expected")),
        }

        Ok(())
    }

    pub fn simple_query_handler(&mut self) -> anyhow::Result<()> {
        //FIXME: This is not a complete implementation
        //* several queries could be ran in a single query message resulting
        //  in several RowDesc + DataRow
        //* queries result in (RowDesc + DataRow + CommandComplete)|Error
        //* commands result in CommandCompelte|Error
        //* NoticeResponse should be supported
        //* use a state machine as proposed by the documentation?

        self.tcp_writer
            .put_message_and_flush(Query::new("SELECT 1 as a, 2 as a, 3 as a;".to_string())?)?;

        let mut raw_message = self.tcp_reader.get_raw_backend_message()?;
        match RowDescription::try_from(&mut raw_message) {
            Ok(message) => debug!("rcv: {message:?}"),
            _ => return Err(anyhow!("RowDescription message expected")),
        }

        let mut raw_message = self.tcp_reader.get_raw_backend_message()?;
        match DataRow::try_from(&mut raw_message) {
            Ok(message) => debug!("rcv: {message:?}"),
            _ => return Err(anyhow!("DataRow message expected")),
        }

        let mut raw_message = self.tcp_reader.get_raw_backend_message()?;
        match CommandComplete::try_from(&mut raw_message) {
            Ok(message) => debug!("rcv: {message:?}"),
            _ => return Err(anyhow!("CommandComplete message expected")),
        }

        let mut raw_message = self.tcp_reader.get_raw_backend_message()?;
        match ReadyForQuery::try_from(&mut raw_message) {
            Ok(message) => debug!("rcv: {message:?}"),
            _ => return Err(anyhow!("ReadyForQuery message expected")),
        }

        Ok(())
    }
}
