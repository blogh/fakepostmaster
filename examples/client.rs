use anyhow::anyhow;
use clap::{Parser, Subcommand};
use std::net::TcpStream;
use tracing::*;
use tracing_subscriber;

use fakepostmaster::handler::client::*;

// Example of a client application.

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
#[command(arg_required_else_help = true)]
enum Commands {
    Replication,
    Query,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .compact()
        .init();

    let cli = Cli::parse();
    let (user, password, database, replication, queries) = match cli.command {
        // https://www.postgresql.org/docs/17/protocol-replication.html#PROTOCOL-REPLICATION-START-REPLICATION-SLOT-LOGICAL
        // https://www.postgresql.org/docs/17/protocol-logical-replication.html
        // postgres=# SELECT  pg_create_logical_replication_slot('slot', 'pgoutput');
        //
        //  pg_create_logical_replication_slot
        // ------------------------------------
        //  (slot,0/1726E08)
        // (1 row)
        Some(Commands::Replication) => (
            "md5userrl",
            "md5passrl",
            "postgres",
            "database",
            vec![format!(
                "START_REPLICATION SLOT {} LOGICAL {} (\
                    \"proto_version\" '{}',\
                    \"publication_names\" '{}',\
                    \"streaming\" 'off'\
                );",
                "slot", "0/1726E08", 2, "pub",
            )],
        ),
        Some(Commands::Query) => (
            "md5user",
            "md5pass",
            "postgres",
            "false",
            vec![
                "BEGIN;".to_string(),
                "SELECT 'hello world';".to_string(),
                "SELECT oid, relname, relpages FROM pg_class LIMIT 1;".to_string(),
                "ROLLBACK;".to_string(),
                "COPY (SELECT * FROM pg_class LIMIT 2) TO STDOUT;".to_string(),
                "COPY (SELECT * FROM pg_class WHERE false) TO STDOUT;".to_string(),
                "VACUUM".to_string(),
                "beuarg".to_string(),
            ],
        ),
        None => return Err(anyhow!("Choose either query or replication mode")),
    };

    let server = "pgsrv";
    let port = 5432;
    let stream = TcpStream::connect(format!("{server}:{port}"));
    info!("Connecting to {server}:{port}...");

    match stream {
        Ok(stream) => {
            info!("Connection established");
            let mut handler = ClientMachine::connect(
                stream,
                user,
                password,
                database,
                replication,
                "fake client",
                queries,
            )?;

            loop {
                match handler.next()? {
                    Context::Disconnected => break,
                    _ => (),
                }
            }
            info!("Connection ended");
        }
        Err(e) => {
            println!("error: {}", e);
        }
    }
    Ok(())
}
