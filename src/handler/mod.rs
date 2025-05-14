pub mod client;
pub mod lclient;
pub mod passthru;
pub mod server;

use anyhow::anyhow;
use bytes::{BufMut, Bytes, BytesMut};
use std::io::{BufReader, BufWriter, Read, Write};
use std::net::TcpStream;

use tracing::*;
use tracing_subscriber;

use libpq_serde_types::{ByteSized, Deserialize, Serialize};

use crate::message::*;

trait LibPqReader: Read {
    fn get_raw_backend_message(&mut self) -> anyhow::Result<RawMessage>;
    fn get_raw_frontend_message(&mut self) -> anyhow::Result<RawMessage>;
}

impl<T> LibPqReader for BufReader<T>
where
    T: Read,
{
    fn get_raw_backend_message(&mut self) -> anyhow::Result<RawMessage> {
        let mut raw_message = RawMessage::get(self)?;
        if let BackendMessageKind::ErrorResponse = BackendMessageKind::try_from(&raw_message.kind)?
        {
            let error = ErrorResponse::try_from(&mut raw_message)?;
            //FIXME:
            error!("{error:?}");
            Err(anyhow!("Error"))
        } else {
            Ok(raw_message)
        }
    }

    fn get_raw_frontend_message(&mut self) -> anyhow::Result<RawMessage> {
        Ok(RawMessage::get(self)?)
    }
}

trait LibPqWriter: Write {
    fn put_raw_message(&mut self, msg: RawMessage) -> anyhow::Result<()>;
    fn put_raw_message_and_flush(&mut self, msg: RawMessage) -> anyhow::Result<()>;

    fn put_message<U>(&mut self, msg: U) -> anyhow::Result<()>
    where
        U: MessageBody + Serialize + ByteSized + std::fmt::Debug;

    fn put_message_and_flush<U>(&mut self, msg: U) -> anyhow::Result<()>
    where
        U: MessageBody + Serialize + ByteSized + std::fmt::Debug;

    fn put_request<U>(&mut self, msg: U) -> anyhow::Result<()>
    where
        U: RequestBody + Serialize + ByteSized + std::fmt::Debug;
}

impl<T> LibPqWriter for BufWriter<T>
where
    T: Write,
{
    fn put_raw_message(&mut self, msg: RawMessage) -> anyhow::Result<()> {
        self.write(&msg.raw_header)?;
        self.write(&msg.raw_body)?;
        Ok(())
    }

    fn put_raw_message_and_flush(&mut self, msg: RawMessage) -> anyhow::Result<()> {
        self.write(&msg.raw_header)?;
        self.write(&msg.raw_body)?;
        self.flush()?;
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

    fn put_message_and_flush<U>(&mut self, msg: U) -> anyhow::Result<()>
    where
        U: MessageBody + Serialize + ByteSized + std::fmt::Debug,
    {
        self.put_message(msg)?;
        self.flush()?;

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
