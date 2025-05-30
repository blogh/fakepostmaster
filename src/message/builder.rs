use bytes::{Bytes, BytesMut};
use std::marker::PhantomData;

use libpq_serde_types::{Deserialize, Serialize};

//FIXME: mode to mod.rs
use super::raw_message::{
    MessageBody, MessageHeader, MessageType, RawMessage, RequestHeader, RequestType,
};

use super::logical_message::*;
use super::message::*;
use super::streaming_message::*;

//*----------------------------------------------------------------------------
// MessageBuilder: structs
//*----------------------------------------------------------------------------

/// No message/request was created
#[derive(Debug, Clone)]
pub struct NoType;

/// No Streaming component in this message
#[derive(Debug, Clone)]
pub struct NotStreaming;

/// No logical component in this message
#[derive(Debug, Clone)]
pub struct NotLogical;

/// No origin component in this message
#[derive(Debug, Clone)]
pub struct NoFrom;

/// The message comes from the Frontend
#[derive(Debug, Clone)]
pub struct Frontend;

/// The message comes from the backend
#[derive(Debug, Clone)]
pub struct Backend;

/// The message builder struct
#[derive(Debug, Clone)]
pub struct MessageBuilder<F, M, S, L> {
    from_marker: PhantomData<F>,
    main: M,
    streaming: S,
    logical: L,
}

impl MessageBuilder<NoFrom, NoType, NotStreaming, NotLogical> {
    /// The MessageBuilder will be used for a Frontend message
    pub fn new_frontend_message() -> MessageBuilder<Frontend, NoType, NotStreaming, NotLogical> {
        MessageBuilder {
            from_marker: PhantomData,
            main: NoType,
            streaming: NotStreaming,
            logical: NotLogical,
        }
    }

    /// The MessageBuilder will be used for a Baclend message
    pub fn new_backend_message() -> MessageBuilder<Backend, NoType, NotStreaming, NotLogical> {
        MessageBuilder {
            from_marker: PhantomData,
            main: NoType,
            streaming: NotStreaming,
            logical: NotLogical,
        }
    }
}

//*----------------------------------------------------------------------------
// MessageBuilder: API
//*----------------------------------------------------------------------------

// --- Main part of the message
impl MessageBuilder<Frontend, NoType, NotStreaming, NotLogical> {
    /// Create a startup message
    pub fn startup_message(
        self,
        message: StartupMessage,
    ) -> MessageBuilder<Frontend, StartupMessage, NotStreaming, NotLogical> {
        MessageBuilder {
            from_marker: PhantomData,
            main: message,
            streaming: NotStreaming,
            logical: NotLogical,
        }
    }

    /// Create a PasswordMessage
    pub fn password_message(
        self,
        message: PasswordMessage,
    ) -> MessageBuilder<Frontend, PasswordMessage, NotStreaming, NotLogical> {
        MessageBuilder {
            from_marker: PhantomData,
            main: message,
            streaming: NotStreaming,
            logical: NotLogical,
        }
    }

    /// Create a Query message
    pub fn query(
        self,
        message: Query,
    ) -> MessageBuilder<Frontend, Query, NotStreaming, NotLogical> {
        MessageBuilder {
            from_marker: PhantomData,
            main: message,
            streaming: NotStreaming,
            logical: NotLogical,
        }
    }
}

impl MessageBuilder<Backend, NoType, NotStreaming, NotLogical> {
    /// Create a Md5AuthenticationPassword request
    pub fn authentication_md5_password(
        self,
        message: AuthenticationMD5Password,
    ) -> MessageBuilder<Backend, AuthenticationMD5Password, NotStreaming, NotLogical> {
        MessageBuilder {
            from_marker: PhantomData,
            main: message,
            streaming: NotStreaming,
            logical: NotLogical,
        }
    }
}

impl<F> MessageBuilder<F, NoType, NotStreaming, NotLogical> {
    /// Create CopyData message. It's content is contextual.
    ///
    /// It's used by the front and backend in CopyIn, CopyOut, and CopyBoth contexts.
    pub fn copy_data(self) -> MessageBuilder<F, CopyData, NotStreaming, NotLogical> {
        MessageBuilder {
            from_marker: PhantomData,
            main: CopyData {},
            streaming: NotStreaming,
            logical: NotLogical,
        }
    }
}

// --- StartupMessage
impl MessageBuilder<Frontend, StartupMessage, NotStreaming, NotLogical> {
    pub fn into_raw_message(self) -> RawMessage<RequestType> {
        let mut buffer_body = BytesMut::new();
        self.main.serialize(&mut buffer_body);

        let mut buffer_header = BytesMut::new();
        RequestHeader {
            length: (buffer_body.len() + 4) as i32,
        }
        .serialize(&mut buffer_header);

        RawMessage {
            mtype: RequestType::StartupMessage,
            header: buffer_header.into(),
            body: buffer_body.into(),
        }
    }
}

// --- PasswordMessage
impl MessageBuilder<Frontend, PasswordMessage, NotStreaming, NotLogical> {
    pub fn into_raw_message(self) -> RawMessage<MessageType> {
        self.build_libpq_message(None)
    }
}

// --- Query
impl MessageBuilder<Frontend, Query, NotStreaming, NotLogical> {
    pub fn into_raw_message(self) -> RawMessage<MessageType> {
        self.build_libpq_message(None)
    }
}

// --- Accessor for the main part
impl<T> MessageBuilder<Frontend, T, NotStreaming, NotLogical>
where
    T: Serialize,
{
    pub fn main_as_ref(&self) -> &T {
        &self.main
    }

    pub fn main_as_ref_mut(&mut self) -> &mut T {
        &mut self.main
    }
}

// --- AuthenticationMd5
impl MessageBuilder<Backend, AuthenticationMD5Password, NotStreaming, NotLogical> {
    pub fn into_raw_message(self) -> RawMessage<MessageType> {
        self.build_libpq_message(Some(i32::from(&AuthenticationMessageKind::MD5Password)))
    }
}

// --- CopyData
impl MessageBuilder<Backend, CopyData, NotStreaming, NotLogical> {
    pub fn primary_keepalive_message(
        self,
        message: PrimaryKeepAliveMessage,
    ) -> MessageBuilder<Backend, CopyData, PrimaryKeepAliveMessage, NotLogical> {
        MessageBuilder {
            from_marker: PhantomData,
            main: self.main,
            streaming: message,
            logical: NotLogical,
        }
    }

    pub fn xlog_data(
        self,
        message: XLogData,
    ) -> MessageBuilder<Backend, CopyData, XLogData, NotLogical> {
        MessageBuilder {
            from_marker: PhantomData,
            main: self.main,
            streaming: message,
            logical: NotLogical,
        }
    }
}

impl MessageBuilder<Frontend, CopyData, NotStreaming, NotLogical> {
    pub fn standby_status_update(
        self,
        message: StandbyStatusUpdate,
    ) -> MessageBuilder<Frontend, CopyData, StandbyStatusUpdate, NotLogical> {
        MessageBuilder {
            from_marker: PhantomData,
            main: self.main,
            streaming: message,
            logical: NotLogical,
        }
    }
}

// --- PrimaryKeepAlive
impl MessageBuilder<Backend, CopyData, PrimaryKeepAliveMessage, NotLogical> {
    pub fn into_raw_message(self) -> RawMessage<MessageType> {
        self.build_streaming_message()
    }
}

// --- StandbyStatusUpdate
impl MessageBuilder<Frontend, CopyData, StandbyStatusUpdate, NotLogical> {
    pub fn into_raw_message(self) -> RawMessage<MessageType> {
        self.build_streaming_message()
    }
}

// --- XlogData
impl MessageBuilder<Backend, CopyData, XLogData, NotLogical> {
    pub fn insert(self, message: Insert) -> MessageBuilder<Backend, CopyData, XLogData, Insert> {
        MessageBuilder {
            from_marker: PhantomData,
            main: self.main,
            streaming: self.streaming,
            logical: message,
        }
    }

    pub fn update(self, message: Update) -> MessageBuilder<Backend, CopyData, XLogData, Update> {
        MessageBuilder {
            from_marker: PhantomData,
            main: self.main,
            streaming: self.streaming,
            logical: message,
        }
    }

    pub fn delete(self, message: Delete) -> MessageBuilder<Backend, CopyData, XLogData, Delete> {
        MessageBuilder {
            from_marker: PhantomData,
            main: self.main,
            streaming: self.streaming,
            logical: message,
        }
    }

    pub fn truncate(
        self,
        message: Truncate,
    ) -> MessageBuilder<Backend, CopyData, XLogData, Truncate> {
        MessageBuilder {
            from_marker: PhantomData,
            main: self.main,
            streaming: self.streaming,
            logical: message,
        }
    }

    pub fn begin(self, message: Begin) -> MessageBuilder<Backend, CopyData, XLogData, Begin> {
        MessageBuilder {
            from_marker: PhantomData,
            main: self.main,
            streaming: self.streaming,
            logical: message,
        }
    }

    pub fn commit(self, message: Commit) -> MessageBuilder<Backend, CopyData, XLogData, Commit> {
        MessageBuilder {
            from_marker: PhantomData,
            main: self.main,
            streaming: self.streaming,
            logical: message,
        }
    }
}

// --- Insert
impl MessageBuilder<Backend, CopyData, XLogData, Insert> {
    pub fn into_raw_message(self) -> RawMessage<MessageType> {
        self.build_logical_message()
    }
}

// --- Update
impl MessageBuilder<Backend, CopyData, XLogData, Update> {
    pub fn into_raw_message(self) -> RawMessage<MessageType> {
        self.build_logical_message()
    }
}

// --- Delete
impl MessageBuilder<Backend, CopyData, XLogData, Delete> {
    pub fn into_raw_message(self) -> RawMessage<MessageType> {
        self.build_logical_message()
    }
}

// --- Truncate
impl MessageBuilder<Backend, CopyData, XLogData, Truncate> {
    pub fn into_raw_message(self) -> RawMessage<MessageType> {
        self.build_logical_message()
    }
}

// --- Begin
impl MessageBuilder<Backend, CopyData, XLogData, Begin> {
    pub fn into_raw_message(self) -> RawMessage<MessageType> {
        self.build_logical_message()
    }
}

// --- Commit
impl MessageBuilder<Backend, CopyData, XLogData, Commit> {
    pub fn into_raw_message(self) -> RawMessage<MessageType> {
        self.build_logical_message()
    }
}

//*----------------------------------------------------------------------------
// MessageBuilder: private encoding functions
//*----------------------------------------------------------------------------

impl<F, M, S, L> MessageBuilder<F, M, S, L>
where
    M: Serialize + MessageBody,
    S: Serialize + MessageBody,
    L: Serialize + MessageBody,
{
    //FIXME: the type constraint are a little lax maybe add a logical and streaming trait
    fn build_logical_message(self) -> RawMessage<MessageType> {
        // Reconstruct the streaming message
        let mut buffer_body = BytesMut::new();
        StreamingHeader {
            message_type: self.streaming.message_type() as i8,
        }
        .serialize(&mut buffer_body);
        self.streaming.serialize(&mut buffer_body);

        // Reconstruct the logical message
        LogicalHeader {
            message_type: self.logical.message_type() as i8,
        }
        .serialize(&mut buffer_body);
        self.logical.serialize(&mut buffer_body);

        // Reconstruct CopyData message
        let mut buffer_header = BytesMut::new();
        MessageHeader {
            message_type: self.main.message_type(),
            length: buffer_body.len() as i32 + 4,
        }
        .serialize(&mut buffer_header);

        let mtype = MessageType {
            main: self.main.message_type(),
            auth: None,
        };

        RawMessage {
            mtype,
            header: buffer_header.into(),
            body: buffer_body.into(),
        }
    }
}

impl<F, M, S> MessageBuilder<F, M, S, NotLogical>
where
    M: Serialize + MessageBody,
    S: Serialize + MessageBody,
{
    //FIXME: the type constraint are a little lax maybe add a streaming trait
    fn build_streaming_message(self) -> RawMessage<MessageType> {
        let mut buffer_body = BytesMut::new();

        // Reconstruct the streaming message
        StreamingHeader {
            message_type: self.streaming.message_type() as i8,
        }
        .serialize(&mut buffer_body);
        self.streaming.serialize(&mut buffer_body);

        // Reconstruct CopyData message
        let mut buffer_header = BytesMut::new();
        MessageHeader {
            message_type: self.main.message_type(),
            length: buffer_body.len() as i32 + 4,
        }
        .serialize(&mut buffer_header);

        let mtype = MessageType {
            main: self.main.message_type(),
            auth: None,
        };

        RawMessage {
            mtype,
            header: buffer_header.into(),
            body: buffer_body.into(),
        }
    }
}

impl<F, M> MessageBuilder<F, M, NotStreaming, NotLogical>
where
    M: Serialize + MessageBody,
{
    //FIXME: the type constraint are a little lax maybe add a streaming trait
    fn build_libpq_message(self, auth: Option<i32>) -> RawMessage<MessageType> {
        let mut buffer_body = BytesMut::new();

        // Reconstruct CopyData message
        self.main.serialize(&mut buffer_body);

        let mut buffer_header = BytesMut::new();
        MessageHeader {
            message_type: self.main.message_type(),
            length: buffer_body.len() as i32 + 4,
        }
        .serialize(&mut buffer_header);

        let mtype = MessageType {
            main: self.main.message_type(),
            auth,
        };

        RawMessage {
            mtype,
            header: buffer_header.into(),
            body: buffer_body.into(),
        }
    }
}
