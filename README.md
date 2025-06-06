# Info

## What?

Implement the PostgreSQL wire protocol as defined here:

* https://www.postgresql.org/docs/17/protocol-flow.html#PROTOCOL-FLOW-START-UP
* https://www.postgresql.org/docs/17/protocol-message-types.html
* https://www.postgresql.org/docs/17/protocol-message-formats.html
* https://www.postgresql.org/docs/17/protocol-replication.html
* https://www.postgresql.org/docs/17/protocol-logical-replication.html

Note: the authentication is done:

* in C, here: `PQconnectPoll()` (`src/interfaces/libpq/fe-connect.c`)
* in Rust, here: [`pg_cat`](https://github.com/postgresml/pgcat/blob/main/src/client.rs#L324)

## Why?

Implementation of PostgreSQL's wire protocol usually cover the frontend side of
things and program that need to implement the backend side of things do it in
the code. That's why the messages are reimplemented here.

It's an experiment to see, what we can do:

* buffering logical replication
* anonymize on the fly
* track the activity submitted to the instance

.. and to see how the protocol works.

## So?

It's still very alpha code, not stable or full featured: a POC.

The main crate is a library, there is a derive macro crate (`libpq-serde-macros`) and
a utility/test crate for encoding decoding (`libpq-serde-types`). There are examples
in `$CRATE_ROOT/examples` namely `client` and `passthru`.

Known limitation:

* the cli is rough (two examples: client and passthru
* TLS connexion are not supported (use ssl_mode=allow)
* there is no async implementation

# Examples

## client

The purpose is to try to executes queries or consume modification from a slot.

```bash
cargo run --example client query &> /tmp/log
```

This will a series of queries / commands. There is a lot of messages, so it's
recommended to redirect to a log file and whatch what happended afterwards.

```bash
cargo run --example client replication &> /tmp/log
```

Thiw will consume the data from a slot, the configuration is directly in the
code.

## passthru

The purpose is to have a proxy that can forward the data from a client
connexion to a PostgreSQL instance for queries or logical replication.

Prerequisite: 

* a target instance `pgsrv:5432` in the code
* an interface/port to listen on `192.168.121.1:9092`
* the user must exist in the database and have the md5 authentication method
configured in the `pg_hba.conf` and the password encryption.
* the connexion must not use TLS

Example: client

```bash
cargo run --example passthru
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

Example: replication

We will need two instances for this.

```bash
cargo run --example passthru
```

In one session connected to thepublication, create a publication:

```sql
-- cleanup the slot
-- /!\ it will kill all the slot (if possible)
SELECT slot_name, pg_drop_replication_slot(slot_name) FROM pg_replication_slots; 

-- create a publication
CREATE TABLE t(i int);
CREATE PUBLICATION pub FOR TABLE t;
```

In one session connected to the subscription instance, create the subscription:

```sql
-- cleanup
ALTER SUBSCRIPTION subtest DISABLE; 
ALTER SUBSCRIPTION subtest SET (slot_name = NONE); 
DROP SUBSCRIPTION subtest ; 

-- create a subscription
CREATE SUBSCRIPTION subtest 
  CONNECTION 'host=192.168.121.1 port=9092 user=md5userrl dbname=postgres sslmode=allow password=md5passrl' 
  PUBLICATION pub;
```

INSERT UPDATE DELETE TRUNCATE on the table will be transferred.

If you run the foillowing 


```bash
cargo run --example passthru -- --anonymize
```

and update the oid on this line of `src/handler/client.rs`:


```rust
anonymize_what.insert((16453, 0), Anonymize::i32(|c: i32| c * -10)); // t.i
```

Note:

* the first tuple `(16453, 0)` is `(relation_oid, column position)`
* `Anonymize::i32(fn(i32) -> i32)`: identifies the type of the column as i32 (int) and
  defines the prototype of the anonymization function
* `|c: i32| c * -10)`: is a lambda function that takes an i32 and returns an i32. It could
  be any valid rust code including other function calls. I suppose we could use a scripting
  language (lua) to make this dynamic.

The target is to have a more user friendly struct and build the Hashmap (`anonymize_what`)
as we receive the `CopyData>XlogData>Relation` messages so that it's more usable.
