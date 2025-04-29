use anyhow::anyhow;
use bytes::{BufMut, Bytes, BytesMut};
use libpq_serde_macros::{
    MessageBody, SerdeLibpqData, TryFromRawBackendMessage, TryFromRawFrontendMessage,
};
use libpq_serde_types::{
    ByteSized, Deserialize, Serialize,
    libpq_types::{Byte, Byte4, Vec16, Vec32, VecNull},
};
use md5::{Digest, Md5};
use std::ffi::CString;
use std::io::{BufReader, Read};

// The list of messages can be found here and has been copied below (v17):
// * https://www.postgresql.org/docs/17/protocol-flow.html
// * https://www.postgresql.org/docs/17/protocol-message-formats.html

//*----------------------------------------------------------------------------
// Requests handling
//*----------------------------------------------------------------------------

/// RequestMessage are send by the frontend to start a connection
/// This trait is used for all such messages.
pub trait RequestBody {}

#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct RequestHeader {
    pub length: i32,
}

/// This struct contains the raw request which can be transformed into
/// a request message body after via the implementation of TryFrom().
///
/// The following Request types are not supported:
/// * CancelRequest,
/// * GSSENCRequest,
/// * SSLRequest,
pub struct RawRequest {
    pub header: RequestHeader,
    pub request_kind: RequestMessageKind,
    pub raw_body: Bytes,
}

impl RawRequest {
    pub fn get<T>(buffered_reader: &mut BufReader<T>) -> anyhow::Result<Self>
    where
        T: Read,
        Self: Sized,
    {
        let mut buffer = vec![0_u8; 4];
        buffered_reader.read_exact(&mut buffer)?;
        let header = RequestHeader::deserialize(&mut Bytes::from(buffer))?;

        let mut buffer = vec![0_u8; (header.length - 4) as usize];
        buffered_reader.read_exact(&mut buffer)?;
        let raw_body = Bytes::from(buffer);

        let mut msg_kind = [0_u8; 4];
        msg_kind.copy_from_slice(&raw_body[0..4]);
        let request_kind = i32::from_be_bytes(msg_kind);
        let request_kind = RequestMessageKind::try_from(request_kind)?;

        Ok(Self {
            header,
            request_kind,
            raw_body,
        })
    }
}

/// All the requests sent by the frontend
#[derive(Debug)]
pub enum RequestMessageKind {
    StartupMessage,
    CancelRequest,
    GSSENCRequest,
    SSLRequest,
}

impl From<&RequestMessageKind> for i32 {
    fn from(msg_kind: &RequestMessageKind) -> i32 {
        match msg_kind {
            &RequestMessageKind::StartupMessage => 196608,
            &RequestMessageKind::CancelRequest => 80877102,
            &RequestMessageKind::GSSENCRequest => 80877104,
            &RequestMessageKind::SSLRequest => 80877103,
        }
    }
}

impl TryFrom<i32> for RequestMessageKind {
    type Error = anyhow::Error;

    fn try_from(request_code: i32) -> anyhow::Result<RequestMessageKind> {
        match request_code {
            196608 => Ok(Self::StartupMessage),
            80877102 => Ok(Self::CancelRequest),
            80877104 => Ok(Self::GSSENCRequest),
            80877103 => Ok(Self::SSLRequest),
            _ => Err(anyhow!("Invalid request message")),
        }
    }
}

//*----------------------------------------------------------------------------
// BackendMessage & FrontendMessage handling
//*----------------------------------------------------------------------------

/// A body has a type
pub trait MessageBody {
    fn message_type(&self) -> u8;
}

//*----------------------------------------------------------------------------
// BackendMessage handling
//*----------------------------------------------------------------------------

/// A BackendMessage in raw form that we can convert into the actual
/// messages via TryFrom.
#[derive(Debug)]
pub struct RawBackendMessage {
    pub header: MessageHeader,
    pub raw_body: Bytes,
}

impl RawBackendMessage {
    pub fn get<T>(buffered_reader: &mut BufReader<T>) -> anyhow::Result<Self>
    where
        T: Read,
    {
        let mut buffer = vec![0_u8; 4 + 1];
        buffered_reader.read_exact(&mut buffer)?;
        let header = MessageHeader::deserialize(&mut Bytes::from(buffer))?;

        let mut buffer = vec![0_u8; (header.length - 4) as usize];
        buffered_reader.read_exact(&mut buffer)?;
        let raw_body = Bytes::from(buffer);

        Ok(Self { header, raw_body })
    }

    pub fn get_message_kind(&self) -> Option<BackendMessageKind> {
        BackendMessageKind::try_from(self.header.message_type).ok()
    }

    pub fn get_auth_message_kind(&self) -> Option<AuthenticationMessageKind> {
        if let Some(BackendMessageKind::Authentication) = self.get_message_kind() {
            let mut msg_kind = [0_u8; 4];
            msg_kind.copy_from_slice(&self.raw_body[0..4]);
            let msg_kind = i32::from_be_bytes(msg_kind);

            AuthenticationMessageKind::try_from(msg_kind).ok()
        } else {
            None
        }
    }
}

/// All the messages sent by the Backend
#[derive(Debug)]
pub enum BackendMessageKind {
    Authentication,           // R
    BackendKeyData,           // K
    BindComplete,             // 2
    CloseCompleten,           // 3
    CommandComplete,          // C
    CopyData,                 // d
    CopyDone,                 // c
    CopyInResponse,           // G
    CopyOutResponse,          // H
    CopyBothResponse,         // W
    DataRow,                  // D
    EmptyQuery,               // I
    ErrorResponse,            // E
    FunctionCallResponse,     // V
    NegotiateProtocolVersion, // v
    NoData,                   // n
    NoticeResponse,           // N
    NotificationResponse,     // A
    ParameterDescription,     // t
    ParameterStatus,          // S
    ParseComplete,            // 1
    PortalSuspended,          // s
    ReadyForQuery,            // Z
    RowDescription,           // T
}

impl From<&BackendMessageKind> for u8 {
    fn from(msg_kind: &BackendMessageKind) -> u8 {
        let msg_code = match msg_kind {
            BackendMessageKind::Authentication => 'R',
            BackendMessageKind::BackendKeyData => 'K',
            BackendMessageKind::BindComplete => '2',
            BackendMessageKind::CloseCompleten => '3',
            BackendMessageKind::CommandComplete => 'C',
            BackendMessageKind::CopyData => 'd',
            BackendMessageKind::CopyDone => 'c',
            BackendMessageKind::CopyInResponse => 'G',
            BackendMessageKind::CopyOutResponse => 'H',
            BackendMessageKind::CopyBothResponse => 'W',
            BackendMessageKind::DataRow => 'D',
            BackendMessageKind::EmptyQuery => 'I',
            BackendMessageKind::ErrorResponse => 'E',
            BackendMessageKind::FunctionCallResponse => 'V',
            BackendMessageKind::NegotiateProtocolVersion => 'v',
            BackendMessageKind::NoData => 'n',
            BackendMessageKind::NoticeResponse => 'N',
            BackendMessageKind::NotificationResponse => 'A',
            BackendMessageKind::ParameterDescription => 't',
            BackendMessageKind::ParameterStatus => 'S',
            BackendMessageKind::ParseComplete => '1',
            BackendMessageKind::PortalSuspended => 's',
            BackendMessageKind::ReadyForQuery => 'Z',
            BackendMessageKind::RowDescription => 'T',
        };
        msg_code as u8
    }
}

impl TryFrom<u8> for BackendMessageKind {
    type Error = anyhow::Error;

    fn try_from(msg_code: u8) -> anyhow::Result<BackendMessageKind> {
        match msg_code {
            0x52 /* 'R' */ => Ok(BackendMessageKind::Authentication),
            0x4b /* 'K' */ => Ok(BackendMessageKind::BackendKeyData),
            0x32 /* '2' */ => Ok(BackendMessageKind::BindComplete),
            0x33 /* '3' */ => Ok(BackendMessageKind::CloseCompleten),
            0x43 /* 'C' */ => Ok(BackendMessageKind::CommandComplete),
            0x64 /* 'd' */ => Ok(BackendMessageKind::CopyData),
            0x63 /* 'c' */ => Ok(BackendMessageKind::CopyDone),
            0x47 /* 'G' */ => Ok(BackendMessageKind::CopyInResponse),
            0x48 /* 'H' */ => Ok(BackendMessageKind::CopyOutResponse),
            0x57 /* 'W' */ => Ok(BackendMessageKind::CopyBothResponse),
            0x44 /* 'D' */ => Ok(BackendMessageKind::DataRow),
            0x49 /* 'I' */ => Ok(BackendMessageKind::EmptyQuery),
            0x45 /* 'E' */ => Ok(BackendMessageKind::ErrorResponse),
            0x56 /* 'V' */ => Ok(BackendMessageKind::FunctionCallResponse),
            0x76 /* 'v' */ => Ok(BackendMessageKind::NegotiateProtocolVersion),
            0x6e /* 'n' */ => Ok(BackendMessageKind::NoData),
            0x4e /* 'N' */ => Ok(BackendMessageKind::NoticeResponse),
            0x41 /* 'A' */ => Ok(BackendMessageKind::NotificationResponse),
            0x74 /* 't' */ => Ok(BackendMessageKind::ParameterDescription),
            0x53 /* 'S' */ => Ok(BackendMessageKind::ParameterStatus),
            0x31 /* '1' */ => Ok(BackendMessageKind::ParseComplete),
            0x73 /* 's' */ => Ok(BackendMessageKind::PortalSuspended),
            0x5a /* 'Z' */ => Ok(BackendMessageKind::ReadyForQuery),
            0x54 /* 'T' */ => Ok(BackendMessageKind::RowDescription),
            _ => Err(anyhow!("Unsupported code for backend message")),
        }
    }
}

/// AuthenticationMessage can have several different kind
/// which are listed here
#[derive(Debug)]
pub enum AuthenticationMessageKind {
    Ok,                // 0
    KerberosV5,        // 2
    CleartextPassword, // 3
    MD5Password,       // 5
    GSS,               // 7
    GSSContinue,       // 8
    SSPI,              // 9
    SASL,              // 10
    SASLContinue,      // 11
    SASLFinal,         // 12
}

impl From<&AuthenticationMessageKind> for i32 {
    fn from(msg_kind: &AuthenticationMessageKind) -> i32 {
        match msg_kind {
            AuthenticationMessageKind::Ok => 0,
            AuthenticationMessageKind::KerberosV5 => 2,
            AuthenticationMessageKind::CleartextPassword => 3,
            AuthenticationMessageKind::MD5Password => 5,
            AuthenticationMessageKind::GSS => 7,
            AuthenticationMessageKind::GSSContinue => 8,
            AuthenticationMessageKind::SSPI => 9,
            AuthenticationMessageKind::SASL => 10,
            AuthenticationMessageKind::SASLContinue => 11,
            AuthenticationMessageKind::SASLFinal => 12,
        }
    }
}

impl TryFrom<i32> for AuthenticationMessageKind {
    type Error = anyhow::Error;

    fn try_from(msg_code: i32) -> anyhow::Result<AuthenticationMessageKind> {
        match msg_code {
            0 => Ok(AuthenticationMessageKind::Ok),
            2 => Ok(AuthenticationMessageKind::KerberosV5),
            3 => Ok(AuthenticationMessageKind::CleartextPassword),
            5 => Ok(AuthenticationMessageKind::MD5Password),
            7 => Ok(AuthenticationMessageKind::GSS),
            8 => Ok(AuthenticationMessageKind::GSSContinue),
            9 => Ok(AuthenticationMessageKind::SSPI),
            10 => Ok(AuthenticationMessageKind::SASL),
            11 => Ok(AuthenticationMessageKind::SASLContinue),
            12 => Ok(AuthenticationMessageKind::SASLFinal),
            _ => Err(anyhow!("Unsupported code for authentication message")),
        }
    }
}

//*----------------------------------------------------------------------------
// FrontendMessage handling
//*----------------------------------------------------------------------------

/// A FrontendMessage in raw form that we can convert into the actual
/// messages via TryFrom.
#[derive(Debug)]
pub struct RawFrontendMessage {
    pub header: MessageHeader,
    pub raw_body: Bytes,
}

impl RawFrontendMessage {
    pub fn get<T>(buffered_reader: &mut BufReader<T>) -> anyhow::Result<Self>
    where
        T: Read,
    {
        let mut buffer = vec![0_u8; 4 + 1];
        buffered_reader.read_exact(&mut buffer)?;
        let header = MessageHeader::deserialize(&mut Bytes::from(buffer))?;

        let mut buffer = vec![0_u8; (header.length - 4) as usize];
        buffered_reader.read_exact(&mut buffer)?;
        let raw_body = Bytes::from(buffer);

        Ok(Self { header, raw_body })
    }

    pub fn get_message_kind(&self) -> Option<FrontendMessageKind> {
        FrontendMessageKind::try_from(self.header.message_type).ok()
    }
}

#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct MessageHeader {
    pub message_type: u8,
    pub length: i32,
}

impl MessageHeader {
    pub fn new_header_from_body<T>(body: &T) -> Self
    where
        T: MessageBody + ByteSized,
    {
        Self {
            message_type: body.message_type(),
            length: body.byte_size() + 4,
        }
    }

    pub fn new_raw_header_from_body<T>(buffer: &mut BytesMut, body: &T)
    where
        T: MessageBody + ByteSized,
    {
        buffer.put_u8(body.message_type());
        buffer.put_i32(body.byte_size() + 4);
    }
}

/// All the messages sent by the Frontend
#[derive(Debug)]
pub enum FrontendMessageKind {
    Bind,                // B
    Close,               // C
    CopyData,            // d
    CopyDone,            // c
    CopyFail,            // f
    Describe,            // D
    Execute,             // E
    Flush,               // F
    FunctionCall,        // H
    GSSResponse,         // p
    Parse,               // P
    PasswordMessage,     // p
    Query,               // Q
    SASLInitialResponse, // p
    SASLResponse,        // p
    Terminate,           // X
}

impl From<&FrontendMessageKind> for u8 {
    fn from(msg_kind: &FrontendMessageKind) -> u8 {
        let msg_code = match msg_kind {
            FrontendMessageKind::Bind => 'B',
            FrontendMessageKind::Close => 'C',
            FrontendMessageKind::CopyData => 'd',
            FrontendMessageKind::CopyDone => 'c',
            FrontendMessageKind::CopyFail => 'f',
            FrontendMessageKind::Describe => 'D',
            FrontendMessageKind::Execute => 'E',
            FrontendMessageKind::Flush => 'F',
            FrontendMessageKind::FunctionCall => 'H',
            FrontendMessageKind::GSSResponse => 'p',
            FrontendMessageKind::Parse => 'P',
            FrontendMessageKind::PasswordMessage => 'p',
            FrontendMessageKind::Query => 'Q',
            FrontendMessageKind::SASLInitialResponse => 'p',
            FrontendMessageKind::SASLResponse => 'p',
            FrontendMessageKind::Terminate => 'X',
        };
        msg_code as u8
    }
}

impl TryFrom<u8> for FrontendMessageKind {
    type Error = anyhow::Error;

    fn try_from(msg_code: u8) -> anyhow::Result<FrontendMessageKind> {
        match msg_code {
            0x42 /* B */ => Ok(FrontendMessageKind::Bind),
            0x43 /* C */ => Ok(FrontendMessageKind::Close),
            0x64 /* d */ => Ok(FrontendMessageKind::CopyData),
            0x63 /* c */ => Ok(FrontendMessageKind::CopyDone),
            0x66 /* f */ => Ok(FrontendMessageKind::CopyFail),
            0x44 /* D */ => Ok(FrontendMessageKind::Describe),
            0x45 /* E */ => Ok(FrontendMessageKind::Execute),
            0x46 /* F */ => Ok(FrontendMessageKind::Flush),
            0x48 /* H */ => Ok(FrontendMessageKind::FunctionCall),
            0x51 /* Q */ => Ok(FrontendMessageKind::Query),
            0x58 /* X */ => Ok(FrontendMessageKind::Terminate),
            0x70 /* p */ => Err(anyhow!(
                "Frontend Message kind cannot be guessed without context: 'p'"
            )),
            0x50 /* P */ => Err(anyhow!(
                "Frontend Message kind cannot be guessed without context: 'P'"
            )),
            _ => Err(anyhow!("Unsupported code for frontend message")),
        }
    }
}

//*----------------------------------------------------------------------------
//LibPQ Messages
//*----------------------------------------------------------------------------

// AuthenticationOk(B)
// * Byte1('R') Identifies the message as an authentication request.
// * Int32(8) Length of message contents in bytes, including self.
// * Int32(0) Specifies that the authentication was successful.
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody)]
#[message_body(kind = 'R')]
pub struct AuthenticationOk {
    pub code: i32,
}

impl AuthenticationOk {
    pub fn new() -> Self {
        Self { code: 0 }
    }
}

// Auth message cannot derive TryFromRawBackendMessage they have a specific implementation
impl TryFrom<&mut RawBackendMessage> for AuthenticationOk {
    type Error = anyhow::Error;

    fn try_from(message: &mut RawBackendMessage) -> anyhow::Result<AuthenticationOk> {
        if let Some(BackendMessageKind::Authentication) = message.get_message_kind() {
            if let Some(AuthenticationMessageKind::Ok) = message.get_auth_message_kind() {
                return AuthenticationOk::deserialize(&mut message.raw_body);
            }
        }
        Err(anyhow!(
            "Impossible to create AuthenticationOk from RawBackendMessage"
        ))
    }
}

// AuthenticationKerberosV5 (B)
// * Byte1('R') Identifies the message as an authentication request.
// * Int32(8) Length of message contents in bytes, including self.
// * Int32(2) Specifies that Kerberos V5 authentication is required.

// AuthenticationCleartextPassword (B)
// * Byte1('R') Identifies the message as an authentication request.
// * Int32(8) Length of message contents in bytes, including self.
// * Int32(3) Specifies that a clear-text password is required.
//NOTE: deprecated, will probably not implement

// AuthenticationMD5Password (B)
// * Byte1('R') Identifies the message as an authentication request.
// * Int32(12) Length of message contents in bytes, including self.
// * Int32(5) Specifies that an MD5-encrypted password is required.
// * Byte4 The salt to use when encrypting the password.
//NOTE: supported for the moment as it's easier to implement,
// but I might it dump later on.
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody)]
#[message_body(kind = 'R')]
pub struct AuthenticationMD5Password {
    pub code: i32,
    pub salt: Byte4,
}

impl AuthenticationMD5Password {
    pub fn new(salt: Byte4) -> Self {
        Self { code: 5, salt }
    }
}

// Auth message cannot derive TryFromRawBackendMessage they have a specific implementation
impl TryFrom<&mut RawBackendMessage> for AuthenticationMD5Password {
    type Error = anyhow::Error;

    fn try_from(message: &mut RawBackendMessage) -> anyhow::Result<AuthenticationMD5Password> {
        if let Some(BackendMessageKind::Authentication) = message.get_message_kind() {
            if let Some(AuthenticationMessageKind::MD5Password) = message.get_auth_message_kind() {
                return AuthenticationMD5Password::deserialize(&mut message.raw_body);
            }
        }
        Err(anyhow!(
            "Impossible to create AuthenticationMD5Password from RawBackendMessage"
        ))
    }
}

// AuthenticationGSS (B)
// * Byte1('R') Identifies the message as an authentication request.
// * Int32(8) Length of message contents in bytes, including self.
// * Int32(7) Specifies that GSSAPI authentication is required.

// AuthenticationGSSContinue (B)
// * Byte1('R') Identifies the message as an authentication request.
// * Int32 Length of message contents in bytes, including self.
// * Int32(8) Specifies that this message contains GSSAPI or SSPI data.
// * Byten GSSAPI or SSPI authentication data.

// AuthenticationSSPI (B)
// * Byte1('R') Identifies the message as an authentication request.
// * Int32(8) Length of message contents in bytes, including self.
// * Int32(9) Specifies that SSPI authentication is required.

// AuthenticationSASL (B)
// * Byte1('R') Identifies the message as an authentication request.
// * Int32 Length of message contents in bytes, including self.
// * Int32(10) Specifies that SASL authentication is required.
//
// The message body is a list of SASL authentication mechanisms, in the server's order of preference.
//     A zero byte is required as terminator after the last authentication mechanism name. For each
//     mechanism, there is the following:
//
// * String Name of a SASL authentication mechanism.
//TODO: implement

// AuthenticationSASLContinue (B)
// * Byte1('R') Identifies the message as an authentication request.
// * Int32 Length of message contents in bytes, including self.
// * Int32(11) Specifies that this message contains a SASL challenge.
// * Byten SASL data, specific to the SASL mechanism being used.
//TODO: implement

// AuthenticationSASLFinal (B)
// * Byte1('R') Identifies the message as an authentication request.
// * Int32 Length of message contents in bytes, including self.
// * Int32(12) Specifies that SASL authentication has completed.
// * Byten SASL outcome "additional data", specific to the SASL mechanism being used.
//TODO: implement

// BackendKeyData (B)
// * Byte1('K') Identifies the message as cancellation key data. The frontend must save these values if
//       it wishes to be able to issue CancelRequest messages later.
// * Int32(12) Length of message contents in bytes, including self.
// * Int32 The process ID of this backend.
// * Int32 The secret key of this backend.
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody, TryFromRawBackendMessage)]
#[message_body(kind = 'K')]
pub struct BackendKeyData {
    pub process_id: i32,
    pub secret_key: i32,
}

impl BackendKeyData {
    pub fn new(process_id: i32, secret_key: i32) -> Self {
        Self {
            process_id,
            secret_key,
        }
    }
}

// Bind (F)
// * Byte1('B') Identifies the message as a Bind command.
// * Int32 Length of message contents in bytes, including self.
// * String The name of the destination portal (an empty string selects the unnamed portal).
// * String The name of the source prepared statement (an empty string selects the unnamed prepared
//           statement).
// * Int16 The number of parameter format codes that follow (denoted C below). This can be zero to
//       indicate that there are no parameters or that the parameters all use the default format (text);
//   or one, in which case the specified format code is applied to all parameters; or it can equal the
//   actual number of parameters.
// * Int16[C] The parameter format codes. Each must presently be zero (text) or one (binary).
// * Int16 The number of parameter values that follow (possibly zero). This must match the number of
// * parameters needed by the query.
//
// Next, the following pair of fields appear for each parameter:
//
// * Int32 The length of the parameter value, in bytes (this count does not include itself). Can be
//   zero. As a special case, -1 indicates a NULL parameter value. No value bytes follow in the NULL
//   case.
// * Byten The value of the parameter, in the format indicated by the associated format code. n is the
//   above length.
//
// After the last parameter, the following fields appear:
//
// * Int16 The number of result-column format codes that follow (denoted R below). This can be zero to
//      indicate that there are no result columns or that the result columns should all use the default
//      format (text); or one, in which case the specified format code is applied to all result columns
//  (if any); or it can equal the actual number of result columns of the query.
// * Int16[R] The result-column format codes. Each must presently be zero (text) or one (binary).

// BindComplete (B)
// * Byte1('2') Identifies the message as a Bind-complete indicator.
// * Int32(4) Length of message contents in bytes, including self.

// CancelRequest (F)
// * Int32(16) Length of message contents in bytes, including self.
// * Int32(80877102) The cancel request code. The value is chosen to contain 1234 in the most
//   significant 16 bits, and 5678 in the least significant 16 bits. (To avoid confusion, this code must
//   not be the same as any protocol version number.)
// * Int32 The process ID of the target backend.
// * Int32 The secret key for the target backend.

// Close (F)
// * Byte1('C') Identifies the message as a Close command.
// * Int32 Length of message contents in bytes, including self.
// * Byte1 'S' to close a prepared statement; or 'P' to close a portal.
// * String The name of the prepared statement or portal to close (an empty string selects the unnamed
//         prepared statement or portal).

// CloseComplete (B)
// * Byte1('3') Identifies the message as a Close-complete indicator.
// * Int32(4) Length of message contents in bytes, including self.

// CommandComplete (B)
// * Byte1('C') Identifies the message as a command-completed response.
// * Int32 Length of message contents in bytes, including self.
// * String The command tag. This is usually a single word that identifies which SQL command was
//    completed.
//   - For an INSERT command, the tag is INSERT oid rows, where rows is the number of rows inserted. oid
//   used to be the object ID of the inserted row if rows was 1 and the target table had OIDs, but OIDs
//   system columns are not supported anymore; therefore oid is always 0.
//   - For a DELETE command, the tag is DELETE rows where rows is the number of rows deleted.
//   - For an UPDATE command, the tag is UPDATE rows where rows is the number of rows updated.
//   - For a MERGE command, the tag is MERGE rows where rows is the number of rows inserted, updated, or
//   deleted.
//   - For a SELECT or CREATE TABLE AS command, the tag is SELECT rows where rows is the number of rows
//   retrieved.
//   - For a MOVE command, the tag is MOVE rows where rows is the number of rows the cursor's position has
//   been changed by.
//   - For a FETCH command, the tag is FETCH rows where rows is the number of rows that have been
//   retrieved from the cursor.
//   - For a COPY command, the tag is COPY rows where rows is the number of rows copied. (Note: the row
//   count appears only in PostgreSQL 8.2 and later.)
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody, TryFromRawBackendMessage)]
#[message_body(kind = 'C')]
pub struct CommandComplete {
    pub command_tag: CString,
}

impl CommandComplete {
    pub fn new(command_tag: String) -> anyhow::Result<Self> {
        Ok(Self {
            command_tag: CString::new(&command_tag[..])?,
        })
    }
}

// CopyData (F & B)
// * Byte1('d') Identifies the message as COPY data.
// * Int32 Length of message contents in bytes, including self.
// * Byten Data that forms part of a COPY data stream. Messages sent from the backend will always
//     correspond to single data rows, but messages sent by frontends might divide the data stream
//     arbitrarily.

// CopyDone (F & B)
// * Byte1('c') Identifies the message as a COPY-complete indicator.
// * Int32(4) Length of message contents in bytes, including self.

// CopyFail (F)
// * Byte1('f') Identifies the message as a COPY-failure indicator.
// * Int32 Length of message contents in bytes, including self.
// * String An error message to report as the cause of failure.

// CopyInResponse (B)
// * Byte1('G') Identifies the message as a Start Copy In response. The frontend must now send copy-in
//   data (if not prepared to do so, send a CopyFail message).
// * Int32 Length of message contents in bytes, including self.
// * Int8
//     0 indicates the overall COPY format is textual (rows separated by newlines, columns separated
//   by separator characters, etc.). 1 indicates the overall copy format is binary (similar to DataRow
//         format). See COPY for more information.
// * Int16 The number of columns in the data to be copied (denoted N below).
// * Int16[N] The format codes to be used for each column. Each must presently be zero (text) or one
//     (binary). All must be zero if the overall copy format is textual.

// CopyOutResponse (B)
// * Byte1('H') Identifies the message as a Start Copy Out response. This message will be followed by
//   copy-out data.
// * Int32 Length of message contents in bytes, including self.
// * Int8
//   0 indicates the overall COPY format is textual (rows separated by newlines, columns separated
//   by separator characters, etc.). 1 indicates the overall copy format is binary (similar to DataRow
//   format). See COPY for more information.
// * Int16 The number of columns in the data to be copied (denoted N below).
// * Int16[N] The format codes to be used for each column. Each must presently be zero (text) or one
//   (binary). All must be zero if the overall copy format is textual.

// CopyBothResponse (B)
// * Byte1('W') Identifies the message as a Start Copy Both response. This message
// is used only for Streaming Replication.
// * Int32 Length of message contents in bytes, including self.
// * Int8
//     0 indicates the overall COPY format is textual (rows separated by newlines, columns separated
//   by separator characters, etc.). 1 indicates the overall copy format is binary (similar to DataRow
//         format). See COPY for more information.
// * Int16 The number of columns in the data to be copied (denoted N below).
// * Int16[N] The format codes to be used for each column. Each must presently be zero (text) or one
//     (binary). All must be zero if the overall copy format is textual.

// DataRow (B)
// * Byte1('D') Identifies the message as a data row.
// * Int32 Length of message contents in bytes, including self.
// * Int16 The number of column values that follow (possibly zero). Next, the following pair of fields
// appear for each column:
// * Int32 The length of the column value, in bytes (this count does not include itself). Can be zero.
// As a special case, -1 indicates a NULL column value. No value bytes follow in the NULL case.
// * Byten The value of the column, in the format indicated by the associated format code. n is the
// above length.
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody, TryFromRawBackendMessage)]
#[message_body(kind = 'D')]
pub struct DataRow {
    // The serialization will create a length field
    pub columns: Vec16<ColumnData>,
}

impl DataRow {
    pub fn new(columns: Vec<ColumnData>) -> Self {
        Self {
            columns: columns.into(),
        }
    }
}

pub type ColumnData = Vec32<Byte>;

// Describe (F)
// * Byte1('D') Identifies the message as a Describe command.
// * Int32 Length of message contents in bytes, including self.
// * Byte1 'S' to describe a prepared statement; or 'P' to describe a portal.
// * String The name of the prepared statement or portal to describe (an empty string selects the
//         unnamed prepared statement or portal).

// EmptyQueryResponse (B)
// * Byte1('I') Identifies the message as a response to an empty query string. (This substitutes for
//   CommandComplete.)
// * Int32(4) Length of message contents in bytes, including self.

// ErrorResponse (B)
// * Byte1('E') Identifies the message as an error.
// * Int32 Length of message contents in bytes, including self.
//
// The message body consists of one or more identified fields, followed by a zero byte as a
// terminator. Fields can appear in any order. For each field there is the following:
//
// * Byte1 A code identifying the field type; if zero, this is the message terminator and no string
// follows. The presently defined field types are listed in Section 53.8. Since more field types might
// be added in future, frontends should silently ignore fields of unrecognized type.
// * String The field value.
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody, TryFromRawBackendMessage)]
#[message_body(kind = 'E')]
pub struct ErrorResponse {
    // The serialization will create a length field
    pub messages: VecNull<ErrorMessage>,
}

impl ErrorResponse {
    pub fn new(messages: Vec<ErrorMessage>) -> Self {
        Self {
            messages: messages.into(),
        }
    }
}

#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct ErrorMessage {
    // Identifier: https://www.postgresql.org/docs/17/protocol-error-fields.html
    pub code: Byte,
    // The actual message
    pub message: CString,
}

impl ErrorMessage {
    pub fn new(code: char, message: &String) -> anyhow::Result<Self> {
        Ok(Self {
            code: code as u8,
            message: CString::new(&message[..])?,
        })
    }
}

// Execute (F)
// * Byte1('E') Identifies the message as an Execute command.
// * Int32 Length of message contents in bytes, including self.
// * String The name of the portal to execute (an empty string selects the unnamed portal).
// * Int32 Maximum number of rows to return, if portal contains a query that returns rows (ignored
//         otherwise). Zero denotes “no limit”.

// Flush (F)
// * Byte1('H') Identifies the message as a Flush command.
// * Int32(4) Length of message contents in bytes, including self.

// FunctionCall (F)
// * Byte1('F') Identifies the message as a function call.
// * Int32 Length of message contents in bytes, including self.
// * Int32 Specifies the object ID of the function to call.
// * Int16 The number of argument format codes that follow (denoted C below). This can be zero to
//     indicate that there are no arguments or that the arguments all use the default format (text);
// or one, in which case the specified format code is applied to all arguments; or it can equal the
// actual number of arguments.
// * Int16[C] The argument format codes. Each must presently be zero (text) or one (binary).
// * Int16 Specifies the number of arguments being supplied to the function.
//
// Next, the following pair of fields appear for each argument:
//
// * Int32 The length of the argument value, in bytes (this count does not include itself). Can be zero.
// As a special case, -1 indicates a NULL argument value. No value bytes follow in the NULL case.
// * Byten The value of the argument, in the format indicated by the associated format code. n is the
// above length.
//
// After the last argument, the following field appears:
//
// * Int16 The format code for the function result. Must presently be zero (text) or one (binary).

// FunctionCallResponse (B)
// * Byte1('V') Identifies the message as a function call result.
// * Int32 Length of message contents in bytes, including self.
// * Int32 The length of the function result value, in bytes (this count does not include itself). Can
//   be zero. As a special case, -1 indicates a NULL function result. No value bytes follow in the NULL
//       case.
// * Byten The value of the function result, in the format indicated by the associated format code. n is
//     the above length.

// GSSENCRequest (F)
//
// * Int32(8) Length of message contents in bytes, including self.
// * Int32(80877104) The GSSAPI Encryption request code. The value is chosen to contain 1234 in the most
// significant 16 bits, and 5680 in the least significant 16 bits. (To avoid confusion, this code must
// not be the same as any protocol version number.)

// GSSResponse (F)
// * Byte1('p') Identifies the message as a GSSAPI or SSPI response. Note that this is also used for
//   SASL and password response messages. The exact message type can be deduced from the context.
// * Int32 Length of message contents in bytes, including self.
// * Byten GSSAPI/SSPI specific message data.

// NegotiateProtocolVersion (B)
// * Byte1('v') Identifies the message as a protocol version negotiation message.
// * Int32 Length of message contents in bytes, including self.
// * Int32 Newest minor protocol version supported by the server for the major protocol version
//   requested by the client.
// * Int32 Number of protocol options not recognized by the server.
//
// Then, for protocol option not recognized by the server, there is the following:
//
// * String The option name.

// NoData (B)
// * Byte1('n') Identifies the message as a no-data indicator.
// * Int32(4) Length of message contents in bytes, including self.

// NoticeResponse (B)
// * Byte1('N') Identifies the message as a notice.
// * Int32 Length of message contents in bytes, including self.
//
// The message body consists of one or more identified fields, followed by a zero byte as a
// terminator. Fields can appear in any order. For each field there is the following:
//
// * Byte1 A code identifying the field type; if zero, this is the message terminator and no string
// follows. The presently defined field types are listed in Section 53.8. Since more field types might
// be added in future, frontends should silently ignore fields of unrecognized type.
// * String The field value.

// NotificationResponse (B)
// * Byte1('A') Identifies the message as a notification response.
// * Int32 Length of message contents in bytes, including self.
// * Int32 The process ID of the notifying backend process.
// * String The name of the channel that the notify has been raised on.
// * String The “payload” string passed from the notifying process.

// ParameterDescription (B)
// * Byte1('t') Identifies the message as a parameter description.
// * Int32 Length of message contents in bytes, including self.
// * Int16 The number of parameters used by the statement (can be zero).
//
// Then, for each parameter, there is the following:
//
// * Int32 Specifies the object ID of the parameter data type.

// ParameterStatus (B)
// * Byte1('S') Identifies the message as a run-time parameter status report.
// * Int32 Length of message contents in bytes, including self.
// * String The name of the run-time parameter being reported.
// * String The current value of the parameter.
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody, TryFromRawBackendMessage)]
#[message_body(kind = 'S')]
pub struct ParameterStatus {
    name: CString,
    value: CString,
}

impl ParameterStatus {
    pub fn new(name: &String, value: &String) -> anyhow::Result<Self> {
        Ok(Self {
            name: CString::new(&name[..])?,
            value: CString::new(&value[..])?,
        })
    }
}

// Parse (F)
// * Byte1('P') Identifies the message as a Parse command.
// * Int32 Length of message contents in bytes, including self.
// * String The name of the destination prepared statement (an empty string selects the unnamed prepared
//           statement).
// * String The query string to be parsed.
// * Int16 The number of parameter data types specified (can be zero). Note that this is not an
//     indication of the number of parameters that might appear in the query string, only the number
//     that the frontend wants to prespecify types for.
//
// Then, for each parameter, there is the following:
//
// * Int32 Specifies the object ID of the parameter data type. Placing a zero here is equivalent to
//     leaving the type unspecified.

// ParseComplete (B)
// * Byte1('1') Identifies the message as a Parse-complete indicator.
// * Int32(4) Length of message contents in bytes, including self.

// PasswordMessage (F)
// * Byte1('p') Identifies the message as a password response. Note that this is also used for GSSAPI,
// SSPI and SASL response messages. The exact message type can be deduced from the context.
// * Int32 Length of message contents in bytes, including self.
// * String The password (encrypted, if requested).
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody, TryFromRawFrontendMessage)]
#[message_body(kind = 'p')]
pub struct PasswordMessage {
    pub password: CString,
}

impl PasswordMessage {
    pub fn new(password: &String) -> anyhow::Result<Self> {
        Ok(Self {
            password: CString::new(&password[..])?,
        })
    }

    pub fn new_from_user_password(
        user: &String,
        password: &String,
        salt: &Byte4,
    ) -> anyhow::Result<Self> {
        let mut md5 = Md5::new();
        md5.update(password.as_bytes());
        md5.update(user.as_bytes());
        let hash = md5.finalize();
        let mut md5 = Md5::new();
        md5.update(format!("{hash:x}"));
        md5.update(salt);
        let hash = md5.finalize();
        let md5 = format!("md5{hash:x}");

        Self::new(&md5)
    }
}

// PortalSuspended (B)
// * Byte1('s') Identifies the message as a portal-suspended indicator. Note this only appears if an
//       Execute message's row-count limit was reached.
// * Int32(4) Length of message contents in bytes, including self.

// Query (F)
// * Byte1('Q') Identifies the message as a simple query.
// * Int32 Length of message contents in bytes, including self.
// * String The query string itself.
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody, TryFromRawFrontendMessage)]
#[message_body(kind = 'Q')]
pub struct Query {
    pub query: CString,
}

impl Query {
    pub fn new(query: String) -> anyhow::Result<Self> {
        Ok(Self {
            query: CString::new(&query[..])?,
        })
    }
}

// ReadyForQuery (B)
// * Byte1('Z') Identifies the message type. ReadyForQuery is sent whenever the backend is ready for a
//     new query cycle.
// * Int32(5) Length of message contents in bytes, including self.
// * Byte1 Current backend transaction status indicator. Possible values are 'I' if idle (not in a
//         transaction block); 'T' if in a transaction block; or 'E' if in a failed transaction block
//     (queries will be rejected until block is ended).
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody, TryFromRawBackendMessage)]
#[message_body(kind = 'Z')]
pub struct ReadyForQuery {
    pub transaction_indicator: Byte,
}

impl ReadyForQuery {
    pub fn new(transaction_indicator: TransactionIndicator) -> Self {
        Self {
            transaction_indicator: Byte::from(&transaction_indicator),
        }
    }
}

pub enum TransactionIndicator {
    Idle,
    IdleInTransaction,
    IdlerInTransactionAborted,
}

impl From<&Byte> for TransactionIndicator {
    fn from(item: &Byte) -> TransactionIndicator {
        match *item as char {
            'I' => TransactionIndicator::Idle,
            'T' => TransactionIndicator::IdleInTransaction,
            'E' => TransactionIndicator::IdlerInTransactionAborted,
            _ => unreachable!("Invalid value for TransactionIndicator"),
        }
    }
}

impl From<&TransactionIndicator> for Byte {
    fn from(item: &TransactionIndicator) -> Byte {
        match item {
            TransactionIndicator::Idle => 'I' as Byte,
            TransactionIndicator::IdleInTransaction => 'T' as Byte,
            TransactionIndicator::IdlerInTransactionAborted => 'E' as Byte,
        }
    }
}

// RowDescription (B)
// * Byte1('T') Identifies the message as a row description.
// * Int32 Length of message contents in bytes, including self.
// * Int16 Specifies the number of fields in a row (can be zero).
//
// Then, for each field, there is the following:
// * String The field name.
// * Int32 If the field can be identified as a column of a specific table, the object ID of the table;
// otherwise zero.
// * Int16 If the field can be identified as a column of a specific table, the attribute number of the
// column; otherwise zero.
// * Int32 The object ID of the field's data type.
// * Int16 The data type size (see pg_type.typlen). Note that negative values denote variable-width
//     types.
// * Int32 The type modifier (see pg_attribute.atttypmod). The meaning of the modifier is type-specific.
// * Int16 The format code being used for the field. Currently will be zero (text) or one (binary). In a
// RowDescription returned from the statement variant of Describe, the format code is not yet known
// and will always be zero.
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody, TryFromRawBackendMessage)]
#[message_body(kind = 'T')]
pub struct RowDescription {
    // The serialization will create a length field
    pub columns: Vec16<ColumnDescription>,
}

impl RowDescription {
    pub fn new(columns: Vec<ColumnDescription>) -> Self {
        Self {
            columns: columns.into(),
        }
    }
}

#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct ColumnDescription {
    pub name: CString,
    pub relation_id: i32,
    pub attribute_id: i16,
    pub datatype_id: i32,
    pub datatype_len: i16,
    pub datatype_mod: i32,
    pub format: i16,
}

impl ColumnDescription {
    pub fn new(name: &String, pgtype: PgType) -> anyhow::Result<Self> {
        Ok(Self {
            name: CString::new(&name[..])?,
            relation_id: 0,
            attribute_id: 0,
            datatype_id: i32::from(&pgtype),
            datatype_len: pgtype.typlen(),
            datatype_mod: pgtype.typmod(),
            format: pgtype.format(),
        })
    }
}

#[derive(Debug)]
pub enum PgType {
    Bool,
    Int4,
    Text,
    Oid,
}

impl From<&PgType> for i32 {
    fn from(pg_type: &PgType) -> Self {
        match pg_type {
            PgType::Bool => 16,
            PgType::Int4 => 23,
            PgType::Text => 25,
            PgType::Oid => 26,
        }
    }
}

impl PgType {
    pub fn typlen(&self) -> i16 {
        match &self {
            PgType::Bool => 1,
            PgType::Int4 => 4,
            PgType::Text => -1,
            PgType::Oid => 4,
        }
    }
    pub fn typmod(&self) -> i32 {
        match &self {
            PgType::Bool => -1,
            PgType::Int4 => -1,
            PgType::Text => -1,
            PgType::Oid => -1,
        }
    }
    pub fn format(&self) -> i16 {
        match &self {
            PgType::Bool => 0,
            PgType::Int4 => 0,
            PgType::Text => 1,
            PgType::Oid => 0,
        }
    }
}

// SASLInitialResponse (F)
// * Byte1('p') Identifies the message as an initial SASL response. Note that this is also used for
// GSSAPI, SSPI and password response messages. The exact message type is deduced from the context.
// * Int32 Length of message contents in bytes, including self.
// * String Name of the SASL authentication mechanism that the client selected.
// * Int32 Length of SASL mechanism specific "Initial Client Response" that follows, or -1 if there is
//     no Initial Response.
// * Byten SASL mechanism specific "Initial Response".
//TODO: implement

// SASLResponse (F)
// * Byte1('p') Identifies the message as a SASL response. Note that this is also used for GSSAPI, SSPI
//   and password response messages. The exact message type can be deduced from the context.
// * Int32 Length of message contents in bytes, including self.
// * Byten SASL mechanism specific message data.
//TODO: implement

// SSLRequest (F)
// * Int32(8) Length of message contents in bytes, including self.
// * Int32(80877103) The SSL request code. The value is chosen to contain 1234 in the most significant
// 16 bits, and 5679 in the least significant 16 bits. (To avoid confusion, this code must not be the
// same as any protocol version number.)
//TODO: implement

// StartupMessage (F)
//
// * Int32 Length of message contents in bytes, including self.
// * Int32(196608) The protocol version number. The most significant 16 bits are the major version
// number (3 for the protocol described here). The least significant 16 bits are the minor version
// number (0 for the protocol described here).
//
// The protocol version number is followed by one or more pairs of parameter name and value strings. A
// zero byte is required as a terminator after the last name/value pair. Parameters can appear in any
// order. user is required, others are optional. Each parameter is specified as:
//
// * String The parameter name. Currently recognized names are:
//   - user The database user name to connect as. Required; there is no default.
//   - database The database to connect to. Defaults to the user name.
//   - options Command-line arguments for the backend. (This is deprecated in favor of setting individual
// run-time parameters.) Spaces within this string are considered to separate arguments, unless
// escaped with a backslash (\); write \\ to represent a literal backslash.
//   - replication Used to connect in streaming replication mode, where a small set of replication
// commands can be issued instead of SQL statements. Value can be true, false, or database, and the
// default is false. See Section 53.4 for details.
//
// In addition to the above, other parameters may be listed. Parameter names beginning with _pq_. are
// reserved for use as protocol extensions, while others are treated as run-time parameters to be set
// at backend start time. Such settings will be applied during backend start (after parsing the
//     command-line arguments if any) and will act as session defaults.
//
// * String The parameter value.
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct StartupMessage {
    pub protocol_version: ProtocolVersion,
    pub parameters: VecNull<ParameterStatus>,
}

impl StartupMessage {
    pub fn new(protocol_version: ProtocolVersion, parameters: Vec<ParameterStatus>) -> Self {
        Self {
            protocol_version,
            parameters: parameters.into(),
        }
    }
}

impl RequestBody for StartupMessage {}

impl TryFrom<&mut RawRequest> for StartupMessage {
    type Error = anyhow::Error;

    fn try_from(request: &mut RawRequest) -> anyhow::Result<StartupMessage> {
        if let RequestMessageKind::StartupMessage = request.request_kind {
            StartupMessage::deserialize(&mut request.raw_body)
        } else {
            Err(anyhow!(
                "Impossible to create StarupMessage from RawRequest"
            ))
        }
    }
}

#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct ProtocolVersion {
    pub major: i16,
    pub minor: i16,
}

// Sync (F)
// * Byte1('S') Identifies the message as a Sync command.
// * Int32(4) Length of message contents in bytes, including self.

// Terminate (F)
// * Byte1('X') Identifies the message as a termination.
// * Int32(4)
// Length of message contents in bytes, including self.

#[cfg(test)]
mod test {
    use super::*;
    use bytes::{Bytes, BytesMut};
    use std::io::{BufReader, Cursor, Read};

    #[test]
    fn authentication_ok_serialize() -> anyhow::Result<()> {
        // serialize
        let m = AuthenticationOk::new();
        let h = MessageHeader {
            message_type: 'R' as u8,
            length: 4 + m.byte_size(),
        };

        let mut buffer = BytesMut::new();
        h.serialize(&mut buffer);
        m.serialize(&mut buffer);

        let e = vec![0x52, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00];

        assert_eq!(e, buffer.to_vec());

        Ok(())
    }

    #[test]
    fn authentication_ok_deserialize() -> anyhow::Result<()> {
        let m = AuthenticationOk::new();
        let h = MessageHeader {
            message_type: 'R' as u8,
            length: 4 + m.byte_size(),
        };

        let mut buffer = Bytes::from(vec![0x52, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00]);
        let h2 = MessageHeader::deserialize(&mut buffer)?;
        let m2 = AuthenticationOk::deserialize(&mut buffer)?;

        assert_eq!(m, m2);
        assert_eq!(h, h2);

        Ok(())
    }

    fn datarow_emptydata_deserialize() -> anyhow::Result<()> {
        // Empty Row Data message
        // 0x0050:                      4400 0000 0a00 0100  ........D.......
        // 0x0060:  0000 00
        let m = DataRow::new(Vec::<ColumnData>::from([ColumnData::new()]));
        let h = MessageHeader {
            message_type: 'D' as u8,
            length: 4 + m.byte_size(),
        };

        let mut buffer = Bytes::from(vec![
            0x44, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        ]);
        let h2 = MessageHeader::deserialize(&mut buffer)?;
        let m2 = DataRow::deserialize(&mut buffer)?;

        assert_eq!(m, m2);
        assert_eq!(h, h2);

        Ok(())
    }

    #[test]
    fn datarow_onebytecol_deserialize() -> anyhow::Result<()> {
        // Empty Row Data message
        // 0x0050:                      4400 0000 0a00 0100  ........D.......
        // 0x0060:  0000 00
        let col_data = Vec::<Byte>::from(['1' as u8]);
        let m = DataRow::new(Vec::<ColumnData>::from([ColumnData::from(col_data)]));
        let h = MessageHeader {
            message_type: 'D' as u8,
            length: 4 + m.byte_size(),
        };

        let mut buffer = Bytes::from(vec![
            0x44, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, '1' as u8,
        ]);
        let h2 = MessageHeader::deserialize(&mut buffer)?;
        let m2 = DataRow::deserialize(&mut buffer)?;

        assert_eq!(m, m2);
        assert_eq!(h, h2);

        Ok(())
    }
}
