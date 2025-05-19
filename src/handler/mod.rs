pub mod client;
pub mod passthru;
//pub mod server;

use bytes::{BufMut, BytesMut};
use std::io::{Read, Write};
use std::net::TcpStream;

use tracing::*;

use libpq_serde_types::{ByteSized, Serialize};

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
