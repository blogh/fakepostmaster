use anyhow::anyhow;
use bytes::Bytes;
use libpq_serde_macros::{MessageBody, SerdeLibpqData};
use libpq_serde_types::{Deserialize, libpq_types::Byte};

// This file contains all the messages from the strealing replication. The are packed inside a
// CopyData message usually in the CopyBoth context (an briefly on CopyIn CopyOut when we stop
// the replication.)
//
// The list of messages can be found here and has been copied below (v17):
// * https://www.postgresql.org/docs/17/protocol-replication.html
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct StreamingHeader {
    pub message_type: i8,
}

impl StreamingHeader {
    pub fn get<T>(buffer: &mut Bytes) -> anyhow::Result<Self> {
        StreamingHeader::deserialize(buffer)
    }
}

#[derive(Debug)]
pub enum StreamingReplicationMessageKind {
    XLogData,
    PrimaryKeepAliveMessage,
    StandbyStatusUpdate,
    HotStandbyFeedbackMessage,
}

impl From<&StreamingReplicationMessageKind> for i8 {
    fn from(msg_kind: &StreamingReplicationMessageKind) -> i8 {
        let msg_code = match msg_kind {
            StreamingReplicationMessageKind::XLogData => 'w',
            StreamingReplicationMessageKind::PrimaryKeepAliveMessage => 'k',
            StreamingReplicationMessageKind::StandbyStatusUpdate => 'r',
            StreamingReplicationMessageKind::HotStandbyFeedbackMessage => 'h',
        };
        msg_code as i8
    }
}

impl TryFrom<i8> for StreamingReplicationMessageKind {
    type Error = anyhow::Error;

    fn try_from(msg_code: i8) -> anyhow::Result<StreamingReplicationMessageKind> {
        match msg_code {
            0x77 /* 'w' */ => Ok(StreamingReplicationMessageKind::XLogData),
            0x6b /* 'k' */ => Ok(StreamingReplicationMessageKind::PrimaryKeepAliveMessage),
            0x72 /* 'r' */ => Ok(StreamingReplicationMessageKind::StandbyStatusUpdate),
            0x68 /* 'h' */ => Ok(StreamingReplicationMessageKind::HotStandbyFeedbackMessage),
            _ => Err(anyhow!("Unsupported code for physical replication message: {msg_code}")),
        }
    }
}

// The list of messages can be found here and has been copied below (v17):
// * https://www.postgresql.org/docs/17/protocol-replication.html

// XLogData (B)
// * Byte1('w') Identifies the message as WAL data.
// * Int64 The starting point of the WAL data in this message.
// * Int64 The current end of WAL on the server.
// * Int64 The server's system clock at the time of transmission, as microseconds since midnight on
//   2000-01-01.
// * Byten A section of the WAL data stream.
//
// A single WAL record is never split across two XLogData messages. When a WAL record crosses a WAL
// page boundary, and is therefore already split using continuation records, it can be split at the
// page boundary. In other words, the first main WAL record and its continuation records can be sent
// in different XLogData messages.
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody)]
#[message_body(kind = 'w')]
pub struct XLogData {
    pub wal_data_start: i64,
    pub end_of_wal: i64,
    pub timestamp: i64,
    //NOTE: The wal data stream is implemented as a logical message
}

// Primary keepalive message (B)
// * Byte1('k') Identifies the message as a sender keepalive.
// * Int64 The current end of WAL on the server.
// * Int64 The server's system clock at the time of transmission, as microseconds since midnight on
//   2000-01-01.
// * Byte1 1 means that the client should reply to this message as soon as possible, to avoid a timeout
//     disconnect. 0 otherwise.
//
// The receiving process can send replies back to the sender at any time, using one of the following
// message formats (also in the payload of a CopyData message):
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody)]
#[message_body(kind = 'k')]
pub struct PrimaryKeepAliveMessage {
    pub end_of_wal: i64,
    pub timestamp: i64,
    pub urgency: Byte,
}

// Standby status update (F)
// * Byte1('r') Identifies the message as a receiver status update.
// * Int64 The location of the last WAL byte + 1 received and written to disk in the standby.
// * Int64 The location of the last WAL byte + 1 flushed to disk in the standby.
// * Int64 The location of the last WAL byte + 1 applied in the standby.
// * Int64 The client's system clock at the time of transmission, as microseconds since midnight on
//   2000-01-01.
// * Byte1 If 1, the client requests the server to reply to this message immediately. This can be used
//   to ping the server, to test if the connection is still healthy.
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody)]
#[message_body(kind = 'r')]
pub struct StandbyStatusUpdate {
    pub reveived_lsn: i64,
    pub flush_lsn: i64,
    pub applied_lsn: i64,
    pub timestamp: i64,
    pub urgency: Byte,
}

// Hot standby feedback message (F)
// * Byte1('h') Identifies the message as a hot standby feedback message.
//   Int64 The client's system clock at the time of transmission, as microseconds since midnight on
//   2000-01-01.
// * Int32 The standby's current global xmin, excluding the catalog_xmin from any replication slots. If
//   both this value and the following catalog_xmin are 0, this is treated as a notification that hot
//   standby feedback will no longer be sent on this connection. Later non-zero messages may reinitiate
//   the feedback mechanism.
// * Int32 The epoch of the global xmin xid on the standby.
// * Int32 The lowest catalog_xmin of any replication slots on the standby. Set to 0 if no catalog_xmin
//   exists on the standby or if hot standby feedback is being disabled.
// * Int32 The epoch of the catalog_xmin xid on the standby.
#[derive(Debug, PartialEq, SerdeLibpqData, MessageBody)]
#[message_body(kind = 'h')]
pub struct HotStandbyFeedBackMessage {
    pub xmin: i32,
    pub xmin_xid_epoch: i32,
    pub lowest_xmin: i32,
    pub lowest_xmin_epoch: i32,
}
