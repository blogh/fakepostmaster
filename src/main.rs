#![allow(unused_imports)]

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, BufWriter, Read, Seek, Write},
    net::TcpListener,
    thread,
};

use bytes::{Buf, BufMut, Bytes, BytesMut};

use anyhow::anyhow;

trait CString {
    fn get_cstring(&mut self) -> String;
    fn put_cstring(&mut self, str: &String);
}

impl CString for BytesMut {
    fn get_cstring(&mut self) -> String {
        let mut s = String::new();
        let mut c: u8 = self.get_u8();

        while c != 0 {
            s.push(c as char);
            c = self.get_u8();
        }

        s
    }

    fn put_cstring(&mut self, str: &String) {
        let bytes = str.as_bytes();

        for c in bytes.iter() {
            self.put_u8(c.clone() as u8);
        }
        self.put_u8(0x00);
    }
}

fn send_message<R>(tcp_writer: &mut BufWriter<R>, message: BackendMessage) -> anyhow::Result<()>
where
    R: Write,
{
    println!("Send: {message:?}");
    let buf = message.compose()?;
    tcp_writer.write(&buf)?;
    tcp_writer.flush()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind("192.168.121.1:9092").unwrap();
    println!("Listening on 192.168.121.1:9092");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("accepted new connection");

                let mut tcp_reader =
                    BufReader::new(stream.try_clone().expect("Failed to clone TcpStream"));
                let mut tcp_writer = BufWriter::new(stream);

                // StartupMessage (ssl_mode ) prefer => Text Auth
                let sm = FrontendMessage::parse_startup_message(&mut tcp_reader)?;
                println!("Received: {sm:#?}");

                // Ask for the Password
                send_message(
                    &mut tcp_writer,
                    BackendMessage::AuthenticationMD5Password { salt: [1, 2, 3, 4] },
                )?;

                // PasswordMessage
                let pm = FrontendMessage::parse_password_message(&mut tcp_reader)?;
                println!("Received: {pm:#?}");

                //TODO:Logic to check if the md5 is good

                // Validate the authentication
                send_message(&mut tcp_writer, BackendMessage::AuthenticationOk)?;

                // Tell the client he can continue
                send_message(&mut tcp_writer, BackendMessage::ReadyForQuery)?;

                // Query?
                let q = FrontendMessage::parse_query(&mut tcp_reader)?;
                println!("Received: {q:#?}");

                // Tell the client the commadn tag
                send_message(
                    &mut tcp_writer,
                    BackendMessage::CommmandComplete {
                        command_tag: String::from("SELECT 1"),
                    },
                )?;

                // Tell the client he can continue
                send_message(&mut tcp_writer, BackendMessage::ReadyForQuery)?;
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
        println!("Request processed");
    }
    Ok(())
}

#[derive(Debug)]
pub enum BackendMessage {
    AuthenticationMD5Password { salt: [u8; 4] },
    AuthenticationOk,
    CommmandComplete { command_tag: String },
    ReadyForQuery,
}

impl BackendMessage {
    pub fn compose(&self) -> anyhow::Result<BytesMut> {
        match &self {
            Self::AuthenticationMD5Password { salt } => {
                self.compose_authentication_md5_password(*salt)
            }
            Self::AuthenticationOk => self.compose_authentication_ok(),
            Self::CommmandComplete { command_tag } => self.compose_command_complete(command_tag),
            Self::ReadyForQuery => self.compose_ready_for_query(),
        }
    }

    fn compose_authentication_md5_password(&self, salt: [u8; 4]) -> anyhow::Result<BytesMut> {
        let mut t = BytesMut::new();

        // Auth request
        t.put_u8('R' as u8);
        // Length
        t.put_i32(12);
        // ask for a md5 encrypted password
        t.put_i32(5);
        // a 4 bytes salt
        t.put_slice(&salt[..]);

        Ok(t)
    }

    fn compose_authentication_ok(&self) -> anyhow::Result<BytesMut> {
        let mut t = BytesMut::new();

        // Auth request
        t.put_u8('R' as u8);
        // Length
        t.put_i32(8);
        // ask for a md5 encrypted password
        t.put_i32(0);

        Ok(t)
    }

    fn compose_command_complete(&self, command_tag: &String) -> anyhow::Result<BytesMut> {
        let mut t = BytesMut::new();

        // Auth request
        t.put_u8('C' as u8);
        // Length
        t.put_i32(command_tag.len() as i32 + 4);
        // Status of the backend I: idle, T: idle in txn, E: failed in txn block
        // let buf = from_cstring(&self.command_tag)?;
        // t.put_slice(&buf.to_vec()[..]);
        t.put_cstring(command_tag);

        Ok(t)
    }

    fn compose_ready_for_query(&self) -> anyhow::Result<BytesMut> {
        let mut t = BytesMut::new();

        // Auth request
        t.put_u8('Z' as u8);
        // Length
        t.put_i32(5);
        // Status of the backend I: idle, T: idle in txn, E: failed in txn block
        t.put_u8('I' as u8);

        Ok(t)
    }
}

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
    fn parse_password_message<T>(tcp_reader: &mut BufReader<T>) -> anyhow::Result<Self>
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
    fn parse_query<T>(tcp_reader: &mut BufReader<T>) -> anyhow::Result<Self>
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
