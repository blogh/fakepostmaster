use anyhow::anyhow;
use bytes::Bytes;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::time::Duration;
use tracing::*;

use libpq_serde_types::Deserialize;

//FIXME: use super::
use crate::handler::{PgToRustTypes, PgType, decode_from_text};
use crate::message::logical::*;
use crate::message::streaming::*;
use crate::message::*;

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

impl TryFrom<&Relation> for CBRelation {
    type Error = anyhow::Error;
    fn try_from(value: &Relation) -> anyhow::Result<CBRelation> {
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

impl TryFrom<Relation> for CBRelation {
    type Error = anyhow::Error;
    fn try_from(value: Relation) -> anyhow::Result<CBRelation> {
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

impl TryFrom<&RColumnDescription> for CBColDesc {
    type Error = anyhow::Error;
    fn try_from(value: &RColumnDescription) -> anyhow::Result<CBColDesc> {
        //FIXME: have standart name for things like type_oid
        Ok(Self {
            name: value.name.clone().into_string()?,
            pg_type: PgType::try_from(value.type_oid)?,
        })
    }
}

impl TryFrom<RColumnDescription> for CBColDesc {
    type Error = anyhow::Error;
    fn try_from(value: RColumnDescription) -> anyhow::Result<CBColDesc> {
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
        let mut raw_startup_request = RawMessage::<RequestType>::receive(&mut fe_stream)?;
        raw_startup_request.send(&mut be_stream);
        let startup_message = StartupMessage::try_from(&mut raw_startup_request)?;

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
                raw_message.send(&mut self.fe_stream)?;
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
                raw_message.send(&mut self.be_stream)?;
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
                raw_message.send(&mut self.fe_stream)?;
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
                raw_message.send(&mut self.fe_stream)?;
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
                raw_message.send(&mut self.be_stream)?;
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
                raw_message.send(&mut self.fe_stream)?;
            }

            // DataRow is one row of data sent by the backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::DataRow),
                Context::SimpleQuery(QState::Data(query))
            ) => {
                self.context = Context::SimpleQuery(QState::Data(query.to_owned()));
                raw_message.send(&mut self.fe_stream)?;
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
                raw_message.send(&mut self.fe_stream)?;
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
                raw_message.send(&mut self.fe_stream)?;
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
                raw_message.send(&mut self.fe_stream)?;
            }

            // CopyOut is done The next message should be Commandcomplete.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyDone),
                Context::CopyOut(COState::Data)
            ) => {
                self.context = Context::CopyOut(COState::Done);
                raw_message.send(&mut self.fe_stream)?;
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
                raw_message.send(&mut self.fe_stream)?;
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
                raw_message.send(&mut self.be_stream)?;
            }

            // CopyIn is done The next message should be Commandcomplete.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Frontend(FrontendMessageKind::CopyDone),
                Context::CopyIn(CIState::Data)
            ) => {
                self.context = Context::CopyIn(CIState::Done);
                raw_message.send(&mut self.be_stream)?;
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
                raw_message.send(&mut self.fe_stream)?;
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
                raw_message.send(&mut self.fe_stream)?;
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
                raw_message.send(&mut self.fe_stream)?;
                // No state change
            }

            // In Authentication context, ErrorResponse is followed by a deconnection
            (_, Message::Backend(BackendMessageKind::ErrorResponse), Context::Authentication) => {
                let message = ErrorResponse::try_from(&mut raw_message.clone())?;
                debug!("DETAIL: {message:#?}");
                self.context = Context::Disconnected;
                raw_message.send(&mut self.fe_stream)?;
            }

            // .. Otherwise we return it to the client en continue
            (_, Message::Backend(BackendMessageKind::ErrorResponse), _) => {
                let message = ErrorResponse::try_from(&mut raw_message.clone())?;
                debug!("DETAIL: {message:#?}");
                raw_message.send(&mut self.fe_stream)?;
            }

            (_, Message::Backend(BackendMessageKind::NoticeResponse), _) => {
                let message = NoticeResponse::try_from(&mut raw_message.clone())?;
                debug!("DETAIL: {message:#?}");
                raw_message.send(&mut self.fe_stream)?;
                // No state change
            }

            // The Frontend can terminate gracefully the connection with Terminate
            (_, Message::Frontend(FrontendMessageKind::Terminate), _) => {
                raw_message.send(&mut self.be_stream)?;
                self.context = Context::Disconnected;
            }

            // All acceptable message have to be sent to the appropriate target
            (Some(_), Message::Frontend(_), _) => {
                raw_message.send(&mut self.be_stream)?;
            }
            (Some(_), Message::Backend(_), _) => {
                raw_message.send(&mut self.fe_stream)?;
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

    fn get_message(&mut self) -> anyhow::Result<(Message, RawMessage<MessageType>)> {
        match self.read_from {
            ReadFrom::Backend => {
                let message = RawMessage::<MessageType>::receive(&mut self.be_stream)?;
                Ok((
                    Message::Backend(BackendMessageKind::try_from(&message.mtype)?),
                    message,
                ))
            }
            ReadFrom::Frontend => {
                let message = RawMessage::<MessageType>::receive(&mut self.fe_stream)?;
                Ok((
                    Message::Frontend(FrontendMessageKind::try_from(&message.mtype)?),
                    message,
                ))
            }
            ReadFrom::PreferBackend => {
                self.fe_stream
                    .set_read_timeout(Some(Duration::from_millis(200)))?;
                let message = RawMessage::<MessageType>::receive(&mut self.fe_stream);
                self.fe_stream.set_read_timeout(None)?;

                match message {
                    Ok(message) => {
                        self.read_from = ReadFrom::Backend;
                        Ok((
                            Message::Frontend(FrontendMessageKind::try_from(&message.mtype)?),
                            message,
                        ))
                    }
                    Err(e) => match e.downcast_ref::<std::io::Error>() {
                        Some(io_error) => match io_error.kind() {
                            ErrorKind::TimedOut | ErrorKind::WouldBlock => {
                                let message =
                                    RawMessage::<MessageType>::receive(&mut self.be_stream)?;
                                Ok((
                                    Message::Backend(BackendMessageKind::try_from(&message.mtype)?),
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

    fn streaming_replication(
        &mut self,
        mut raw_message: RawMessage<MessageType>,
    ) -> anyhow::Result<bool> {
        let mut needs_feedback = false;

        // we can try from FrontendMessageKind::CopyData or BackendMessageKind it doesn't matter
        // the header is te same.
        if let BackendMessageKind::CopyData = BackendMessageKind::try_from(&raw_message.mtype)? {
            let saved_raw_message = raw_message.clone();

            let xlog_data_header = StreamingHeader::deserialize(&mut raw_message.body)?;
            debug!(
                "DETAIL: {:?} {:?}",
                raw_message.mtype,
                StreamingReplicationMessageKind::try_from(xlog_data_header.message_type)?,
            );

            match StreamingReplicationMessageKind::try_from(xlog_data_header.message_type)? {
                StreamingReplicationMessageKind::XLogData => {
                    let xlog_data_body = XLogData::deserialize(&mut raw_message.body)?;
                    debug!("DETAIL: {:?}", xlog_data_body,);

                    let header = LogicalHeader::deserialize(&mut raw_message.body)?;
                    match LogicalReplicationMessageKind::try_from(header.message_type)? {
                        LogicalReplicationMessageKind::Relation => {
                            let message = Relation::deserialize(&mut raw_message.body)?;
                            debug!("DETAIL: {:?}", message,);
                        }
                        LogicalReplicationMessageKind::Begin => {
                            let message = Begin::deserialize(&mut raw_message.body)?;
                            debug!("DETAIL: {:?}", message,);
                        }
                        LogicalReplicationMessageKind::Commit => {
                            let message = Commit::deserialize(&mut raw_message.body)?;
                            debug!("DETAIL: {:?}", message,);
                        }
                        LogicalReplicationMessageKind::Insert => {
                            let mut insert_message = Insert::deserialize(&mut raw_message.body)?;
                            debug!("DETAIL: {:?}", insert_message,);

                            if self.anonymize {
                                // modify the message
                                let new_value = match decode_from_text(
                                    &insert_message.new_tuple_data.data[0].column_value,
                                    &PgType::Int8,
                                ) {
                                    Ok(PgToRustTypes::Int8(value)) => value,
                                    _ => return Err(anyhow!("Incompatible type")),
                                };

                                let new_value = new_value * -10;
                                insert_message.new_tuple_data.data[0].column_value =
                                    Bytes::from(new_value.to_string());

                                debug!("DETAIL: Anonymize {:?}", insert_message,);

                                // create a new one and send it
                                MessageBuilder::new_backend_message()
                                    .copy_data()
                                    .xlog_data(xlog_data_body)
                                    .insert(insert_message)
                                    .into_raw_message()
                                    .send(&mut self.fe_stream)?;

                                return Ok(false);
                            }
                        }
                        LogicalReplicationMessageKind::Update => {
                            let mut update_message = Update::deserialize(&mut raw_message.body)?;
                            debug!("DETAIL: {:?}", update_message,);

                            if self.anonymize {
                                // get the new value
                                let new_value = match decode_from_text(
                                    &update_message.new_tuple_data.data[0].column_value,
                                    &PgType::Int8,
                                ) {
                                    Ok(PgToRustTypes::Int8(value)) => value,
                                    _ => return Err(anyhow!("Incompatible type")),
                                };

                                // modify the new value in the message
                                let new_value = new_value * -10;
                                update_message.new_tuple_data.data[0].column_value =
                                    Bytes::from(new_value.to_string());

                                // get the old value
                                match update_message.old_tuple_data {
                                    ReplicaIdentity::Old(ref mut tuple) => {
                                        let old_value = match decode_from_text(
                                            &tuple.data[0].column_value,
                                            &PgType::Int8,
                                        ) {
                                            Ok(PgToRustTypes::Int8(value)) => value,
                                            _ => return Err(anyhow!("Incompatible type")),
                                        };

                                        let old_value = old_value * -10;
                                        tuple.data[0].column_value =
                                            Bytes::from(old_value.to_string());

                                        dbg!("Old {tuple}");
                                    }
                                    ReplicaIdentity::Key(ref mut tuple) => {
                                        let old_value = match decode_from_text(
                                            &tuple.data[0].column_value,
                                            &PgType::Int8,
                                        ) {
                                            Ok(PgToRustTypes::Int8(value)) => value,
                                            _ => return Err(anyhow!("Incompatible type")),
                                        };

                                        let old_value = old_value * -10;
                                        tuple.data[0].column_value =
                                            Bytes::from(old_value.to_string());

                                        dbg!("Key {tuple}");
                                    }
                                    ReplicaIdentity::None => (),
                                }

                                debug!("DETAIL: Anonymize {:?}", update_message,);

                                // create a new one and send it
                                MessageBuilder::new_backend_message()
                                    .copy_data()
                                    .xlog_data(xlog_data_body)
                                    .update(update_message)
                                    .into_raw_message()
                                    .send(&mut self.fe_stream)?;

                                return Ok(false);
                            }
                        }
                        LogicalReplicationMessageKind::Delete => {
                            let mut delete_message = Delete::deserialize(&mut raw_message.body)?;
                            debug!("DETAIL: {:?}", delete_message,);

                            if self.anonymize {
                                // get the old value
                                match delete_message.old_tuple_data {
                                    ReplicaIdentity::Old(ref mut tuple) => {
                                        let old_value = match decode_from_text(
                                            &tuple.data[0].column_value,
                                            &PgType::Int8,
                                        ) {
                                            Ok(PgToRustTypes::Int8(value)) => value,
                                            _ => return Err(anyhow!("Incompatible type")),
                                        };

                                        let old_value = old_value * -10;
                                        tuple.data[0].column_value =
                                            Bytes::from(old_value.to_string());

                                        dbg!("Old {tuple}");
                                    }
                                    ReplicaIdentity::Key(ref mut tuple) => {
                                        let old_value = match decode_from_text(
                                            &tuple.data[0].column_value,
                                            &PgType::Int8,
                                        ) {
                                            Ok(PgToRustTypes::Int8(value)) => value,
                                            _ => return Err(anyhow!("Incompatible type")),
                                        };

                                        let old_value = old_value * -10;
                                        tuple.data[0].column_value =
                                            Bytes::from(old_value.to_string());

                                        dbg!("Key {tuple}");
                                    }
                                    ReplicaIdentity::None => (),
                                }

                                debug!("DETAIL: Anonymize {:?}", delete_message,);

                                // create a new one and send it
                                MessageBuilder::new_backend_message()
                                    .copy_data()
                                    .xlog_data(xlog_data_body)
                                    .delete(delete_message)
                                    .into_raw_message()
                                    .send(&mut self.fe_stream)?;

                                return Ok(false);
                            }
                        }
                        LogicalReplicationMessageKind::Truncate => {
                            let truncate_message = Truncate::deserialize(&mut raw_message.body)?;
                            debug!("DETAIL: {:?}", truncate_message,);
                        }
                        _ => debug!(
                            "Unsupported message: {:?}",
                            LogicalReplicationMessageKind::try_from(header.message_type)?
                        ),
                    };
                    saved_raw_message.send(&mut self.fe_stream)?;
                }
                StreamingReplicationMessageKind::PrimaryKeepAliveMessage => {
                    let primary_keep_alive_message =
                        PrimaryKeepAliveMessage::deserialize(&mut raw_message.body)?;
                    debug!("DETAIL: {:?}", primary_keep_alive_message,);
                    needs_feedback = true;

                    saved_raw_message.send(&mut self.fe_stream)?;
                }
                StreamingReplicationMessageKind::StandbyStatusUpdate => {
                    let standby_system_update_message =
                        StandbyStatusUpdate::deserialize(&mut raw_message.body)?;
                    debug!("DETAIL: {:?}", standby_system_update_message,);

                    saved_raw_message.send(&mut self.be_stream)?;
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
