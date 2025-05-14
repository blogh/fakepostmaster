use anyhow::anyhow;
use libpq_serde_types::Deserialize;
use std::{
    io::{BufReader, BufWriter},
    net::TcpStream,
};
use tracing::*;

use crate::handler::{LibPqReader, LibPqWriter};
use crate::logical_message::*;
use crate::message::*;
use crate::streaming_message::*;

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
                ParameterStatus::new(&(String::from("user")), &(String::from("md5userrl")))?,
                ParameterStatus::new(&(String::from("database")), &(String::from("postgres")))?,
                ParameterStatus::new(&(String::from("replication")), &(String::from("database")))?,
                //ParameterStatus::new(&(String::from("replication")), &(String::from("database")))?,
                ParameterStatus::new(
                    &(String::from("application_name")),
                    &(String::from("pgfake")),
                )?,
                ParameterStatus::new(&(String::from("client_encoding")), &(String::from("utf8")))?,
            ],
        ))?;

        // Receive Athentication message from server
        //let mut raw_message = RawBackendMessage::get(&mut self.tcp_reader)?;
        let mut raw_message = self.tcp_reader.get_raw_backend_message()?;
        debug!("{:?}", raw_message.kind);
        match AuthenticationMD5Password::try_from(&mut raw_message) {
            Ok(message) => {
                debug!("rcv: {message:?}");
                self.tcp_writer
                    .put_message_and_flush(PasswordMessage::new_from_user_password(
                        &"md5userrl".to_string(),
                        &"md5passrl".to_string(),
                        &message.salt,
                    )?)?;
            }
            Err(_) => return Err(anyhow!("AuthenticationMD5Password message expected")),
        }

        // Receive Authentication Ok
        let mut raw_message = self.tcp_reader.get_raw_backend_message()?;
        match AuthenticationOk::try_from(&mut raw_message) {
            Ok(message) => debug!("rcv: {message:?}"),
            _ => return Err(anyhow!("AuthenticationOk message expected")),
        };

        // ParameterStatus Messages
        let mut raw_message = self.tcp_reader.get_raw_backend_message()?;
        while let BackendMessageKind::ParameterStatus =
            BackendMessageKind::try_from(&raw_message.kind)?
        {
            debug!("rcv: {:?}", ParameterStatus::try_from(&mut raw_message)?);

            raw_message = self.tcp_reader.get_raw_backend_message()?;
        }

        // BackendKeyData
        match BackendKeyData::try_from(&mut raw_message) {
            Ok(message) => debug!("rcv: {message:?}"),
            _ => return Err(anyhow!("BackendKeyData message expected")),
        }

        // ReadyForQuery
        let mut raw_message = self.tcp_reader.get_raw_backend_message()?;
        match ReadyForQuery::try_from(&mut raw_message) {
            Ok(message) => debug!("rcv: {message:?}"),
            _ => return Err(anyhow!("ReadyForQuery message expected")),
        }

        Ok(())
    }

    pub fn simple_query_handler(&mut self) -> anyhow::Result<()> {
        // https://www.postgresql.org/docs/17/protocol-replication.html#PROTOCOL-REPLICATION-START-REPLICATION-SLOT-LOGICAL
        // https://www.postgresql.org/docs/17/protocol-logical-replication.html
        // postgres=# SELECT  pg_create_logical_replication_slot('slot', 'pgoutput');
        //
        //  pg_create_logical_replication_slot
        // ------------------------------------
        //  (slot,0/1726E08)
        // (1 row)

        self.tcp_writer.put_message_and_flush(Query::new(format!(
            r#"
            START_REPLICATION SLOT {} LOGICAL {} (
                "proto_version" '{}',
                "publication_names" '{}',
                "streaming" 'off'
            )
            "#,
            "slot", "0/1726E08", 2, "pub",
        ))?)?;

        let mut raw_message = self.tcp_reader.get_raw_backend_message()?;
        match CopyBothResponse::try_from(&mut raw_message) {
            Ok(message) => debug!("rcv: {message:?}"),
            _ => return Err(anyhow!("CopyBothResponse message expected")),
        }

        let mut raw_message = self.tcp_reader.get_raw_backend_message()?;
        while let BackendMessageKind::CopyData = BackendMessageKind::try_from(&raw_message.kind)? {
            let header = StreamingHeader::deserialize(&mut raw_message.raw_body)?;
            debug!(
                "rcv: {:?} {:?}",
                raw_message.kind,
                StreamingReplicationMessageKind::try_from(header.message_type)?,
            );

            match StreamingReplicationMessageKind::try_from(header.message_type)? {
                StreamingReplicationMessageKind::XLogData => {
                    let message = XLogData::deserialize(&mut raw_message.raw_body)?;
                    debug!("{:?}", message,);

                    let header = LogicalHeader::deserialize(&mut raw_message.raw_body)?;
                    match LogicalReplicationMessageKind::try_from(header.message_type)? {
                        LogicalReplicationMessageKind::Relation => {
                            let message = Relation::deserialize(&mut raw_message.raw_body)?;
                            debug!("{:?}", message,);
                        }
                        LogicalReplicationMessageKind::Begin => {
                            let message = Begin::deserialize(&mut raw_message.raw_body)?;
                            debug!("{:?}", message,);
                        }
                        LogicalReplicationMessageKind::Commit => {
                            let message = Commit::deserialize(&mut raw_message.raw_body)?;
                            debug!("{:?}", message,);
                        }
                        LogicalReplicationMessageKind::Insert => {
                            let message = Insert::deserialize(&mut raw_message.raw_body)?;
                            debug!("{:?}", message,);
                        }
                        _ => debug!(
                            "Unsupported message: {:?}",
                            LogicalReplicationMessageKind::try_from(header.message_type)?
                        ),
                    };
                }
                StreamingReplicationMessageKind::PrimaryKeepAliveMessage => {
                    let message = PrimaryKeepAliveMessage::deserialize(&mut raw_message.raw_body)?;
                    debug!("{:?}", message,);
                }
                _ => (),
            }

            raw_message = self.tcp_reader.get_raw_backend_message()?;
        }

        let raw_message = self.tcp_reader.get_raw_backend_message()?;
        info!("{:?}", raw_message.kind);

        Ok(())
    }
}
