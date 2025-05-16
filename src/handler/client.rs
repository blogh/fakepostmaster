use std::net::TcpStream;
use tracing::*;

use libpq_serde_types::Deserialize;

use crate::handler::{LibPqReader, LibPqWriter};
use crate::logical_message::*;
use crate::message::*;
use crate::streaming_message::*;

#[derive(Debug, Clone)]
pub enum Message {
    Frontend(FrontendMessageKind),
    Backend(BackendMessageKind),
}

#[derive(Debug, Clone, Copy)]
pub enum Context {
    Connected,
    Disconnected,
    Authentication,
    Query(Option<(QType, QState)>),
}

#[derive(Debug, Clone, Copy)]
pub enum QType {
    Query,
    CopyIn,
    CopyOut,
    CopyBoth,
}

#[derive(Debug, Clone, Copy)]
pub enum QState {
    Submitted,
    SendData,
    CommandComplete,
}

#[derive(Debug, Clone, Copy)]
pub struct StreamingData {
    start_lsn: i64,
    last_commit_lsn: i64,
}

#[derive(Debug)]
pub struct ClientMachine {
    last_message_kind: Option<Message>,
    context: Context,
    tcp_stream: TcpStream,
    user: String,
    password: String,
    database: String,
    application_name: String,
    query: Vec<String>,
    streaming_data: Option<StreamingData>,
}

impl ClientMachine {
    pub fn connect(
        mut tcp_stream: TcpStream,
        user: &str,
        password: &str,
        database: &str,
        replication: &str,
        application_name: &str,
        query: Vec<String>,
    ) -> anyhow::Result<Self> {
        let user = user.to_string();
        let password = password.to_string();
        let database = database.to_string();
        let replication = replication.to_string();
        let application_name = application_name.to_string();
        let mut query = query;
        query.reverse();

        tcp_stream.put_request(StartupMessage::new(
            ProtocolVersion { major: 3, minor: 0 },
            vec![
                ParameterStatus::new(&(String::from("user")), &user)?,
                ParameterStatus::new(&(String::from("database")), &database)?,
                ParameterStatus::new(&(String::from("application_name")), &application_name)?,
                ParameterStatus::new(&(String::from("replication")), &replication)?,
                ParameterStatus::new(&(String::from("client_encoding")), &(String::from("utf8")))?,
            ],
        ))?;

        Ok(Self {
            last_message_kind: None,
            context: Context::Authentication,
            tcp_stream,
            user,
            password,
            database,
            application_name,
            query,
            streaming_data: None,
        })
    }

    pub fn next(&mut self) -> anyhow::Result<Context> {
        let (current_message_kind, mut raw_message) = self.get_message()?;

        match (
            &self.last_message_kind,
            &current_message_kind,
            &self.context,
        ) {
            // The next message should be PasswordMessage .. so we send it
            (
                None,
                Message::Backend(BackendMessageKind::AuthenticationMD5Password),
                Context::Authentication,
            ) => {
                let message = AuthenticationMD5Password::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
                self.tcp_stream
                    .put_message(PasswordMessage::new_from_user_password(
                        &self.user,
                        &self.password,
                        &message.salt,
                    )?)?;
            }

            // BackendKeyData hold secret data send from the server and the process PID
            // The next message should be sent by the backend: ReadyForQuery
            (
                Some(_),
                Message::Backend(BackendMessageKind::BackendKeyData),
                Context::Authentication,
            ) => {
                // do something with the data
                let message = BackendKeyData::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
            }

            // ReadyForQuery could be received from the authentication context or the query
            // context, After that we wait for the frontend to send us a Query message.
            //
            //
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
                Message::Backend(BackendMessageKind::ReadyForQuery),
                _
            ) => {
                // We create the Query message and send it
                self.context = Context::Query(Some((QType::Query, QState::Submitted)));
                match self.query.pop() {
                    // We still have queries to process
                    Some(query) => {
                        self.tcp_stream.put_message(Query::new(query)?)?;
                    }
                    // No more queries: exit
                    None => {
                        self.context = Context::Disconnected;
                    }
                };
            }

            // RowDescription means we will send the result to the client. The next message
            // should be DataRow or CommandComplete sent from the backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::RowDescription),
                Context::Query(Some((QType::Query, QState::Submitted)))
            ) => {
                self.context = Context::Query(Some((QType::Query, QState::SendData)));
                let message = RowDescription::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
            }

            // DataRow is one row of data sent by the backend
            (
                Some(_),
                Message::Backend(BackendMessageKind::DataRow),
                Context::Query(Some((QType::Query, QState::SendData))),
            ) => {
                let message = DataRow::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
            }

            // CommandComplete marks the end of the query. The next message should be
            // ReadyForQuery sent by the backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CommandComplete),
                Context::Query(Some((query_type, _))),
            ) => {
                self.context = Context::Query(Some((*query_type, QState::CommandComplete)));
            }

            // CopyOutResponse is sent after receiving a COPY TO STDOUT. The next message
            // should be a CopyData also sent by the Backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyOutResponse),
                Context::Query(Some((QType::Query, QState::Submitted)))
            ) => {
                self.context = Context::Query(Some((QType::CopyOut, QState::SendData)));
                let message = CopyOutResponse::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
            }

            // CopyInResponse is sent after receiving a COPY FROM STDIN. The next message
            // should be a CopyData also sent by the Backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyInResponse),
                Context::Query(Some((QType::Query, QState::Submitted)))
            ) => {
                self.context = Context::Query(Some((QType::CopyIn, QState::SendData)));
                let message = CopyInResponse::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
            }

            // We are streaming data from CopyIn or CopyOut. The next message
            // should also come from the backend en be either CopyData or CommandComplete.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyData),
                Context::Query(Some((QType::CopyIn, QState::SendData)))
            ) |
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyData),
                Context::Query(Some((QType::CopyOut, QState::SendData)))
            ) => {
                let message = CopyData::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
            }

            // CopyBothResponse is sent after receiving a request to stream data with
            // the logical replication. The next message should be a CopyData also sent
            // by the Backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyBothResponse),
                Context::Query(Some((QType::Query, QState::Submitted)))
            ) => {
                self.context = Context::Query(Some((QType::CopyBoth, QState::SendData)));
                self.streaming_data = Some(StreamingData { start_lsn: lsn_create(0, 24276488), last_commit_lsn: lsn_create(0, 24276488) });
                let message = CopyBothResponse::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
            }

            // We are streaming data from CopyBoth. The next message should also come from the
            // backend en be either CopyData or CopyDone.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyData),
                Context::Query(Some((QType::CopyBoth, QState::SendData)))
            ) => {
                self.streaming_replication(&mut raw_message)?;
            }

            // We are done streaming data from CopyBoth. The next message should also come from
            // the backend en be CommandComplete.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyDone),
                Context::Query(Some((QType::CopyBoth, QState::SendData)))
            ) => {
                self.streaming_data = None;
            }

            // Async message can happend anytime, they dont change the context
            // except if they are ment to interrupt the flow of message.

            // ParameterStatus is typically sent in the Authentication Context
            // but it can be sent anytime if some client specific setting are modified
            // on the backend.
            (Some(_), Message::Backend(BackendMessageKind::ParameterStatus), _) => {
                //TODO: store or update parameters
                let message = ParameterStatus::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
            }

            // In Authentication context, ErrorResponse is followed by a deconnection
            (_, Message::Backend(BackendMessageKind::ErrorResponse), Context::Authentication) => {
                self.context = Context::Disconnected;
                let message = ErrorResponse::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:#?}");
            }

            // .. Otherwise we return it to the client en continue
            (_, Message::Backend(BackendMessageKind::ErrorResponse), _) => {
                self.context = Context::Disconnected;
                let message = ErrorResponse::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:#?}");
            }

            (_, Message::Backend(BackendMessageKind::NoticeResponse), _) => {
                let message = NoticeResponse::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:#?}");
            }

            // The Frontend can terminate gracefully the connection with Terminate
            (_, Message::Frontend(FrontendMessageKind::Terminate), _) => {
                self.context = Context::Disconnected;
            }

            // All acceptable messages
            (Some(_), _, _) => (),

            // And the obvious error
            (_, _, _) => {
                error!(
                    "Unexpected message type: {:?}\nlast message: {:?}\ncontext: {:?}",
                    current_message_kind, self.last_message_kind, self.context
                );
                self.context = Context::Disconnected;
            }
        }

        self.last_message_kind = Some(current_message_kind);
        Ok(self.context.clone())
    }

    fn get_message(&mut self) -> anyhow::Result<(Message, RawMessage)> {
        let message = self.tcp_stream.get_raw_backend_message()?;
        Ok((
            Message::Backend(BackendMessageKind::try_from(&message.kind)?),
            message,
        ))
    }

    fn streaming_replication(&mut self, raw_message: &mut RawMessage) -> anyhow::Result<()> {
        if let BackendMessageKind::CopyData = BackendMessageKind::try_from(&raw_message.kind)? {
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
                            let inner = self
                                .streaming_data
                                .as_mut()
                                .expect("Failed to access streaming_data");
                            inner.last_commit_lsn = message.commit_lsn;
                            debug!("DETAIL: {:?}", message,);
                        }
                        LogicalReplicationMessageKind::Insert => {
                            let message = Insert::deserialize(&mut raw_message.raw_body)?;
                            debug!("DETAIL: {:?}", message,);
                        }
                        LogicalReplicationMessageKind::Update => {
                            let message = Update::deserialize(&mut raw_message.raw_body)?;
                            debug!("DETAIL: {:?}", message,);
                        }
                        LogicalReplicationMessageKind::Delete => {
                            let message = Delete::deserialize(&mut raw_message.raw_body)?;
                            debug!("DETAIL: {:?}", message,);
                        }
                        _ => debug!(
                            "Unsupported message: {:?}",
                            LogicalReplicationMessageKind::try_from(header.message_type)?
                        ),
                    };
                }
                StreamingReplicationMessageKind::PrimaryKeepAliveMessage => {
                    let primary_keep_alive_message =
                        PrimaryKeepAliveMessage::deserialize(&mut raw_message.raw_body)?;
                    debug!("DETAIL: {:?}", primary_keep_alive_message,);

                    // let standby_status_update = StandbyStatusUpdate {
                    //     reveived_lsn: self.streaming_data.expect("streaming_data should be Some in streaming").start_lsn + 1,
                    //     flush_lsn: self.streaming_data.expect("streaming_data should be Some in streaming").start_lsn + 1,
                    //     applied_lsn: self.streaming_data.expect("streaming_data should be Some in streaming").start_lsn + 1,
                    //     timestamp: primary_keep_alive_message.timestamp + 1,
                    //     urgency: 0,
                    // };
                    // let copy_data = CopyData {};

                    // let mut buffer_body = BytesMut::new();
                    // copy_data.serialize(&mut buffer_body);
                    // StreamingHeader { message_type: standby_status_update.message_type() as i8 }.serialize(&mut buffer_body);
                    // standby_status_update.serialize(&mut buffer_body);

                    // let mut buffer_header = BytesMut::new();
                    // MessageHeader {
                    //     message_type: u8::from(&FrontendMessageKind::CopyData),
                    //     length: buffer_body.len() as i32 + 4
                    // }.serialize(&mut buffer_header);

                    // let raw_copy_data = RawMessage {
                    //     kind: RawMessageKind { main: u8::from(&FrontendMessageKind::CopyData), auth: None },
                    //     raw_header: buffer_header.into(),
                    //     raw_body: buffer_body.into(),
                    // };
                    // debug!("snd: StandbyStatusUpdate");

                    // self.tcp_stream.put_raw_message_and_flush(raw_copy_data)?;
                }
                _ => (),
            }
        } else {
            unreachable!("streaming_replication() only receives CopyData messages");
        }
        Ok(())
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
