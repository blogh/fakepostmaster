use anyhow::anyhow;
use libpq_serde_types::libpq_types::NullLength;
use libpq_serde_types::libpq_types::VecWithEncoding;
use std::collections::HashMap;
use std::net::TcpStream;
use tracing::*;

use libpq_serde_types::Deserialize;

use super::{PgToRustTypes, PgType, decode_from_text};
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
    QTextSubmitted(String),
    SimpleQuery(QState),
    CopyIn(CIState),
    CopyOut(COState),
    CopyBoth(CBState),
}

#[derive(Debug, Clone)]
pub enum QState {
    Data(QData),
    Done,
}

#[derive(Debug, Clone)]
pub struct QData {
    pub query: String,
    pub header: Vec<QColDesc>,
    pub data: Vec<Vec<Option<PgToRustTypes>>>,
}

#[derive(Debug, Clone)]
pub struct QColDesc {
    pub name: String,
    pub pg_type: PgType,
}

impl TryFrom<&QColumnDescription> for QColDesc {
    type Error = anyhow::Error;
    fn try_from(value: &QColumnDescription) -> anyhow::Result<QColDesc> {
        Ok(Self {
            name: value.name.clone().into_string()?,
            pg_type: PgType::try_from(value.datatype_id)?,
        })
    }
}

impl TryFrom<QColumnDescription> for QColDesc {
    type Error = anyhow::Error;
    fn try_from(value: QColumnDescription) -> anyhow::Result<QColDesc> {
        Ok(Self {
            name: value.name.into_string()?,
            pg_type: PgType::try_from(value.datatype_id)?,
        })
    }
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
    client_parameters: HashMap<String, String>,
    server_parameters: HashMap<String, String>,
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

        let startup_message =
            MessageBuilder::new_frontend_message().startup_message(StartupMessage {
                protocol_version: ProtocolVersion { major: 3, minor: 0 },
                parameters: VecWithEncoding::<ParameterStatus, NullLength>::from(vec![
                    ParameterStatus::new(&(String::from("user")), &user)?,
                    ParameterStatus::new(&(String::from("database")), &database)?,
                    ParameterStatus::new(&(String::from("application_name")), &application_name)?,
                    ParameterStatus::new(&(String::from("replication")), &replication)?,
                    ParameterStatus::new(
                        &(String::from("client_encoding")),
                        &(String::from("utf8")),
                    )?,
                ]),
            });
        let client_parameters = HashMap::<String, String>::try_from(startup_message.main_as_ref())?;
        startup_message.into_raw_message().send(&mut tcp_stream)?;

        Ok(Self {
            last_message_kind: None,
            context: Context::Authentication,
            tcp_stream,
            user,
            password,
            database,
            application_name,
            query,
            client_parameters,
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
            // The next message should be PasswordMessage .. so we send it
            (
                None,
                Message::Backend(BackendMessageKind::AuthenticationMD5Password),
                Context::Authentication,
            ) => {
                let message = AuthenticationMD5Password::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
                MessageBuilder::new_frontend_message()
                    .password_message(PasswordMessage::new_from_user_password(
                        &self.user,
                        &self.password,
                        &message.salt,
                    )?)
                    .into_raw_message()
                    .send(&mut self.tcp_stream)?;
            }

            // BackendKeyData hold secret data send from the server and the process PID
            // The next message should be sent by the backend: ReadyForQuery
            (
                Some(_),
                Message::Backend(BackendMessageKind::BackendKeyData),
                Context::Authentication,
            ) => {
                let message = BackendKeyData::try_from(&mut raw_message)?;
                self.server_parameters
                    .insert("process_id".to_string(), message.process_id.to_string());
                self.server_parameters
                    .insert("secret_key".to_string(), message.secret_key.to_string());
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
                match self.query.pop() {
                    // We still have queries to process
                    Some(query) => {
                        MessageBuilder::new_frontend_message()
                            .query(Query::new(query.clone())?)
                            .into_raw_message()
                            .send(&mut self.tcp_stream)?;
                        self.context = Context::QTextSubmitted(query);
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
                Context::QTextSubmitted(query)
            ) => {
                let message = RowDescription::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");

                let mut qdata = QData {
                    //TODO: Is the data cloned here? And same everywhere we use to_owned()
                    query: query.to_owned(),
                    header: Vec::<QColDesc>::new(),
                    data: Vec::new(),
                };
                for col in message.columns {
                    qdata.header.push(QColDesc::try_from(col)?);
                }

                self.context = Context::SimpleQuery(QState::Data(qdata));
            }

            // DataRow is one row of data sent by the backend
            (
                Some(_),
                Message::Backend(BackendMessageKind::DataRow),
                Context::SimpleQuery(QState::Data(qdata)),
            ) => {
                let message = DataRow::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
                let mut datarow = Vec::new();
                for (idx, data) in message.columns.into_iter().enumerate() {
                    let decoded_data = match data {
                        None => None,
                        Some(data) => Some(decode_from_text(&data, &qdata.header[idx].pg_type)?),
                    };
                    datarow.push(decoded_data);
                }
                qdata.data.push(datarow);
                self.context = Context::SimpleQuery(QState::Data(qdata.to_owned()));
            }

            // CommandComplete marks the end of the query. The next message should be
            // ReadyForQuery sent by the backend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CommandComplete),
                Context::SimpleQuery(QState::Data(qdata)),
            ) => {
                info!("{qdata:#?}");
                self.context = Context::SimpleQuery(QState::Done);
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
                let message = CopyOutResponse::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
            }

            // We are streaming data from CopyOut. The next message should also come from the
            // backend en be either CopyData or CommandComplete.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyData),
                Context::CopyOut(COState::Data)
            ) => {
                let message = CopyData::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
            }

            // CopyOut is done The next message should be Commandcomplete.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CommandComplete),
                Context::CopyOut(COState::Data)
            ) => {
                self.context = Context::CopyOut(COState::Done);
            }

            // CopyInResponse is sent after receiving a COPY FROM STDIN. The next message
            // should be a CopyData sent by the Frontend
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyInResponse),
                Context::QTextSubmitted(_query)
            ) => {
                self.context = Context::CopyIn(CIState::Data);
                let message = CopyInResponse::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
            }

            // We are streaming data from CopyIn. The next message
            // should come from the frontend and be either CopyData or CopyDone.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Frontend(FrontendMessageKind::CopyData),
                Context::CopyIn(CIState::Data)
            ) => {
                let message = CopyData::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
            }

            // CopyIn is done The next message should be Commandcomplete.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CommandComplete),
                Context::CopyIn(CIState::Data)
            ) => {
                self.context = Context::CopyIn(CIState::Done);
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
                //FIXME:rework init
                self.context = Context::CopyBoth(
                    CBState::Data(
                        CBData {
                            start_lsn: 0,
                            last_commit_lsn: 0,
                            relation_cache: HashMap::new()
                        }
                    )
                );
                let message = CopyBothResponse::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
            }

            // We are streaming data from CopyBoth. The next message should also come from the
            // backend en be either CopyData or CopyDone.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyData),
                Context::CopyBoth(CBState::Data(_))
            ) => {
                self.streaming_replication(&mut raw_message)?;
            }

            // the backend is done streaming data from CopyBoth. The next message should come from the
            // frontend be either CopyData or CopyDone
            #[cfg_attr(rustfmt, rustfmt_skip)]
            (
                Some(_),
                Message::Backend(BackendMessageKind::CopyDone),
                Context::CopyBoth(CBState::Data(_))
            ) => {
                //FIXME: We should send CopyDone here
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
            }

            // Async message can happend anytime, they dont change the context
            // except if they are ment to interrupt the flow of message.

            // ParameterStatus is typically sent in the Authentication Context
            // but it can be sent anytime if some client specific setting are modified
            // on the backend.
            (Some(_), Message::Backend(BackendMessageKind::ParameterStatus), _) => {
                let message = ParameterStatus::try_from(&mut raw_message)?;
                debug!("DETAIL: {message:?}");
                self.server_parameters
                    .insert(message.name.into_string()?, message.value.into_string()?);
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

    fn get_message(&mut self) -> anyhow::Result<(Message, RawMessage<MessageType>)> {
        let message = RawMessage::<MessageType>::receive(&mut self.tcp_stream)?;
        Ok((
            Message::Backend(BackendMessageKind::try_from(&message.mtype)?),
            message,
        ))
    }

    fn streaming_replication(
        &mut self,
        raw_message: &mut RawMessage<MessageType>,
    ) -> anyhow::Result<()> {
        if let BackendMessageKind::CopyData = BackendMessageKind::try_from(&raw_message.mtype)? {
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

                            //FIXME: probably do something with else? or assert that we are in the
                            //right context at the beginning
                            if let Context::CopyBoth(CBState::Data(ref mut cbdata)) = self.context {
                                cbdata
                                    .relation_cache
                                    .insert(message.rel_oid, CBRelation::try_from(message)?);
                            }
                        }
                        LogicalReplicationMessageKind::Begin => {
                            let message = Begin::deserialize(&mut raw_message.body)?;
                            debug!("DETAIL: {:?}", message,);
                        }
                        LogicalReplicationMessageKind::Commit => {
                            let message = Commit::deserialize(&mut raw_message.body)?;
                            debug!("DETAIL: {:?}", message,);

                            //FIXME: probably do something with else? or assert that we are in the
                            //right context at the beginning
                            if let Context::CopyBoth(CBState::Data(ref mut cbdata)) = self.context {
                                cbdata.last_commit_lsn = message.commit_lsn;
                            }
                        }
                        LogicalReplicationMessageKind::Insert => {
                            let message = Insert::deserialize(&mut raw_message.body)?;
                            debug!("DETAIL: {:?}", message,);

                            if let Context::CopyBoth(CBState::Data(ref cbdata)) = self.context {
                                match cbdata.relation_cache.get(&message.rel_oid) {
                                    Some(relation) => {
                                        let cols = relation
                                            .columns
                                            .iter()
                                            .map(|d| d.name.clone())
                                            .collect::<Vec<_>>()
                                            .join(", ");
                                        let mut datarow = Vec::new();
                                        for (idx, data) in
                                            message.new_tuple_data.columns.into_iter().enumerate()
                                        {
                                            //FIXME: column_value is a bad name
                                            datarow.push(decode_from_text(
                                                &data.column_value,
                                                &relation.columns[idx].pg_type,
                                            )?);
                                        }
                                        info!(
                                            "INSERT INTO {}({}) VALUES {:?}",
                                            relation.relation, cols, datarow
                                        );
                                    }
                                    None => return Err(anyhow!("Unknown relation in Insert")),
                                }
                            }
                        }
                        LogicalReplicationMessageKind::Update => {
                            debug!("UPDATE: {:?}", raw_message.body);
                            let message = Update::deserialize(&mut raw_message.body)?;
                            debug!("DETAIL: {:?}", message,);

                            if let Context::CopyBoth(CBState::Data(ref cbdata)) = self.context {
                                match cbdata.relation_cache.get(&message.rel_oid) {
                                    Some(relation) => {
                                        let cols = relation
                                            .columns
                                            .iter()
                                            .map(|d| d.name.clone())
                                            .collect::<Vec<_>>()
                                            .join(", ");

                                        let mut old_datarow = Vec::new();
                                        for (idx, data) in
                                            message.old_tuple_data.columns.into_iter().enumerate()
                                        {
                                            //FIXME: column_value is a bad name
                                            old_datarow.push(decode_from_text(
                                                &data.column_value,
                                                &relation.columns[idx].pg_type,
                                            )?);
                                        }

                                        let mut new_datarow = Vec::new();
                                        for (idx, data) in
                                            message.new_tuple_data.columns.into_iter().enumerate()
                                        {
                                            //FIXME: column_value is a bad name
                                            new_datarow.push(decode_from_text(
                                                &data.column_value,
                                                &relation.columns[idx].pg_type,
                                            )?);
                                        }
                                        info!(
                                            "UPDATE ON {}({}) OLD_VALUES {:?} NEW_VALUES {:?}",
                                            relation.relation, cols, old_datarow, new_datarow
                                        );
                                    }
                                    None => return Err(anyhow!("Unknown relation in Update")),
                                }
                            }
                        }
                        LogicalReplicationMessageKind::Delete => {
                            let message = Delete::deserialize(&mut raw_message.body)?;
                            debug!("DETAIL: {:?}", message,);

                            if let Context::CopyBoth(CBState::Data(ref cbdata)) = self.context {
                                match cbdata.relation_cache.get(&message.rel_oid) {
                                    Some(relation) => {
                                        let cols = relation
                                            .columns
                                            .iter()
                                            .map(|d| d.name.clone())
                                            .collect::<Vec<_>>()
                                            .join(", ");
                                        let mut datarow = Vec::new();
                                        for (idx, data) in
                                            message.old_tuple_data.columns.into_iter().enumerate()
                                        {
                                            //FIXME: column_value is a bad name
                                            datarow.push(decode_from_text(
                                                &data.column_value,
                                                &relation.columns[idx].pg_type,
                                            )?);
                                        }
                                        info!(
                                            "DELETE FROM {}({}) VALUES {:?}",
                                            relation.relation, cols, datarow
                                        );
                                    }
                                    None => return Err(anyhow!("Unknown relation in Delete")),
                                }
                            }
                        }
                        _ => debug!(
                            "Unsupported message: {:?}",
                            LogicalReplicationMessageKind::try_from(header.message_type)?
                        ),
                    };
                }
                StreamingReplicationMessageKind::PrimaryKeepAliveMessage => {
                    let primary_keep_alive_message =
                        PrimaryKeepAliveMessage::deserialize(&mut raw_message.body)?;
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
