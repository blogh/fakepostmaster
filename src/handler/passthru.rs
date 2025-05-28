use anyhow::anyhow;
use bytes::{Bytes, BytesMut};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::time::Duration;
use tracing::*;

use libpq_serde_types::{Deserialize, Serialize};

//FIXME: use super::
use crate::handler::{LibPqReader, LibPqWriter, PgToRustTypes, PgType, decode_from_text};
use crate::message::logical_message::*;
use crate::message::message::*;
use crate::message::streaming_message::*;

#[derive(Debug, Clone)]
pub enum Message {
    Frontend(FrontendMessageKind),
    Backend(BackendMessageKind),
}

#[derive(Debug, Clone)]
pub enum Context {
    Connected,
    Disconnected,
    Authentication,
    ReadyForQuery,
    QTextSubmitted(String),
    SimpleQuery(QState),
    CopyIn(CIState),
    CopyOut(COState),
    CopyBoth(CBState),
}

#[derive(Debug, Clone)]
pub enum QState {
    Data(String),
    Done,
}

#[derive(Debug, Clone)]
pub enum CIState {
    Data,
    Done,
}

#[derive(Debug, Clone)]
pub enum COState {
    Data,
    Done,
}

#[derive(Debug, Clone)]
pub enum CBState {
    Data(CBData),
    Done,
}

#[derive(Debug, Clone)]
pub struct CBData {
    start_lsn: i64,
    last_commit_lsn: i64,
    relation_cache: HashMap<i32, CBRelation>,
}

#[derive(Debug, Clone)]
pub struct CBRelation {
    pub relation: String,
    pub schema: String,
    pub columns: Vec<CBColDesc>,
}

impl TryFrom<&crate::message::logical_message::Relation> for CBRelation {
    type Error = anyhow::Error;
    fn try_from(value: &crate::message::logical_message::Relation) -> anyhow::Result<CBRelation> {
        let relation: String = value.relname.clone().into_string()?;
        let schema: String = value.namespace.clone().into_string()?;
        let mut columns = Vec::<CBColDesc>::new();

        for col in value.columns.iter() {
            columns.push(CBColDesc::try_from(col)?);
        }

        Ok(Self {
            relation,
            schema,
            columns,
        })
    }
}

impl TryFrom<crate::message::logical_message::Relation> for CBRelation {
    type Error = anyhow::Error;
    fn try_from(value: crate::message::logical_message::Relation) -> anyhow::Result<CBRelation> {
        let relation: String = value.relname.into_string()?;
        let schema: String = value.namespace.into_string()?;
        let mut columns = Vec::<CBColDesc>::new();

        for col in value.columns.into_iter() {
            columns.push(CBColDesc::try_from(col)?);
        }

        Ok(Self {
            relation,
            schema,
            columns,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CBColDesc {
    pub name: String,
    pub pg_type: PgType,
}

impl TryFrom<&crate::message::logical_message::ColumnDescription> for CBColDesc {
    type Error = anyhow::Error;
    fn try_from(
        value: &crate::message::logical_message::ColumnDescription,
    ) -> anyhow::Result<CBColDesc> {
        //FIXME: have standart name for things like type_oid
        Ok(Self {
            name: value.name.clone().into_string()?,
            pg_type: PgType::try_from(value.type_oid)?,
        })
    }
}

impl TryFrom<crate::message::logical_message::ColumnDescription> for CBColDesc {
    type Error = anyhow::Error;
    fn try_from(
        value: crate::message::logical_message::ColumnDescription,
    ) -> anyhow::Result<CBColDesc> {
        Ok(Self {
            name: value.name.into_string()?,
            pg_type: PgType::try_from(value.type_oid)?,
        })
    }
}
#[derive(Debug, Clone, Copy)]
pub enum ReadFrom {
    Frontend,       // Read from frontend wait if necessary
    Backend,        // Read from backend wait if necessary
    PreferFrontend, // Check on backend then read from frontend and wait if necessary
    PreferBackend,  // Check on frontend then read from backend and wait if necessary
}

#[derive(Debug)]
pub struct PassThruMachine {
    last_message_kind: Option<Message>,
    context: Context,
    read_from: ReadFrom,
    // use TcpStreams because BufReader doesn't have a has_data_left() outside of nightly rust
    be_stream: TcpStream,
    fe_stream: TcpStream,
    user: Option<String>,
    database: Option<String>,
    anonymize: bool,
    application_name: Option<String>,
    client_parameters: HashMap<String, String>,
    server_parameters: HashMap<String, String>,
}

impl PassThruMachine {
    pub fn connect(
        mut be_stream: TcpStream,
        mut fe_stream: TcpStream,
        anonymize: bool,
    ) -> anyhow::Result<Self> {
        //be_stream.set_read_timeout(Some(Duration::from_millis(100)))?;
        //fe_stream.set_read_timeout(Some(Duration::from_millis(100)))?;

        //TODO: extract parameter data
        let raw_startup_request = StartupMessage::try_from(&mut RawRequest::get(&mut fe_stream)?)?;
        let startup_message = StartupMessage::from(raw_startup_request.clone());
        be_stream.put_request(raw_startup_request)?;

        //FIXME:replace the expects
        Ok(Self {
            last_message_kind: None,
            context: Context::Authentication,
            read_from: ReadFrom::Backend,
            be_stream,
            fe_stream,
            user: None,
            database: None,
            application_name: None,
            anonymize,
            client_parameters: HashMap::<String, String>::try_from(&startup_message)?,
            server_parameters: HashMap::<String, String>::new(),
        })
    }

    pub fn next(&mut self) -> anyhow::Result<Context> {
        let (current_message_kind, mut raw_message) = self.get_message()?;

        match (
            &self.last_message_kind,
            &current_message_kind,
            &mut self.context,
        ) {
            // The next message should be PasswordMessage, we read it from the frontend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                None,
                Message::Backend(BackendMessageKind::AuthenticationMD5Password),
                Context::Authentication,
            ) => {
                self.fe_stream.put_raw_message(raw_message)?;
                self.read_from = ReadFrom::Frontend;
            }

            // Password message's code is p which interpretation is context dependant
            // One received we expect an ErrorResponse or AuthenticationOk from the
            // backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Frontend(FrontendMessageKind::ContextDependant),
                Context::Authentication,
            ) => {
                // It's PasswordMessage
                self.be_stream.put_raw_message(raw_message)?;
                self.read_from = ReadFrom::Backend;
            }

            // BackendKeyData hold secret data send from the server and the process PID
            // The next message should be sent by the backend: ReadyForQuery
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::BackendKeyData),
                Context::Authentication,
            ) => {
                let message = BackendKeyData::try_from(&mut raw_message.clone())?;
                self.server_parameters
                    .insert("process_id".to_string(), message.process_id.to_string());
                self.server_parameters
                    .insert("secret_key".to_string(), message.secret_key.to_string());
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // ReadyForQuery could be received from the authentication context or the query
            // context, After that we wait for the frontend to send us a query.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::ReadyForQuery),
                _,
            ) => {
                self.context = Context::ReadyForQuery;
                self.fe_stream.put_raw_message(raw_message)?;
                self.read_from = ReadFrom::Frontend;
            }

            // Query contains the query sent from the frontend, it can contain a regular query
            // or a streaming replication specific one. The next message will be sent by the
            // backend:
            // * CommandComplete: if it's a command (BEGIN, ROLLBACK, COMMIT, REINDEX ..)
            // * RowDescription: if it's a query that return's rows (SELECT, UPDATE .. RETURNING ..)
            // * CopyIn: if it's a COPY FROM STDIN
            // * CopyOut: if it's a COPY TO STDOUT
            // * CopyBoth: if we are streaming data from the brackend
            //
            // Asynchronous message could come instread eg: NoticeResponse with VACUMM
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Frontend(FrontendMessageKind::Query),
                Context::ReadyForQuery,
            ) => {
                let msg = Query::try_from(&mut raw_message.clone())?;
                let query = msg.query.into_string()?;
                debug!("DETAIL: query: {:}", query);
                self.context = Context::QTextSubmitted(query);
                self.be_stream.put_raw_message(raw_message)?;
                self.read_from = ReadFrom::Backend;
            }

            // RowDescription means we will send the result to the client. The next message
            // should be DataRow or CommandComplete sent by the backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::RowDescription),
                Context::QTextSubmitted(query),
            ) => {
                self.context = Context::SimpleQuery(QState::Data(query.to_owned()));
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // DataRow is one row of data sent by the backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::DataRow),
                Context::SimpleQuery(QState::Data(query))
            ) => {
                self.context = Context::SimpleQuery(QState::Data(query.to_owned()));
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // CommandComplete marks the end of the query. The next message should be
            // ReadyForQuery sent by the backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CommandComplete),
                Context::SimpleQuery(QState::Data(_)),
            ) => {
                self.context = Context::SimpleQuery(QState::Done);
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // CopyOutResponse is sent after receiving a COPY TO STDOUT. The next message
            // should be a CopyData also sent by the Backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyOutResponse),
                Context::QTextSubmitted(_query)
            ) => {
                self.context = Context::CopyOut(COState::Data);
                let message = CopyOutResponse::try_from(&mut raw_message.clone())?;
                debug!("DETAIL: {message:?}");
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // We are streaming data from CopyOut. The next message should also come from the
            // backend en be either CopyData or CopyDone
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyData),
                Context::CopyOut(COState::Data)
            ) => {
                self.context = Context::CopyOut(COState::Data);
                let message = CopyData::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // CopyOut is done The next message should be Commandcomplete.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyDone),
                Context::CopyOut(COState::Data)
            ) => {
                self.context = Context::CopyOut(COState::Done);
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // CopyInResponse is sent after receiving a COPY FROM STDIN. The next message
            // should be a CopyData sent by the frontend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyInResponse),
                Context::QTextSubmitted(_query)
            ) => {
                self.context = Context::CopyIn(CIState::Data);
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // We are streaming data from CopyIn. The next message
            // should come from the frontend and be either CopyData or CopyDone.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Frontend(FrontendMessageKind::CopyData),
                Context::CopyIn(CIState::Data)
            ) => {
                self.context = Context::CopyIn(CIState::Data);
                self.be_stream.put_raw_message(raw_message)?;
            }

            // CopyIn is done The next message should be Commandcomplete.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Frontend(FrontendMessageKind::CopyDone),
                Context::CopyIn(CIState::Data)
            ) => {
                self.context = Context::CopyIn(CIState::Done);
                self.be_stream.put_raw_message(raw_message)?;
            }

            // CopyBothResponse is sent after receiving a request to stream data with
            // the logical replication. The next message should be a CopyData also sent
            // by the Backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyBothResponse),
                Context::QTextSubmitted(_query)
            ) => {
                self.context = Context::CopyBoth(CBState::Data( CBData { start_lsn: 0, last_commit_lsn: 0, relation_cache: HashMap::new() }));
                let message = CopyBothResponse::try_from(&mut raw_message.clone())?;
                debug!("DETAIL: {message:?}");
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // We are streaming data from the backend in CopyBoth. The next message should
            // also come from the backend en be either CopyData or CopyDone.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyData),
                Context::CopyBoth(CBState::Data(_))
            ) => {
                if self.streaming_replication(raw_message)? {
                    self.read_from = ReadFrom::PreferBackend;
                }
            },

            // We are streaming data from the frontend in CopyBoth. The next message should
            // come from the frontend en be either CopyData or CopyDone.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Frontend(FrontendMessageKind::CopyData),
                Context::CopyBoth(CBState::Data(_))
            ) => {
                self.streaming_replication(raw_message)?;
            }

            // We are done streaming data from CopyBoth. The next message should also come from
            // the backend and be CommandComplete.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CommandComplete),
                Context::CopyBoth(CBState::Data(_))
            ) => {
                self.context = Context::CopyBoth(CBState::Done);
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // Async message can happend anytime, they can change the state
            // of the machine il they are ment to interrupt the flow of
            // message but it's not requiered

            // ParameterStatus is typically sent in the Authentication Context
            // but it can be sent anytime if some client specific setting are modified
            // on the backend.
            (Some(_), Message::Backend(BackendMessageKind::ParameterStatus), _) => {
                let message = ParameterStatus::try_from(&mut raw_message.clone())?;
                debug!("DETAIL: {message:?}");
                self.server_parameters
                    .insert(message.name.into_string()?, message.value.into_string()?);
                self.fe_stream.put_raw_message(raw_message)?;
                // No state change
            }

            // In Authentication context, ErrorResponse is followed by a deconnection
            (_, Message::Backend(BackendMessageKind::ErrorResponse), Context::Authentication) => {
                let message = ErrorResponse::try_from(&mut raw_message.clone())?;
                debug!("DETAIL: {message:#?}");
                self.context = Context::Disconnected;
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // .. Otherwise we return it to the client en continue
            (_, Message::Backend(BackendMessageKind::ErrorResponse), _) => {
                let message = ErrorResponse::try_from(&mut raw_message.clone())?;
                debug!("DETAIL: {message:#?}");
                self.fe_stream.put_raw_message(raw_message)?;
            }

            (_, Message::Backend(BackendMessageKind::NoticeResponse), _) => {
                let message = NoticeResponse::try_from(&mut raw_message.clone())?;
                debug!("DETAIL: {message:#?}");
                self.fe_stream.put_raw_message(raw_message)?;
                // No state change
            }

            // The Frontend can terminate gracefully the connection with Terminate
            (_, Message::Frontend(FrontendMessageKind::Terminate), _) => {
                self.be_stream.put_raw_message(raw_message)?;
                self.context = Context::Disconnected;
            }

            // All acceptable message have to be sent to the appropriate target
            (Some(_), Message::Frontend(_), _) => {
                self.be_stream.put_raw_message(raw_message)?;
            }
            (Some(_), Message::Backend(_), _) => {
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // And the error
            (_, _, _) => {
                error!(
                    "Unexpected message type: {:?} last message: {:?} context: {:?}",
                    current_message_kind, self.last_message_kind, self.context
                );
                self.context = Context::Disconnected;
            }
        }

        self.last_message_kind = Some(current_message_kind);
        Ok(self.context.clone())
    }

    fn get_message(&mut self) -> anyhow::Result<(Message, RawMessage)> {
        match self.read_from {
            ReadFrom::Backend => {
                let message = self.be_stream.get_raw_backend_message()?;
                Ok((
                    Message::Backend(BackendMessageKind::try_from(&message.kind)?),
                    message,
                ))
            }
            ReadFrom::Frontend => {
                let message = self.fe_stream.get_raw_frontend_message()?;
                Ok((
                    Message::Frontend(FrontendMessageKind::try_from(&message.kind)?),
                    message,
                ))
            }
            ReadFrom::PreferBackend => {
                self.fe_stream
                    .set_read_timeout(Some(Duration::from_millis(200)))?;
                let message = self.fe_stream.get_raw_frontend_message();
                self.fe_stream.set_read_timeout(None)?;

                match message {
                    Ok(message) => {
                        self.read_from = ReadFrom::Backend;
                        Ok((
                            Message::Frontend(FrontendMessageKind::try_from(&message.kind)?),
                            message,
                        ))
                    }
                    Err(e) => match e.downcast_ref::<std::io::Error>() {
                        Some(io_error) => match io_error.kind() {
                            ErrorKind::TimedOut | ErrorKind::WouldBlock => {
                                let message = self.be_stream.get_raw_backend_message()?;
                                Ok((
                                    Message::Backend(BackendMessageKind::try_from(&message.kind)?),
                                    message,
                                ))
                            }
                            _ => Err(e),
                        },
                        _ => Err(e),
                    },
                }
            }
            _ => unimplemented!(),
        }
    }

    fn streaming_replication(&mut self, mut raw_message: RawMessage) -> anyhow::Result<bool> {
        let mut needs_feedback = false;

        // we can try from FrontendMessageKind::CopyData or BackendMessageKind it doesn't matter
        // the header is te same.
        if let BackendMessageKind::CopyData = BackendMessageKind::try_from(&raw_message.kind)? {
            let saved_raw_message = raw_message.clone();

            let xlog_data_header = StreamingHeader::deserialize(&mut raw_message.raw_body)?;
            debug!(
                "DETAIL: {:?} {:?}",
                raw_message.kind,
                StreamingReplicationMessageKind::try_from(xlog_data_header.message_type)?,
            );

            match StreamingReplicationMessageKind::try_from(xlog_data_header.message_type)? {
                StreamingReplicationMessageKind::XLogData => {
                    let xlog_data_body = XLogData::deserialize(&mut raw_message.raw_body)?;
                    debug!("DETAIL: {:?}", xlog_data_body,);

                    let header = LogicalHeader::deserialize(&mut raw_message.raw_body)?;
                    match LogicalReplicationMessageKind::try_from(header.message_type)? {
                        LogicalReplicationMessageKind::Relation => {
                            let message = Relation::deserialize(&mut raw_message.raw_body)?;
                            debug!("DETAIL: {:?}", message,);
                        }
                        LogicalReplicationMessageKind::Begin => {
                            let message = Begin::deserialize(&mut raw_message.raw_body)?;
                            debug!("DETAIL: {:?}", message,);
                        }
                        LogicalReplicationMessageKind::Commit => {
                            let message = Commit::deserialize(&mut raw_message.raw_body)?;
                            debug!("DETAIL: {:?}", message,);
                        }
                        LogicalReplicationMessageKind::Insert => {
                            let mut insert_message =
                                Insert::deserialize(&mut raw_message.raw_body)?;
                            debug!("DETAIL: {:?}", insert_message,);

                            if self.anonymize {
                                // modify the message
                                let new_value = match decode_from_text(
                                    &insert_message.new_tuple_data.columns[0].column_value,
                                    &PgType::Int8,
                                ) {
                                    Ok(PgToRustTypes::Int8(value)) => value,
                                    _ => return Err(anyhow!("Incompatible type")),
                                };

                                let new_value = new_value * -10;
                                insert_message.new_tuple_data.columns[0].column_value =
                                    Bytes::from(new_value.to_string());
                                debug!("DETAIL: Anonymize {:?}", insert_message,);

                                // create a new one and send it
                                self.fe_stream.put_raw_message(
                                    create_logical_replication_message(
                                        &xlog_data_body,
                                        &insert_message,
                                    ),
                                )?;

                                return Ok(false);
                            }
                        }
                        LogicalReplicationMessageKind::Update => {
                            let mut update_message =
                                Update::deserialize(&mut raw_message.raw_body)?;
                            debug!("DETAIL: {:?}", update_message,);

                            if self.anonymize {
                                let new_value = match decode_from_text(
                                    &update_message.new_tuple_data.columns[0].column_value,
                                    &PgType::Int8,
                                ) {
                                    Ok(PgToRustTypes::Int8(value)) => value,
                                    _ => return Err(anyhow!("Incompatible type")),
                                };

                                let old_value = match decode_from_text(
                                    &update_message.old_tuple_data.columns[0].column_value,
                                    &PgType::Int8,
                                ) {
                                    Ok(PgToRustTypes::Int8(value)) => value,
                                    _ => return Err(anyhow!("Incompatible type")),
                                };

                                // modify the message
                                let new_value = new_value * -10;
                                update_message.new_tuple_data.columns[0].column_value =
                                    Bytes::from(new_value.to_string());

                                let old_value = old_value * -10;
                                update_message.old_tuple_data.columns[0].column_value =
                                    Bytes::from(old_value.to_string());

                                debug!("DETAIL: Anonymize {:?}", update_message,);

                                // create a new one and send it
                                self.fe_stream.put_raw_message(
                                    create_logical_replication_message(
                                        &xlog_data_body,
                                        &update_message,
                                    ),
                                )?;

                                return Ok(false);
                            }
                        }
                        LogicalReplicationMessageKind::Delete => {
                            let mut delete_message =
                                Delete::deserialize(&mut raw_message.raw_body)?;
                            debug!("DETAIL: {:?}", delete_message,);

                            if self.anonymize {
                                let old_value = match decode_from_text(
                                    &delete_message.old_tuple_data.columns[0].column_value,
                                    &PgType::Int8,
                                ) {
                                    Ok(PgToRustTypes::Int8(value)) => value,
                                    _ => return Err(anyhow!("Incompatible type")),
                                };

                                let old_value = old_value * -10;
                                delete_message.old_tuple_data.columns[0].column_value =
                                    Bytes::from(old_value.to_string());

                                debug!("DETAIL: Anonymized {:?}", delete_message,);

                                // create a new one and send it
                                self.fe_stream.put_raw_message(
                                    create_logical_replication_message(
                                        &xlog_data_body,
                                        &delete_message,
                                    ),
                                )?;

                                return Ok(false);
                            }
                        }
                        _ => debug!(
                            "Unsupported message: {:?}",
                            LogicalReplicationMessageKind::try_from(header.message_type)?
                        ),
                    };
                    self.fe_stream.put_raw_message(saved_raw_message)?;
                }
                StreamingReplicationMessageKind::PrimaryKeepAliveMessage => {
                    let primary_keep_alive_message =
                        PrimaryKeepAliveMessage::deserialize(&mut raw_message.raw_body)?;
                    debug!("DETAIL: {:?}", primary_keep_alive_message,);
                    needs_feedback = true;

                    self.fe_stream.put_raw_message(saved_raw_message)?;
                }
                StreamingReplicationMessageKind::StandbyStatusUpdate => {
                    let standby_system_update_message =
                        StandbyStatusUpdate::deserialize(&mut raw_message.raw_body)?;
                    debug!("DETAIL: {:?}", standby_system_update_message,);

                    self.be_stream.put_raw_message(saved_raw_message)?;
                }
                _ => (),
            }
        } else {
            unreachable!("streaming_replication() only uses CopyData messages");
        }

        Ok(needs_feedback)
    }
}

fn lsn_split(value: i64) -> (i32, i32) {
    let upper = (value >> 32) as i32;
    let lower = value as u32 as i32;
    (upper, lower)
}

fn lsn_create(upper: i32, lower: i32) -> i64 {
    (upper as i64) << 32 | (lower as i64)
}

fn create_logical_replication_message<T>(
    xlog_data_body: &XLogData,
    logical_message_body: &T,
) -> RawMessage
where
    T: Serialize + MessageBody,
{
    // Reconstruct the streaming message
    let mut buffer_body = BytesMut::new();
    StreamingHeader {
        message_type: xlog_data_body.message_type() as i8,
    }
    .serialize(&mut buffer_body);
    xlog_data_body.serialize(&mut buffer_body);

    // Reconstruct the logical message
    LogicalHeader {
        message_type: logical_message_body.message_type() as i8,
    }
    .serialize(&mut buffer_body);
    logical_message_body.serialize(&mut buffer_body);

    // Reconstruct CopyData message
    let mut buffer_header = BytesMut::new();
    MessageHeader {
        message_type: u8::from(&BackendMessageKind::CopyData),
        length: buffer_body.len() as i32 + 4,
    }
    .serialize(&mut buffer_header);

    RawMessage {
        kind: RawMessageKind {
            main: u8::from(&FrontendMessageKind::CopyData),
            auth: None,
        },
        raw_header: buffer_header.into(),
        raw_body: buffer_body.into(),
    }
}
