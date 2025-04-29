use anyhow::anyhow;
use std::{
    io::{BufReader, BufWriter},
    net::TcpStream,
};

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
        // StartupMessage: (ssl_mode) prefer => Text Auth
        let sm = StartupMessage::try_from(&mut RawRequest::get(&mut self.tcp_reader)?)?;
        println!("{sm:#?}");

        // Ask for the Password
        //FIXME: random salt
        self.tcp_writer
            .put_message_and_flush(AuthenticationMD5Password::new([1, 2, 3, 4]))?;

        // PasswordMessage
        let mut raw_message = self.tcp_reader.get_raw_frontend_message()?;
        let _password_message = match PasswordMessage::try_from(&mut raw_message) {
            Ok(message) => message,
            _ => return Err(anyhow!("Password message expected")),
        };

        if auth_function() {
            // Validate the authentication
            self.tcp_writer.put_message(AuthenticationOk::new())?;

            // Validate the authentication
            //FIXME: There should me much mode parameters to send back to the client..
            self.tcp_writer.put_message(ParameterStatus::new(
                &String::from("server_version"),
                &String::from("0.1 (fakepostmaster)"),
            )?)?;

            // Tell the client he can continue
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
        // Query?
        let mut raw_message = self.tcp_reader.get_raw_frontend_message()?;
        let query_message = match Query::try_from(&mut raw_message) {
            Ok(message) => message,
            _ => return Err(anyhow!("Query message expected")),
        };

        // execute query
        let (column_desc, column_data, command_tag) = executor(query_message.query.into_string()?);

        // row description
        self.tcp_writer
            .put_message(RowDescription::new(column_desc))?;

        // data row
        if column_data.len() > 0 {
            self.tcp_writer.put_message(DataRow::new(column_data))?;
        }

        // Tell the client the commadn tag
        self.tcp_writer
            .put_message(CommandComplete::new(command_tag)?)?;

        // Tell the client he can continue
        self.tcp_writer
            .put_message_and_flush(ReadyForQuery::new(TransactionIndicator::Idle))?;

        Ok(())
    }
}
