pub mod client;
pub mod passthru;
//pub mod server;

use anyhow::anyhow;
use bytes::{BufMut, Bytes, BytesMut};
use std::collections::HashMap;
use std::ffi::CString;
use std::io::{Read, Write};
use std::net::TcpStream;

use tracing::*;

use libpq_serde_types::{ByteSized, Deserialize, Serialize, libpq_types};

use crate::message::*;

trait LibPqReader: Read {
    fn get_raw_backend_message(&mut self) -> anyhow::Result<RawMessage>;
    fn get_raw_frontend_message(&mut self) -> anyhow::Result<RawMessage>;
}

impl LibPqReader for TcpStream {
    fn get_raw_backend_message(&mut self) -> anyhow::Result<RawMessage> {
        let raw_message = RawMessage::get(self)?;
        debug!("{:?}", BackendMessageKind::try_from(&raw_message.kind)?);
        Ok(raw_message)
    }

    fn get_raw_frontend_message(&mut self) -> anyhow::Result<RawMessage> {
        let raw_message = RawMessage::get(self)?;
        debug!("{:?}", FrontendMessageKind::try_from(&raw_message.kind)?);
        Ok(raw_message)
    }
}

trait LibPqWriter: Write {
    fn put_raw_message(&mut self, msg: RawMessage) -> anyhow::Result<()>;

    fn put_message<U>(&mut self, msg: U) -> anyhow::Result<()>
    where
        U: MessageBody + Serialize + ByteSized + std::fmt::Debug;

    fn put_request<U>(&mut self, msg: U) -> anyhow::Result<()>
    where
        U: RequestBody + Serialize + ByteSized + std::fmt::Debug;
}

impl LibPqWriter for TcpStream {
    fn put_raw_message(&mut self, msg: RawMessage) -> anyhow::Result<()> {
        self.write(&msg.raw_header)?;
        self.write(&msg.raw_body)?;
        Ok(())
    }

    fn put_message<U>(&mut self, msg: U) -> anyhow::Result<()>
    where
        U: MessageBody + Serialize + ByteSized + std::fmt::Debug,
    {
        debug!("snd: {msg:?}");

        let mut buffer = BytesMut::new();
        MessageHeader::new_raw_header_from_body(&mut buffer, &msg);
        msg.serialize(&mut buffer);
        self.write(&buffer)?;

        Ok(())
    }

    fn put_request<U>(&mut self, msg: U) -> anyhow::Result<()>
    where
        U: RequestBody + Serialize + ByteSized + std::fmt::Debug,
    {
        debug!("snd: {msg:?}");

        let mut buffer = BytesMut::new();
        buffer.put_i32(msg.byte_size() + 4);
        msg.serialize(&mut buffer);
        self.write(&buffer)?;
        self.flush()?;

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PgType {
    Bool,
    Char,
    Name,
    Int8,
    Int4,
    Text,
    Oid,
}

impl TryFrom<i32> for PgType {
    type Error = anyhow::Error;
    fn try_from(value: i32) -> anyhow::Result<PgType> {
        match value {
            16 => Ok(PgType::Bool),
            18 => Ok(PgType::Char),
            19 => Ok(PgType::Name),
            20 => Ok(PgType::Int8),
            23 => Ok(PgType::Int4),
            25 => Ok(PgType::Text),
            26 => Ok(PgType::Oid),
            _ => Err(anyhow!("Unsupported PostgreSQL Type: {value}")),
        }
    }
}

impl From<PgType> for i32 {
    fn from(value: PgType) -> i32 {
        match value {
            PgType::Bool => 16,
            PgType::Char => 18,
            PgType::Name => 19,
            PgType::Int8 => 20,
            PgType::Int4 => 23,
            PgType::Text => 25,
            PgType::Oid => 26,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PgToRustTypes {
    Bool(bool),
    Char(char),
    Name(String),
    Int8(i32),
    Int4(i16),
    Text(String),
    Oid(u32),
}

pub fn decode_from_text(data: &Bytes, pg_type: &PgType) -> anyhow::Result<PgToRustTypes> {
    //FIXME: quick and dirty hack
    let mut data = data.clone();
    match pg_type {
        PgType::Bool => {
            let t = match u8::deserialize(&mut data)? as char {
                't' => true,
                'f' => false,
                _ => return Err(anyhow!("Unsupported value for boolean")),
            };
            Ok(PgToRustTypes::Bool(t))
        }
        PgType::Char => {
            let t = u8::deserialize(&mut data)? as char;
            Ok(PgToRustTypes::Char(t))
        }
        PgType::Name => Ok(PgToRustTypes::Name(String::from_utf8(data.to_vec())?)),
        PgType::Int8 => Ok(PgToRustTypes::Int8(
            String::from_utf8(data.to_vec())?.parse()?,
        )),
        PgType::Int4 => Ok(PgToRustTypes::Int4(
            String::from_utf8(data.to_vec())?.parse()?,
        )),
        PgType::Text => Ok(PgToRustTypes::Text(String::from_utf8(data.to_vec())?)),
        PgType::Oid => Ok(PgToRustTypes::Oid(
            String::from_utf8(data.to_vec())?.parse()?,
        )),
    }
}
