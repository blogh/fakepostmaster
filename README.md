# Info

## What?

Do a mini server that process the libpq authentication part.

* The C implementation is here: `PQconnectPoll()` (`src/interfaces/libpq/fe-connect.c`)
* A Rust implementation example is [`pg_cat`](https://github.com/postgresml/pgcat/blob/main/src/client.rs#L324)
  (or any other pooler)

Doc:

* https://www.postgresql.org/docs/17/protocol-flow.html#PROTOCOL-FLOW-START-UP
* https://www.postgresql.org/docs/17/protocol-message-types.html
* https://www.postgresql.org/docs/17/protocol-message-formats.html

## Why?

* it's fun
* it could be useful for the "logical anonymization" of pg_anon

## So?

It's rought, works only with non TLS connexions, not async, mono-threaded and there is no tests.
But it's a half day effort so...

# Example

## Normal connection

On a first session, start the server:

```bash
cargo run
```
```text
   Compiling fakepostmaster v0.1.0 (/home/benoit/Documents/mystuff/dev/rust/tests/fakepostmaster)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.27s
     Running `target/debug/fakepostmaster`
Listening on 192.168.121.1:9092
```

In another session (PGPASSWORD is necessary, we  fail otherwise):

```bash
PGPASSWORD=benoit psql "host=192.168.121.1 port=9092 sslmode=allow"
```
```text
psql (17.4 (Debian 17.4-1.pgdg120+2), server 0.0.0)
WARNING: psql major version 17, server major version 0.0.
         Some psql features might not work.
Type "help" for help.

192.168.121.1:9092 benoit@benoit=>
```
Note: The server version is not correct.

On the server session:

```text
accepted new connection
Received: StartupMessage {
    length: 80,
    protocol_version: (
        3,
        0,
    ),
    parameters: {
        "database": "benoit",
        "user": "benoit",
        "application_name": "psql",
        "client_encoding": "UTF8",
    },
}
Send: AuthenticationMD5Password { salt: [1, 2, 3, 4] }
Received: PasswordMessage {
    kind: 'p',
    length: 40,
    password: "md5cb8a43bdf51958828a459f426311fad8",
}
Send: AuthenticationOk
Send: ReadyForQuery
Request processed
```

## Replication connexion

On a first session, start the server:

```bash
cargo run
```
```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.02s
     Running `target/debug/fakepostmaster`
Listening on 192.168.121.1:9092
```

In another session, connected to a real instance:

```sql
postgres=# CREATE SUBSCRIPTION sub CONNECTION 'host=192.168.121.1 port=9092 password=p sslmode=allow' PUBLICATION pub;
```
```text
ERROR:  could not connect to the publisher: could not clear search path: server closed the connection unexpectedly
        This probably means the server terminated abnormally
        before or while processing the request.
```

On our server:

```text
accepted new connection
Received: StartupMessage {
    length: 104,
    protocol_version: (
        3,
        0,
    ),
    parameters: {
        "replication": "database",
        "client_encoding": "UTF8",
        "user": "postgres",
        "database": "postgres",
        "application_name": "sub",
    },
}
Send: AuthenticationMD5Password { salt: [1, 2, 3, 4] }
Received: PasswordMessage {
    kind: 'p',
    length: 40,
    password: "md59e8ff8a1be9a30f9d95e66cab2eeacbd",
}
Send: AuthenticationOk
Send: ReadyForQuery
Received: Query {
    kind: 'Q',
    length: 60,
    query: "SELECT pg_catalog.set_config('search_path', '', false);",
}
Request processed
```

# Memo: tcpdump ftw

````bash
sudo tcpdump -i any port 9092 -X
```
```text
...
21:13:08.201980 vnet4 P   IP pgsrv.53744 > dalibl.9092: Flags [P.], seq 146:207, ack 30, win 502, options [nop,nop,TS val 3422236578 ecr 3908890869], length 61
        0x0000:  4500 0071 aa0e 4000 4006 1ccd c0a8 7959  E..q..@.@.....yY
        0x0010:  c0a8 7901 d1f0 2384 6255 0fad c5ea 4ea9  ..y...#.bU....N.
        0x0020:  8018 01f6 740f 0000 0101 080a cbfb 2fa2  ....t........./.
        0x0030:  e8fc f0f5 5100 0000 3c53 454c 4543 5420  ....Q...<SELECT.
        0x0040:  7067 5f63 6174 616c 6f67 2e73 6574 5f63  pg_catalog.set_c
        0x0050:  6f6e 6669 6728 2773 6561 7263 685f 7061  onfig('search_pa
        0x0060:  7468 272c 2027 272c 2066 616c 7365 293b  th',.'',.false);
        0x0070:  00                                       .
...
```


