use anyhow::anyhow;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, BufWriter, Read, Seek, Write},
    net::TcpListener,
    thread,
};

use crate::bytes::CString;

#[derive(Debug)]
pub enum FrontendMessage {
    StartupMessage {
        length: i32,
        protocol_version: (i16, i16),
        parameters: HashMap<String, String>,
    },
    PasswordMessage {
        kind: char,
        length: i32,
        password: String,
    },
    Query {
        kind: char,
        length: i32,
        query: String,
    },
}

impl FrontendMessage {
    /// Parses a FrontendMessage::StartupMessage
    //TODO: needs test
    pub fn parse_startup_message<T>(tcp_reader: &mut BufReader<T>) -> anyhow::Result<Self>
    where
        T: Read,
    {
        let mut buf = [0u8; 4];
        tcp_reader.read_exact(&mut buf)?;
        let length = i32::from_be_bytes(buf);

        let mut buf = vec![0u8; length as usize - 4];
        tcp_reader.read_exact(&mut buf)?;
        let mut buf = BytesMut::from(&buf[..]);

        let protocol_version = (buf.get_i16(), buf.get_i16());

        let mut parameters = HashMap::new();
        while buf.has_remaining() {
            // let parm = get_cstring(&mut buf)?;
            let parm = buf.get_cstring();
            if parm.len() == 0 {
                break;
            }
            // let value = get_cstring(&mut buf)?;
            let value = buf.get_cstring();

            parameters.insert(parm, value);
        }

        Ok(Self::StartupMessage {
            length,
            protocol_version,
            parameters,
        })
    }

    /// Parses a FrontendMessage::PasswordMessage
    //TODO: needs test
    pub fn parse_password_message<T>(tcp_reader: &mut BufReader<T>) -> anyhow::Result<Self>
    where
        T: Read,
    {
        let mut kind = [0u8];
        tcp_reader.read_exact(&mut kind)?;
        let kind = kind[0] as char;
        if kind != 'p' {
            Err(anyhow!("Incorrect PasswordMessage provided"))
        } else {
            let mut buf = [0u8; 4];
            tcp_reader.read_exact(&mut buf)?;
            let length = i32::from_be_bytes(buf);

            let mut buf = vec![0u8; length as usize - 4];
            tcp_reader.read_exact(&mut buf)?;
            let mut buf = BytesMut::from(&buf[..]);
            //let password = get_cstring(&mut buf)?;
            let password = buf.get_cstring();

            Ok(Self::PasswordMessage {
                kind,
                length,
                password,
            })
        }
    }

    /// Parses a FrontendMessage::Query
    //TODO: needs test
    pub fn parse_query<T>(tcp_reader: &mut BufReader<T>) -> anyhow::Result<Self>
    where
        T: Read,
    {
        let mut kind = [0u8];
        tcp_reader.read_exact(&mut kind)?;
        let kind = kind[0] as char;
        if kind != 'Q' {
            Err(anyhow!("Incorrect Query provided"))
        } else {
            let mut buf = [0u8; 4];
            tcp_reader.read_exact(&mut buf)?;
            let length = i32::from_be_bytes(buf);

            let mut buf = vec![0u8; length as usize - 4];
            tcp_reader.read_exact(&mut buf)?;
            let mut buf = BytesMut::from(&buf[..]);
            //let query = get_cstring(&mut buf)?;
            let query = buf.get_cstring();

            Ok(Self::Query {
                kind,
                length,
                query,
            })
        }
    }
}
