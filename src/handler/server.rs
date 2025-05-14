use anyhow::anyhow;
use std::{
    io::{BufReader, BufWriter},
    net::TcpStream,
};
use tracing::*;

use crate::handler::{LibPqReader, LibPqWriter};
use crate::message::*;

pub struct TcpHandler {
    pub tcp_reader: BufReader<TcpStream>,
    pub tcp_writer: BufWriter<TcpStream>,
}

impl TcpHandler {
    pub fn new(stream: TcpStream) -> anyhow::Result<Self> {
        Ok(Self {
            tcp_reader: BufReader::new(stream.try_clone().expect("Failed to clone TcpStream")),
            tcp_writer: BufWriter::new(stream),
        })
    }

    //FIXME: Go Back to a HashMap
    pub fn md5_authentication_handler(
        &mut self,
        auth_function: &dyn Fn() -> bool,
    ) -> anyhow::Result<Vec<ParameterStatus>> {
        let sm = StartupMessage::try_from(&mut RawRequest::get(&mut self.tcp_reader)?)?;
        debug!("rcv: {sm:?}");

        //FIXME: random salt
        self.tcp_writer
            .put_message_and_flush(AuthenticationMD5Password::new([1, 2, 3, 4]))?;

        let mut raw_message = self.tcp_reader.get_raw_frontend_message()?;
        let _password_message = match PasswordMessage::try_from(&mut raw_message) {
            Ok(message) => {
                debug!("rcv: {message:?}");
                message
            }
            _ => return Err(anyhow!("Password message expected")),
        };

        if auth_function() {
            self.tcp_writer.put_message(AuthenticationOk::new())?;

            //FIXME: There should me much mode parameters to send back to the client..
            self.tcp_writer.put_message(ParameterStatus::new(
                &String::from("server_version"),
                &String::from("0.1 (fakepostmaster)"),
            )?)?;

            self.tcp_writer
                .put_message_and_flush(ReadyForQuery::new(TransactionIndicator::Idle))?;

            Ok(sm.parameters.into())
        } else {
            // Error out
            self.tcp_writer
                .put_message_and_flush(ErrorResponse::new(vec![ErrorMessage::new(
                    'M',
                    &String::from("Incorrect password or user"),
                )?]))?;

            Err(anyhow!("Auth failed"))
        }
    }

    pub fn simple_query_handler(
        &mut self,
        executor: &dyn Fn(String) -> (Vec<ColumnDescription>, Vec<ColumnData>, String),
    ) -> anyhow::Result<()> {
        //FIXME: See handler/client.rs
        let mut raw_message = self.tcp_reader.get_raw_frontend_message()?;
        let query_message = match Query::try_from(&mut raw_message) {
            Ok(message) => message,
            _ => return Err(anyhow!("Query message expected")),
        };
        debug!("rcv: {query_message:?}");

        let (column_desc, column_data, command_tag) = executor(query_message.query.into_string()?);

        self.tcp_writer
            .put_message(RowDescription::new(column_desc))?;

        if column_data.len() > 0 {
            self.tcp_writer.put_message(DataRow::new(column_data))?;
        }

        self.tcp_writer
            .put_message(CommandComplete::new(command_tag)?)?;

        self.tcp_writer
            .put_message_and_flush(ReadyForQuery::new(TransactionIndicator::Idle))?;

        Ok(())
    }
}
