use std::io::ErrorKind;
use std::net::TcpStream;
use std::time::Duration;
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
    application_name: Option<String>,
}

impl PassThruMachine {
    pub fn connect(mut be_stream: TcpStream, mut fe_stream: TcpStream) -> anyhow::Result<Self> {
        //be_stream.set_read_timeout(Some(Duration::from_millis(100)))?;
        //fe_stream.set_read_timeout(Some(Duration::from_millis(100)))?;

        //TODO: extract parameter data
        let startup_message = StartupMessage::try_from(&mut RawRequest::get(&mut fe_stream)?)?;
        be_stream.put_request(startup_message)?;

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
        })
    }

    pub fn next(&mut self) -> anyhow::Result<Context> {
        let (current_message_kind, raw_message) = self.get_message()?;

        match (
            &self.last_message_kind,
            &current_message_kind,
            &self.context,
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
                //TODO: Store this data
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
                self.context = Context::Query(None);
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
                Context::Query(None),
            ) => {
                self.context = Context::Query(Some((QType::Query, QState::Submitted)));
                let msg = Query::try_from(&mut raw_message.clone())?;
                debug!("DETAIL: query: {:}", msg.query.into_string()?);
                self.be_stream.put_raw_message(raw_message)?;
                self.read_from = ReadFrom::Backend;
            }

            // RowDescription means we will send the result to the client. The next message
            // should be DataRow or CommandComplete sent by the backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::RowDescription),
                Context::Query(Some((QType::Query, QState::Submitted))),
            ) => {
                self.context = Context::Query(Some((QType::Query, QState::SendData)));
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // DataRow is one row of data sent by the backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::DataRow),
                Context::Query(Some((QType::Query, QState::SendData)))
            ) => {
                let message = DataRow::try_from(&mut raw_message.clone())?;
                debug!("DETAIL: {message:?}");
                self.fe_stream.put_raw_message(raw_message)?;
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
                self.fe_stream.put_raw_message(raw_message)?;
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
                let message = CopyOutResponse::try_from(&mut raw_message.clone())?;
                debug!("DETAIL: {message:?}");
                self.fe_stream.put_raw_message(raw_message)?;
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
                let message = CopyInResponse::try_from(&mut raw_message.clone())?;
                debug!("DETAIL: {message:?}");
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // We are streaming data from CopyIn CopyOut or CopyBoth. The next message
            // should also come from the backend en be either CopyData or CopyDone.
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
                let message = CopyData::try_from(&mut raw_message.clone())?;
                debug!("DETAIL: {message:?}");
                self.fe_stream.put_raw_message(raw_message)?;
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
                Context::Query(Some((QType::CopyBoth, QState::SendData)))
            ) => {
                if self.streaming_replication(&mut raw_message.clone())? {
                    self.read_from = ReadFrom::PreferBackend;
                }
                self.fe_stream.put_raw_message(raw_message)?;
            }

            // We are streaming data from the frontend in CopyBoth. The next message should
            // come from the frontend en be either CopyData or CopyDone.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Frontend(FrontendMessageKind::CopyData),
                Context::Query(Some((QType::CopyBoth, QState::SendData)))
            ) => {
                self.streaming_replication(&mut raw_message.clone())?;
                self.be_stream.put_raw_message(raw_message)?;
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
        Ok(self.context)
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

    fn streaming_replication(&mut self, raw_message: &mut RawMessage) -> anyhow::Result<bool> {
        let mut needs_feedback = false;

        // we can try from FrontendMessageKind::CopyData or BackendMessageKind it doesn't matter
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
                    needs_feedback = true;

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

                    // self.tcp_writer.put_raw_message_and_flush(raw_copy_data)?;
                }
                StreamingReplicationMessageKind::StandbyStatusUpdate => {
                    let standby_system_update_message =
                        StandbyStatusUpdate::deserialize(&mut raw_message.raw_body)?;
                    debug!("DETAIL: {:?}", standby_system_update_message,);
                }
                _ => (),
            }
        } else {
            unreachable!("streaming_replication() only receives CopyData messages");
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
