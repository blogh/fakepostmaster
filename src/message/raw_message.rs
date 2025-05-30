use anyhow::anyhow;
use bytes::Bytes;
use std::io::{Read, Write};
use std::marker::PhantomData;
use tracing::*;

use libpq_serde_macros::SerdeLibpqData;
use libpq_serde_types::Deserialize;

use super::message::{BackendMessageKind, FrontendMessageKind};

//*----------------------------------------------------------------------------
// RawMessage
//*----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RawMessage<T> {
    //FIXME: remove public?
    pub mtype: T,
    pub header: Bytes,
    pub body: Bytes,
}

impl<T> RawMessage<T> {
    pub fn send<S>(&self, stream: &mut S) -> anyhow::Result<()>
    where
        S: Write,
        T: std::fmt::Debug,
    {
        stream.write(&self.header)?;
        stream.write(&self.body)?;

        //debug!(
        //    "Detailed dump:\nheader: {:}\nbody\n{:}",
        //    format_bytes(&self.header),
        //    format_bytes(&self.body)
        //);

        Ok(())
    }
}

fn format_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write;

    let mut formatted = String::new();

    for (i, &byte) in bytes.iter().enumerate() {
        // Format each byte as a two-character hexadecimal string
        write!(formatted, "{:02x}", byte).unwrap();

        // Add spaces at specific intervals
        if (i + 1) % 16 == 0 {
            formatted.push('\n');
        } else if i > 0 && i % 2 == 1 {
            formatted.push(' ');
        }
    }

    formatted
}

//*----------------------------------------------------------------------------
// Request / Initial messages
//*----------------------------------------------------------------------------
//
// RequestMessage (or initial messages) are send by the frontend to start a
// connection
//
// The raw request can be transformed into a request message body after via the
// implementation of TryFrom().
//
// The following Request types are not supported:
//
// * CancelRequest,
// * GSSENCRequest,
// * SSLRequest,

/// This trait is used for all Request/Initial messages
//FIXME: Do I really use it?
pub trait RequestBody {}

/// All the requests sent by the frontend
#[derive(Debug, Clone)]
pub enum RequestType {
    StartupMessage,
    CancelRequest,
    GSSENCRequest,
    SSLRequest,
}

impl From<&RequestType> for i32 {
    fn from(msg_kind: &RequestType) -> i32 {
        match msg_kind {
            &RequestType::StartupMessage => 196608,
            &RequestType::CancelRequest => 80877102,
            &RequestType::GSSENCRequest => 80877104,
            &RequestType::SSLRequest => 80877103,
        }
    }
}

impl TryFrom<i32> for RequestType {
    type Error = anyhow::Error;

    fn try_from(request_code: i32) -> anyhow::Result<RequestType> {
        match request_code {
            196608 => Ok(Self::StartupMessage),
            80877102 => Ok(Self::CancelRequest),
            80877104 => Ok(Self::GSSENCRequest),
            80877103 => Ok(Self::SSLRequest),
            _ => Err(anyhow!("Invalid request message: {request_code}")),
        }
    }
}

//FIXME: check in SerdeLibPqData if the import have the full path
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct RequestHeader {
    pub length: i32,
}
impl RawMessage<RequestType> {
    pub fn receive<S>(stream: &mut S) -> anyhow::Result<Self>
    where
        S: Read,
    {
        let mut buffer_header = vec![0_u8; 4];
        stream.read_exact(&mut buffer_header)?;
        let header = RequestHeader::deserialize(&mut Bytes::from(buffer_header.clone()))?;

        let mut buffer_body = vec![0_u8; (header.length - 4) as usize];
        stream.read_exact(&mut buffer_body)?;
        let buffer_body = Bytes::from(buffer_body);

        let mut mtype = [0_u8; 4];
        mtype.copy_from_slice(&buffer_body[0..4]);
        let mtype = i32::from_be_bytes(mtype);
        let mtype = RequestType::try_from(mtype)?;

        Ok(Self {
            mtype,
            header: buffer_header.into(),
            body: buffer_body.into(),
        })
    }
}

//*----------------------------------------------------------------------------
// Normal messages
//*----------------------------------------------------------------------------

/// This trait is used for all normal messages
pub trait MessageBody {
    fn message_type(&self) -> u8;
}

#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct MessageHeader {
    pub message_type: u8,
    pub length: i32,
}

#[derive(Clone)]
pub struct MessageType {
    pub main: u8,
    pub auth: Option<i32>,
}

impl std::fmt::Debug for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "MessageType {{ main: 0x{:x?}, auth: {:?} }}",
            self.main, self.auth
        )
    }
}

impl TryFrom<&MessageType> for BackendMessageKind {
    type Error = anyhow::Error;
    fn try_from(msg_kind: &MessageType) -> anyhow::Result<BackendMessageKind> {
        match msg_kind {
            &MessageType {
                main: 0x52, /* 'R' */
                auth: Some(0),
            } => Ok(BackendMessageKind::AuthenticationOk),
            &MessageType {
                main: 0x52, /* 'R' */
                auth: Some(2),
            } => Ok(BackendMessageKind::AuthenticationKerberosV5),
            &MessageType {
                main: 0x52, /* 'R' */
                auth: Some(3),
            } => Ok(BackendMessageKind::AuthenticationCleartextPassword),
            &MessageType {
                main: 0x52, /* 'R' */
                auth: Some(5),
            } => Ok(BackendMessageKind::AuthenticationMD5Password),
            &MessageType {
                main: 0x52, /* 'R' */
                auth: Some(7),
            } => Ok(BackendMessageKind::AuthenticationGSS),
            &MessageType {
                main: 0x52, /* 'R' */
                auth: Some(8),
            } => Ok(BackendMessageKind::AuthenticationGSSContinue),
            &MessageType {
                main: 0x52, /* 'R' */
                auth: Some(9),
            } => Ok(BackendMessageKind::AuthenticationSSPI),
            &MessageType {
                main: 0x52, /* 'R' */
                auth: Some(10),
            } => Ok(BackendMessageKind::AuthenticationSASL),
            &MessageType {
                main: 0x52, /* 'R' */
                auth: Some(11),
            } => Ok(BackendMessageKind::AuthenticationSASLContinue),
            &MessageType {
                main: 0x52, /* 'R' */
                auth: Some(12),
            } => Ok(BackendMessageKind::AuthenticationSASLFinal),
            &MessageType {
                main: 0x4b, /* 'K' */
                auth: None,
            } => Ok(BackendMessageKind::BackendKeyData),
            &MessageType {
                main: 0x32, /* '2' */
                auth: None,
            } => Ok(BackendMessageKind::BindComplete),
            &MessageType {
                main: 0x33, /* '3' */
                auth: None,
            } => Ok(BackendMessageKind::CloseComplete),
            &MessageType {
                main: 0x43, /* 'C' */
                auth: None,
            } => Ok(BackendMessageKind::CommandComplete),
            &MessageType {
                main: 0x64, /* 'd' */
                auth: None,
            } => Ok(BackendMessageKind::CopyData),
            &MessageType {
                main: 0x63, /* 'x' */
                auth: None,
            } => Ok(BackendMessageKind::CopyDone),
            &MessageType {
                main: 0x47, /* 'G' */
                auth: None,
            } => Ok(BackendMessageKind::CopyInResponse),
            &MessageType {
                main: 0x48, /* 'H' */
                auth: None,
            } => Ok(BackendMessageKind::CopyOutResponse),
            &MessageType {
                main: 0x57, /* 'W' */
                auth: None,
            } => Ok(BackendMessageKind::CopyBothResponse),
            &MessageType {
                main: 0x44, /* 'D' */
                auth: None,
            } => Ok(BackendMessageKind::DataRow),
            &MessageType {
                main: 0x49, /* 'I' */
                auth: None,
            } => Ok(BackendMessageKind::EmptyQuery),
            &MessageType {
                main: 0x45, /* 'E' */
                auth: None,
            } => Ok(BackendMessageKind::ErrorResponse),
            &MessageType {
                main: 0x56, /* 'V' */
                auth: None,
            } => Ok(BackendMessageKind::FunctionCallResponse),
            &MessageType {
                main: 0x76, /* 'v' */
                auth: None,
            } => Ok(BackendMessageKind::NegotiateProtocolVersion),
            &MessageType {
                main: 0x6e, /* 'n' */
                auth: None,
            } => Ok(BackendMessageKind::NoData),
            &MessageType {
                main: 0x4e, /* 'N' */
                auth: None,
            } => Ok(BackendMessageKind::NoticeResponse),
            &MessageType {
                main: 0x41, /* 'A' */
                auth: None,
            } => Ok(BackendMessageKind::NotificationResponse),
            &MessageType {
                main: 0x74, /* 't' */
                auth: None,
            } => Ok(BackendMessageKind::ParameterDescription),
            &MessageType {
                main: 0x53, /* 'S' */
                auth: None,
            } => Ok(BackendMessageKind::ParameterStatus),
            &MessageType {
                main: 0x31, /* '1' */
                auth: None,
            } => Ok(BackendMessageKind::ParseComplete),
            &MessageType {
                main: 0x73, /* 's' */
                auth: None,
            } => Ok(BackendMessageKind::PortalSuspended),
            &MessageType {
                main: 0x5a, /* 'Z' */
                auth: None,
            } => Ok(BackendMessageKind::ReadyForQuery),
            &MessageType {
                main: 0x54, /* 'T' */
                auth: None,
            } => Ok(BackendMessageKind::RowDescription),
            _ => Err(anyhow!("Unsupported backend message: {:?}", msg_kind)),
        }
    }
}

impl TryFrom<&MessageType> for FrontendMessageKind {
    type Error = anyhow::Error;

    fn try_from(msg_kind: &MessageType) -> anyhow::Result<FrontendMessageKind> {
        match msg_kind {
            &MessageType {
                main: 0x42, /* 'B' */
                auth: None,
            } => Ok(FrontendMessageKind::Bind),
            &MessageType {
                main: 0x43, /* 'C' */
                auth: None,
            } => Ok(FrontendMessageKind::Close),
            &MessageType {
                main: 0x64, /* 'd' */
                auth: None,
            } => Ok(FrontendMessageKind::CopyData),
            &MessageType {
                main: 0x63, /* 'c' */
                auth: None,
            } => Ok(FrontendMessageKind::CopyDone),
            &MessageType {
                main: 0x66, /* 'f' */
                auth: None,
            } => Ok(FrontendMessageKind::CopyFail),
            &MessageType {
                main: 0x44, /* 'D' */
                auth: None,
            } => Ok(FrontendMessageKind::Describe),
            &MessageType {
                main: 0x45, /* 'E' */
                auth: None,
            } => Ok(FrontendMessageKind::Execute),
            &MessageType {
                main: 0x46, /* 'F' */
                auth: None,
            } => Ok(FrontendMessageKind::Flush),
            &MessageType {
                main: 0x48, /* 'H' */
                auth: None,
            } => Ok(FrontendMessageKind::FunctionCall),
            &MessageType {
                main: 0x51, /* 'Q' */
                auth: None,
            } => Ok(FrontendMessageKind::Query),
            &MessageType {
                main: 0x58, /* 'X' */
                auth: None,
            } => Ok(FrontendMessageKind::Terminate),
            &MessageType {
                main: 0x70, /* 'p' */
                auth: None,
            } => Ok(FrontendMessageKind::ContextDependant),
            &MessageType {
                main: 0x50, /* 'P' */
                auth: None,
            } => Ok(FrontendMessageKind::Parse),
            _ => Err(anyhow!(
                "Unsupported code for frontend message: {:?}",
                msg_kind
            )),
        }
    }
}

impl RawMessage<MessageType> {
    pub fn receive<S>(stream: &mut S) -> anyhow::Result<Self>
    where
        S: Read,
    {
        let mut buffer_header = vec![0_u8; 4 + 1];
        stream.read_exact(&mut buffer_header)?;
        let header = MessageHeader::deserialize(&mut Bytes::from(buffer_header.clone()))?;

        let mut buffer_body = vec![0_u8; (header.length - 4) as usize];
        stream.read_exact(&mut buffer_body)?;
        let buffer_body = Bytes::from(buffer_body);

        let auth_msg_type = match header.message_type {
            // Authentication messages have a sub type
            0x52 /* 'R' */ => {
                let mut auth_msg_type = [0_u8; 4];
                auth_msg_type.copy_from_slice(&buffer_body[0..4]);
                Some(i32::from_be_bytes(auth_msg_type))
            }
            // Others dont
            _ => None,
        };

        let mtype = MessageType {
            main: header.message_type,
            auth: auth_msg_type,
        };

        //debug!(
        //    "Detailed dump:\nheader: {:}\nbody\n{:}",
        //    format_bytes(&buffer_header),
        //    format_bytes(&buffer_body)
        //);

        Ok(Self {
            mtype,
            header: buffer_header.into(),
            body: buffer_body.into(),
        })
    }
}
