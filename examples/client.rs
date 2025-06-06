use anyhow::anyhow;
use clap::{ArgAction, Args, Parser, Subcommand};
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
    #[command(flatten)]
    common_args: CommonArgs,
}

#[derive(Args, Debug)]
struct CommonArgs {
    // Use standart libpq defaults
    #[clap(long, default_value = "127.0.0.1")]
    host: String,
    #[clap(long, default_value = "5432")]
    port: i32,
    #[clap(long, default_value = "postgres")]
    database: String,
    #[clap(long, default_value = "postgres")]
    username: String,
    #[clap(long)]
    password: String,
    #[clap(short, long, action = ArgAction::SetTrue, default_value_t = false)]
    debug: bool,
}

#[derive(Subcommand)]
#[command(arg_required_else_help = true)]
enum Commands {
    Replication(ReplicationArgs),
    Query(QueryArgs),
}

#[derive(Args, Debug)]
struct QueryArgs {
    #[clap(short, long, action = ArgAction::SetTrue, default_value_t = false)]
    use_sample: bool,

    query_text: Vec<String>,
}

#[derive(Args, Debug)]
struct ReplicationArgs {
    #[clap(short, long, default_value = "0/0")]
    lsn: String,
    publication: String,
    slot: String,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_max_level(if cli.common_args.debug {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .compact()
        .init();

    let (replication, queries) = match cli.command {
        // https://www.postgresql.org/docs/17/protocol-replication.html#PROTOCOL-REPLICATION-START-REPLICATION-SLOT-LOGICAL
        // https://www.postgresql.org/docs/17/protocol-logical-replication.html
        // postgres=# SELECT  pg_create_logical_replication_slot('slot', 'pgoutput');
        //
        //  pg_create_logical_replication_slot
        // ------------------------------------
        //  (slot,0/1726E08)
        // (1 row)
        Some(Commands::Replication(repl_args)) => (
            "database",
            vec![format!(
                "START_REPLICATION SLOT {} LOGICAL {} (\
                    \"proto_version\" '{}',\
                    \"publication_names\" '{}',\
                    \"streaming\" 'off'\
                );",
                repl_args.slot, repl_args.lsn, 2, repl_args.publication,
            )],
        ),
        Some(Commands::Query(query_args)) => (
            "false",
            if query_args.use_sample {
                vec![
                    "BEGIN;".to_string(),
                    "SELECT 'hello world';".to_string(),
                    "SELECT oid, relname, relpages FROM pg_class LIMIT 1;".to_string(),
                    "ROLLBACK;".to_string(),
                    "COPY (SELECT * FROM pg_class LIMIT 2) TO STDOUT;".to_string(),
                    "COPY (SELECT * FROM pg_class WHERE false) TO STDOUT;".to_string(),
                    "DROP TABLE IF EXISTS tovacuum ".to_string(),
                    "CREATE TABLE tovacuum AS SELECT generate_series(1, 10000);".to_string(),
                    "VACUUM ANALYZE tovacuum".to_string(),
                    "des bétises pour avoir une erreur".to_string(),
                    "".to_string(),
                ]
            } else {
                query_args.query_text
            },
        ),
        None => return Err(anyhow!("Choose either query or replication mode")),
    };

    let stream = TcpStream::connect(format!("{}:{}", cli.common_args.host, cli.common_args.port));
    info!(
        "Connecting to {}:{}...",
        cli.common_args.host, cli.common_args.port
    );

    match stream {
        Ok(stream) => {
            info!("Connection established");
            let mut handler = ClientMachine::connect(
                stream,
                &cli.common_args.username,
                &cli.common_args.password,
                &cli.common_args.database,
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
