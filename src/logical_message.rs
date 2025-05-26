use anyhow::anyhow;
use bytes::{BufMut, Bytes, BytesMut};
use libpq_serde_macros::SerdeLibpqData;
use libpq_serde_types::{
    ByteSized, Deserialize, Serialize,
    libpq_types::{Byte, Length16, Length32, VecWithEncoding},
};
use std::ffi::CString;

#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct LogicalHeader {
    pub message_type: i8,
}

impl LogicalHeader {
    pub fn get<T>(buffer: &mut Bytes) -> anyhow::Result<Self> {
        LogicalHeader::deserialize(buffer)
    }
}

#[derive(Debug)]
pub enum LogicalReplicationMessageKind {
    Begin,
    Message,
    Commit,
    Origin,
    Relation,
    Type,
    Insert,
    Update,
    Delete,
    Truncate,
    StreamStart,
    StreamStop,
    StreamCommit,
    StreamAbort,
    BeginPrepare,
    Prepare,
    CommitPrepared,
    RollbackPrepared,
    StreamPrepare,
}

impl From<&LogicalReplicationMessageKind> for i8 {
    fn from(msg_kind: &LogicalReplicationMessageKind) -> i8 {
        let msg_code = match msg_kind {
            LogicalReplicationMessageKind::Begin => 'B',
            LogicalReplicationMessageKind::Message => 'M',
            LogicalReplicationMessageKind::Commit => 'C',
            LogicalReplicationMessageKind::Origin => 'O',
            LogicalReplicationMessageKind::Relation => 'R',
            LogicalReplicationMessageKind::Type => 'Y',
            LogicalReplicationMessageKind::Insert => 'I',
            LogicalReplicationMessageKind::Update => 'U',
            LogicalReplicationMessageKind::Delete => 'D',
            LogicalReplicationMessageKind::Truncate => 'T',
            LogicalReplicationMessageKind::StreamStart => 'S',
            LogicalReplicationMessageKind::StreamStop => 'E',
            LogicalReplicationMessageKind::StreamCommit => 'c',
            LogicalReplicationMessageKind::StreamAbort => 'A',
            LogicalReplicationMessageKind::BeginPrepare => 'b',
            LogicalReplicationMessageKind::Prepare => 'P',
            LogicalReplicationMessageKind::CommitPrepared => 'K',
            LogicalReplicationMessageKind::RollbackPrepared => 'r',
            LogicalReplicationMessageKind::StreamPrepare => 'p',
        };
        msg_code as i8
    }
}

impl TryFrom<i8> for LogicalReplicationMessageKind {
    type Error = anyhow::Error;

    fn try_from(msg_code: i8) -> anyhow::Result<LogicalReplicationMessageKind> {
        match msg_code {
            0x42 /* 'B' */ => Ok(LogicalReplicationMessageKind::Begin),
            0x4d /* 'M' */ => Ok(LogicalReplicationMessageKind::Message),
            0x43 /* 'C' */ => Ok(LogicalReplicationMessageKind::Commit),
            0x4f /* 'O' */ => Ok(LogicalReplicationMessageKind::Origin),
            0x52 /* 'R' */ => Ok(LogicalReplicationMessageKind::Relation),
            0x59 /* 'Y' */ => Ok(LogicalReplicationMessageKind::Type),
            0x49 /* 'I' */ => Ok(LogicalReplicationMessageKind::Insert),
            0x55 /* 'U' */ => Ok(LogicalReplicationMessageKind::Update),
            0x44 /* 'D' */ => Ok(LogicalReplicationMessageKind::Delete),
            0x54 /* 'T' */ => Ok(LogicalReplicationMessageKind::Truncate),
            0x53 /* 'S' */ => Ok(LogicalReplicationMessageKind::StreamStart),
            0x45 /* 'E' */ => Ok(LogicalReplicationMessageKind::StreamStop),
            0x63 /* 'c' */ => Ok(LogicalReplicationMessageKind::StreamCommit),
            0x41 /* 'A' */ => Ok(LogicalReplicationMessageKind::StreamAbort),
            0x62 /* 'b' */ => Ok(LogicalReplicationMessageKind::BeginPrepare),
            0x50 /* 'P' */ => Ok(LogicalReplicationMessageKind::Prepare),
            0x4b /* 'K' */ => Ok(LogicalReplicationMessageKind::CommitPrepared),
            0x72 /* 'r' */ => Ok(LogicalReplicationMessageKind::RollbackPrepared),
            0x70 /* 'p' */ => Ok(LogicalReplicationMessageKind::StreamPrepare),
            _ => Err(anyhow!("Unsupported code for logical replication message: {msg_code}")),
        }
    }
}

// The list of messages can be found here and has been copied below (v17):
// * https://www.postgresql.org/docs/17/protocol-logicalrep-message-formats.html

// Begin
// * Byte1('B') Identifies the message as a begin message.
// * Int64 (XLogRecPtr) The final LSN of the transaction.
// * Int64 (TimestampTz) Commit timestamp of the transaction. The value is in number of microseconds
//       since PostgreSQL epoch (2000-01-01).
// * Int32 (TransactionId) Xid of the transaction.
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct Begin {
    pub final_lsn: i64,
    pub commit_timestamp: i64,
    pub txn_id: i32,
}

// Message
// * Byte1('M') Identifies the message as a logical decoding message.
// * Int32 (TransactionId) Xid of the transaction (only present for streamed transactions). This field
//      is available since protocol version 2.
// * Int8 Flags; Either 0 for no flags or 1 if the logical decoding message is transactional.
// * Int64 (XLogRecPtr) The LSN of the logical decoding message.
// * String The prefix of the logical decoding message.
// * Int32 Length of the content.
// * Byten The content of the logical decoding message.
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct Message {
    //NOTE: Only for streamed transaction
    //pub txn_id: i32,
    pub is_txn: i8,
    pub lsn: i64,
    pub prefix: CString,
    pub message: VecWithEncoding<Byte, Length32>,
}

// Commit
// * Byte1('C') Identifies the message as a commit message.
// * Int8(0) Flags; currently unused.
// * Int64 (XLogRecPtr) The LSN of the commit.
// * Int64 (XLogRecPtr) The end LSN of the transaction.
// * Int64 (TimestampTz) Commit timestamp of the transaction. The value is in number of microseconds
//     since PostgreSQL epoch (2000-01-01).
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct Commit {
    pub flag: i8,
    pub commit_lsn: i64,
    pub txn_end_lsn: i64,
    pub commit_timestamp: i64,
}

// Origin
// * Byte1('O') Identifies the message as an origin message.
// * Int64 (XLogRecPtr) The LSN of the commit on the origin server.
// * String Name of the origin.
// Note that there can be multiple Origin messages inside a single transaction.
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct Origin {
    pub commit_lsn_orig: i64,
    pub orig_name: CString,
}

// Relation
// * Byte1('R') Identifies the message as a relation message.
// * Int32 (TransactionId) Xid of the transaction (only present for streamed transactions). This field
//       is available since protocol version 2.
// * Int32 (Oid) OID of the relation.
// * String Namespace (empty string for pg_catalog).
// * String Relation name.
// * Int8 Replica identity setting for the relation (same as relreplident in pg_class).
// * Int16 Number of columns.
//
// Next, the following message part appears for each column included in the publication (except generated columns):
// * Int8 Flags for the column. Currently can be either 0 for no flags or 1 which marks the column as
//       part of the key.
// * String Name of the column.
// * Int32 (Oid) OID of the column's data type.
// * Int32 Type modifier of the column (atttypmod).
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct Relation {
    //NOTE: Only for streamed transaction
    //pub txn_id: i32,
    pub rel_oid: i32,
    pub namespace: CString,
    pub relname: CString,
    pub replica_identity: i8,
    pub columns: VecWithEncoding<ColumnDescription, Length16>,
}

#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct ColumnDescription {
    pub flag: i8,
    pub name: CString,
    pub type_oid: i32,
    pub typemod: i32,
}

//Type
//* Byte1('Y') Identifies the message as a type message.
//* Int32 (TransactionId) Xid of the transaction (only present for streamed transactions). This field
//      is available since protocol version 2.
//* Int32 (Oid) OID of the data type.
//* String Namespace (empty string for pg_catalog).
//* String Name of the data type.
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct Type {
    //NOTE: Only for streamed transaction
    //pub txn_id: i32,
    pub type_oid: i32,
    pub namespace: CString,
    pub type_name: CString,
}

// Insert
// * Byte1('I') Identifies the message as an insert message.
// * Int32 (TransactionId) Xid of the transaction (only present for streamed transactions). This field
//       is available since protocol version 2.
// * Int32 (Oid) OID of the relation corresponding to the ID in the relation message.
// * Byte1('N') Identifies the following TupleData message as a new tuple.
// * TupleData TupleData message part representing the contents of new tuple.
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct Insert {
    //NOTE: Only for streamed transaction
    //pub txn_id: i32,
    pub rel_oid: i32,
    pub new_tuple: Byte,
    pub new_tuple_data: TupleData,
}

//Update
//* Byte1('U') Identifies the message as an update message.
//* Int32 (TransactionId) Xid of the transaction (only present for streamed transactions). This field
//      is available since protocol version 2.
//* Int32 (Oid) OID of the relation corresponding to the ID in the relation message.
//* Byte1('K') Identifies the following TupleData submessage as a key. This field is optional and is
//      only present if the update changed data in any of the column(s) that are part of the REPLICA
//      IDENTITY index.
//* Byte1('O') Identifies the following TupleData submessage as an old tuple. This field is optional
//      and is only present if table in which the update happened has REPLICA IDENTITY set to FULL.
// * TupleData TupleData message part representing the contents of the old tuple or primary key. Only
//   present if the previous 'O' or 'K' part is present.
// * Byte1('N') Identifies the following TupleData message as a new tuple.
// * TupleData TupleData message part representing the contents of a new tuple.
//
// The Update message may contain either a 'K' message part or an 'O' message part or neither of them,
// but never both of them.
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct Update {
    //NOTE: Only for streamed transaction
    //pub txn_id: i32,
    pub rel_oid: i32,
    pub key_tuple: Byte,
    pub old_tuple: Byte,
    pub old_tuple_data: TupleData,
    pub new_tuple: Byte,
    pub new_tuple_data: TupleData,
}

// Delete
// * Byte1('D') Identifies the message as a delete message.
// * Int32 (TransactionId) Xid of the transaction (only present for streamed transactions). This field
//       is available since protocol version 2.
// * Int32 (Oid) OID of the relation corresponding to the ID in the relation message.
// * Byte1('K') Identifies the following TupleData submessage as a key. This field is present if the
//       table in which the delete has happened uses an index as REPLICA IDENTITY.
// * Byte1('O') Identifies the following TupleData message as an old tuple. This field is present if the
//   table in which the delete happened has REPLICA IDENTITY set to FULL.
// * TupleData TupleData message part representing the contents of the old tuple or primary key,
//   depending on the previous field.
//
// The Delete message may contain either a 'K' message part or an 'O' message part, but never both of them.
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct Delete {
    //NOTE: Only for streamed transaction
    //pub txn_id: i32,
    pub rel_oid: i32,
    pub key_tuple: Byte,
    pub old_tuple: Byte,
    pub old_tuple_data: TupleData,
}

// Truncate
// * Byte1('T') Identifies the message as a truncate message.
// * Int32 (TransactionId) Xid of the transaction (only present for streamed transactions). This field
//       is available since protocol version 2.
// * Int32 Number of relations
// * Int8 Option bits for TRUNCATE: 1 for CASCADE, 2 for RESTART IDENTITY
// * Int32 (Oid) OID of the relation corresponding to the ID in the relation message. This field is
//       repeated for each relation.
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct Truncate {
    //NOTE: Only for streamed transaction
    //pub txn_id: i32,
    pub rel_cnt: i32,
    pub flag: i8,
    //FIXME: The encoding is wrong here
    pub relations: VecWithEncoding<i32, Length32>,
}

// Stream Start
// * Byte1('S') Identifies the message as a stream start message.
// * Int32 (TransactionId) Xid of the transaction.
// * Int8 A value of 1 indicates this is the first stream segment for this XID, 0 for any other stream
// * segment.
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct StreamStart {
    pub txn_id: i32,
    pub first_segment: i8,
}

// Stream Stop
// * Byte1('E') Identifies the message as a stream stop message.
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct StreamStop {}

// Stream Commit
// * Byte1('c') Identifies the message as a stream commit message.
// * Int32 (TransactionId) Xid of the transaction.
// * Int8(0) Flags; currently unused.
// * Int64 (XLogRecPtr) The LSN of the commit.
// * Int64 (XLogRecPtr) The end LSN of the transaction.
// * Int64 (TimestampTz) Commit timestamp of the transaction. The value is in number of microseconds
//     since PostgreSQL epoch (2000-01-01).
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct StreamCommit {
    pub txn_id: i32,
    pub flag: i8,
    pub commit_lsn: i64,
    pub end_lsn: i64,
    pub commit_timestamp: i64,
}

// Stream Abort
// * Byte1('A') Identifies the message as a stream abort message.
// * Int32 (TransactionId) Xid of the transaction.
// * Int32 (TransactionId) Xid of the subtransaction (will be same as xid of the transaction for
//       top-level transactions).
// * Int64 (XLogRecPtr) The LSN of the abort. This field is available since protocol version 4.
// * Int64 (TimestampTz) Abort timestamp of the transaction. The value is in number of microseconds
//       since PostgreSQL epoch (2000-01-01). This field is available since protocol version 4.
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct StreamAbort {
    pub txn_id: i32,
    pub sub_txn_id: i32,
    pub abort_lsn: i64,
    pub abort_timestamp: i64,
}

// Begin Prepare
// * Byte1('b') Identifies the message as the beginning of a prepared transaction message.
// * Int64 (XLogRecPtr) The LSN of the prepare.
// * Int64 (XLogRecPtr) The end LSN of the prepared transaction.
// * Int64 (TimestampTz) Prepare timestamp of the transaction. The value is in number of microseconds
//       since PostgreSQL epoch (2000-01-01).
// * Int32 (TransactionId) Xid of the transaction.
// * String The user defined GID of the prepared transaction.

// Prepare
// * Byte1('P') Identifies the message as a prepared transaction message.
// * Int8(0) Flags; currently unused.
// * Int64 (XLogRecPtr) The LSN of the prepare.
// * Int64 (XLogRecPtr) The end LSN of the prepared transaction.
// * Int64 (TimestampTz) Prepare timestamp of the transaction. The value is in number of microseconds
//       since PostgreSQL epoch (2000-01-01).
// * Int32 (TransactionId) Xid of the transaction.
// * String The user defined GID of the prepared transaction.

// Commit Prepared
// * Byte1('K') Identifies the message as the commit of a prepared transaction message.
// * Int8(0) Flags; currently unused.
// * Int64 (XLogRecPtr) The LSN of the commit of the prepared transaction.
// * Int64 (XLogRecPtr) The end LSN of the commit of the prepared transaction.
// * Int64 (TimestampTz) Commit timestamp of the transaction. The value is in number of microseconds
//       since PostgreSQL epoch (2000-01-01).
// * Int32 (TransactionId) Xid of the transaction.
// * String The user defined GID of the prepared transaction.

// Rollback Prepared
// * Byte1('r') Identifies the message as the rollback of a prepared transaction message.
// * Int8(0) Flags; currently unused.
// * Int64 (XLogRecPtr) The end LSN of the prepared transaction.
// * Int64 (XLogRecPtr) The end LSN of the rollback of the prepared transaction.
// * Int64 (TimestampTz) Prepare timestamp of the transaction. The value is in number of microseconds
//       since PostgreSQL epoch (2000-01-01).
// * Int64 (TimestampTz) Rollback timestamp of the transaction. The value is in number of microseconds
//       since PostgreSQL epoch (2000-01-01).
// * Int32 (TransactionId) Xid of the transaction.
// * String The user defined GID of the prepared transaction.

// Stream Prepare
// * Byte1('p') Identifies the message as a stream prepared transaction message.
// * Int8(0) Flags; currently unused.
// * Int64 (XLogRecPtr) The LSN of the prepare.
// * Int64 (XLogRecPtr) The end LSN of the prepared transaction.
// * Int64 (TimestampTz) Prepare timestamp of the transaction. The value is in number of microseconds
//       since PostgreSQL epoch (2000-01-01).
// * Int32 (TransactionId) Xid of the transaction.
// * String The user defined GID of the prepared transaction.

// TupleData
// * Int16 Number of columns.
//
// Next, one of the following submessages appears for each column (except generated columns):
// *
//  - Byte1('n') Identifies the data as NULL value.
//  - Byte1('u') Identifies unchanged TOASTed value (the actual value is not sent).
//  - Byte1('t') Identifies the data as text formatted value.
//  - Byte1('b') Identifies the data as binary formatted value.
// * Int32 Length of the column value.
// * Byten The value of the column, either in binary or in text format. (As specified in the preceding
//   format byte). n is the above length.
#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct TupleData {
    columns: VecWithEncoding<ColumnData, Length16>,
}

#[derive(Debug, PartialEq, SerdeLibpqData)]
pub struct ColumnData {
    flag: Byte,
    column_value: VecWithEncoding<Byte, Length32>,
}
