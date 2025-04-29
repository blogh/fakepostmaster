pub mod client;
pub mod server;

use anyhow::anyhow;
use std::io::{BufReader, BufWriter, Read, Write};
use std::net::TcpStream;

use bytes::{BufMut, Bytes, BytesMut};

use libpq_serde_types::{ByteSized, Deserialize, Serialize};

use crate::message::*;

trait LibPqReader: Read {
    fn get_raw_backend_message(&mut self) -> anyhow::Result<RawBackendMessage>;
    fn get_raw_frontend_message(&mut self) -> anyhow::Result<RawFrontendMessage>;
}

impl<T> LibPqReader for BufReader<T>
where
    T: Read,
{
    fn get_raw_backend_message(&mut self) -> anyhow::Result<RawBackendMessage> {
        let mut raw_message = RawBackendMessage::get(self)?;
        if let Some(BackendMessageKind::ErrorResponse) = raw_message.get_message_kind() {
            let error = ErrorResponse::try_from(&mut raw_message)?;
            //FIXME:
            dbg!(error);
            Err(anyhow!("Error"))
        } else {
            Ok(raw_message)
        }
    }

    fn get_raw_frontend_message(&mut self) -> anyhow::Result<RawFrontendMessage> {
        Ok(RawFrontendMessage::get(self)?)
    }
}

trait LibPqWriter: Write {
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
    fn put_message<U>(&mut self, msg: U) -> anyhow::Result<()>
    where
        U: MessageBody + Serialize + ByteSized + std::fmt::Debug,
    {
        let mut buffer = BytesMut::new();
        MessageHeader::new_raw_header_from_body(&mut buffer, &msg);
        msg.serialize(&mut buffer);
        println!("{msg:#?}");
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
        println!("{msg:#?}");

        let mut buffer = BytesMut::new();
        buffer.put_i32(msg.byte_size() + 4);
        msg.serialize(&mut buffer);
        self.write(&buffer)?;
        self.flush()?;

        Ok(())
    }
}
