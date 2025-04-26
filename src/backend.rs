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
pub struct RowDescription {
    pub name: String,
    pub relation_id: i32,  // id or 0
    pub attribute_id: i16, // id or 0
    pub datatype_id: i32,
    pub datatype_len: i16, // negative values denote variable-width types.
    pub datatype_mod: i32,
    pub format: i16, // 0  text 1 binay
}

#[derive(Debug)]
pub struct ErrorMessage {
    // Identifier: https://www.postgresql.org/docs/17/protocol-error-fields.html
    pub code: char,
    // The actual message
    pub message: String,
}

#[derive(Debug)]
pub enum BackendMessage {
    AuthenticationMD5Password { salt: [u8; 4] },
    AuthenticationOk,
    CommmandComplete { command_tag: String },
    DataRow { columns: Vec<String> },
    ReadyForQuery,
    RowDescription { columns: Vec<RowDescription> },
    ErrorResponse { messages: Vec<ErrorMessage> },
    ParameterStatus { parameter: String, value: String },
}

impl BackendMessage {
    pub fn compose(&self) -> anyhow::Result<BytesMut> {
        match &self {
            Self::AuthenticationMD5Password { salt } => {
                self.compose_authentication_md5_password(*salt)
            }
            Self::AuthenticationOk => self.compose_authentication_ok(),
            Self::CommmandComplete { command_tag } => self.compose_command_complete(command_tag),
            Self::DataRow { columns } => self.compose_data_row(columns),
            Self::ReadyForQuery => self.compose_ready_for_query(),
            Self::RowDescription { columns } => self.compose_row_description(columns),
            Self::ErrorResponse { messages } => self.compose_error_response(messages),
            Self::ParameterStatus { parameter, value } => {
                self.compose_parameter_status(parameter, value)
            }
        }
    }

    //TODO: needs test
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

    //TODO: needs test
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
        t.put_i32(command_tag.len() as i32 + 1 + 4);
        // Status of the backend I: idle, T: idle in txn, E: failed in txn block
        // let buf = from_cstring(&self.command_tag)?;
        // t.put_slice(&buf.to_vec()[..]);
        //FIXME: this shouldn't be a string
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

    //TODO: needs test
    fn compose_data_row(&self, columns: &Vec<String>) -> anyhow::Result<BytesMut> {
        //FIXME: Not tested
        let mut t = BytesMut::new();

        // Auth request
        t.put_u8('D' as u8);
        // Length
        t.put_i32(5);
        // Number of columns
        t.put_i16(i16::try_from(columns.len())?);
        // Columns
        for col in columns {
            t.put_i32(col.len() as i32);
            //FIXME: I should encode the type here
            t.put_cstring(&col);
        }

        Ok(t)
    }

    fn compose_row_description(&self, columns: &Vec<RowDescription>) -> anyhow::Result<BytesMut> {
        let mut t = BytesMut::new();
        let mut t2 = BytesMut::new();

        // Number of columns
        t2.put_i16(i16::try_from(columns.len())?);
        // Columns
        for col in columns {
            t2.put_cstring(&col.name);
            t2.put_i32(col.relation_id);
            t2.put_i16(col.attribute_id);
            t2.put_i32(col.datatype_id);
            t2.put_i16(col.datatype_len);
            t2.put_i32(col.datatype_mod);
            t2.put_i16(col.format);
        }

        // Auth request
        t.put_u8('T' as u8);
        // Length
        t.put_i32(4 + t2.len() as i32);
        t.extend_from_slice(&t2.to_vec());

        Ok(t)
    }

    //TODO: needs test
    fn compose_error_response(&self, messages: &Vec<ErrorMessage>) -> anyhow::Result<BytesMut> {
        let mut t = BytesMut::new();
        let mut t2 = BytesMut::new();

        // Messages
        for msg in messages {
            t2.put_u8(msg.code as u8);
            t2.put_cstring(&msg.message);
        }
        t2.put_u8(0x00);

        // Auth request
        t.put_u8('E' as u8);
        // Length
        t.put_i32(4 + t2.len() as i32);
        t.extend_from_slice(&t2.to_vec());

        Ok(t)
    }

    fn compose_parameter_status(
        &self,
        parameter: &String,
        value: &String,
    ) -> anyhow::Result<BytesMut> {
        let mut t = BytesMut::new();
        let mut t2 = BytesMut::new();

        // Messages
        t2.put_cstring(parameter);
        t2.put_cstring(value);

        // Auth request
        t.put_u8('S' as u8);
        // Length
        t.put_i32(4 + t2.len() as i32);
        t.extend_from_slice(&t2.to_vec());

        Ok(t)
    }
}

#[cfg(test)]
mod test_backend {
    use super::*;

    #[test]
    fn test_row_description() -> anyhow::Result<()> {
        // Row Descption message
        // 0x0030:            5400 0000 2300 0173 6574 5f63     T...#..set_c
        // 0x0040:  6f6e 6669 6700 0000 0000 0000 0000 0019  onfig...........
        // 0x0050:  ffff ffff ffff 0000o
        //
        // postgres=# SELECT oid, typname, typlen, typtypmod FROM pg_type WHERE oid = 25 \gx
        // -[ RECORD 1 ]---
        // oid       | 25
        // typname   | text
        // typlen    | -1
        // typtypmod | -1
        let bm = BackendMessage::RowDescription {
            columns: vec![RowDescription {
                name: String::from("set_config"),
                relation_id: 0,
                attribute_id: 0,
                datatype_id: 25,
                datatype_len: -1,
                datatype_mod: -1,
                format: 153,
            }],
        };
        assert_eq!(
            bm.compose()?.to_vec(),
            [
                0x54, 0x00, 0x00, 0x00, 0x23, 0x00, 0x01, 0x73, 0x65, 0x74, 0x5f, 0x63, 0x6f, 0x6e,
                0x66, 0x69, 0x67, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x19,
                0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00, 0x99
            ]
        );

        // Row Data message
        // 0x0050:                      4400 0000 0a00 0100  ........D.......
        // 0x0060:  0000 0043 0000 000d 5345 4c45 4354 2031  ...C....SELECT.1
        // 0x0070:  005a 0000 0005 49                        .Z....I

        // Command Complete message
        // 0x0060:         43 0000 000d 5345 4c45 4354 2031     C....SELECT.1
        // 0x0070:  005a 0000 0005 49                        .Z....I
        //
        Ok(())
    }

    #[test]
    fn test_parameter_status() -> anyhow::Result<()> {
        // Row Descption message
        // 0x0160:              53 0000 0034 7365 7276 6572       S...4server
        // 0x0170:  5f76 6572 7369 6f6e 0031 342e 3137 2028  _version.14.17.(
        // 0x0180:  4465 6269 616e 2031 342e 3137 2d31 2e70  Debian.14.17-1.p
        // 0x0190:  6764 6731 3230 2b31 2900                 gdg120+1).
        let bm = BackendMessage::ParameterStatus {
            parameter: String::from("server_version"),
            value: String::from("14.17 (Debian 14.17-1 pgdg120+1)"),
        };
        assert_eq!(
            bm.compose()?.to_vec(),
            [
                0x53, 0x00, 0x00, 0x00, 0x34, 's' as u8, 'e' as u8, 'r' as u8, 'v' as u8,
                'e' as u8, 'r' as u8, '_' as u8, 'v' as u8, 'e' as u8, 'r' as u8, 's' as u8,
                'i' as u8, 'o' as u8, 'n' as u8, 0x00, '1' as u8, '4' as u8, '.' as u8, '1' as u8,
                '7' as u8, ' ' as u8, '(' as u8, 'D' as u8, 'e' as u8, 'b' as u8, 'i' as u8,
                'a' as u8, 'n' as u8, ' ' as u8, '1' as u8, '4' as u8, '.' as u8, '1' as u8,
                '7' as u8, '-' as u8, '1' as u8, ' ' as u8, 'p' as u8, 'g' as u8, 'd' as u8,
                'g' as u8, '1' as u8, '2' as u8, '0' as u8, '+' as u8, '1' as u8, ')' as u8, 0x00
            ]
        );
        Ok(())
    }

    #[test]
    fn test_command_complete() -> anyhow::Result<()> {
        //  0x0060:         43 0000 000d 5345 4c45 4354 2031  ...C....SELECT.1
        //  0x0070:  00
        //FIXME: this shouldn't be a string
        let bm = BackendMessage::CommmandComplete {
            command_tag: String::from("SELECT 1"),
        };
        assert_eq!(
            bm.compose()?.to_vec(),
            [
                0x43, 0x00, 0x00, 0x00, 0x0d, 'S' as u8, 'E' as u8, 'L' as u8, 'E' as u8,
                'C' as u8, 'T' as u8, ' ' as u8, '1' as u8, 0x00
            ]
        );
        Ok(())
    }

    #[test]
    fn test_ready_for_query() -> anyhow::Result<()> {
        //  0x0070:    5a 0000 0005 49                         Z....I
        let bm = BackendMessage::ReadyForQuery;
        assert_eq!(bm.compose()?.to_vec(), [0x5a, 0x00, 0x00, 0x00, 0x05, 0x49]);
        Ok(())
    }
}
