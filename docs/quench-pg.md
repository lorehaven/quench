# Quench PG

`quench-pg` (crate `quench_pg`) is quench's PostgreSQL client. It replaces `sqlx`: the v3 wire protocol written against `tokio`, SCRAM-SHA-256 authentication, TLS through [Quench TLS](./quench-tls.md), parameterised queries with typed rows, transactions and a connection pool. Rows are read through [Quench Serde](./quench-serde.md), which is how [Quench DB](./quench-db.md) maps models without a `FromRow` derive.

```rust
let pool = quench_pg::Pool::connect("postgres://app:secret@localhost/app").await?;
let rows = pool.query("SELECT id, name FROM users WHERE name = $1", &[&"ann"]).await?;
for row in rows {
    let (id, name): (i32, String) = (row.get("id")?, row.get("name")?);
}
```

## Features

`tls` (default) enables `sslmode=require` and `verify-full` through [`quench-tls`](./quench-tls.md). Without it nothing else changes, except the SSL modes: `require` and `verify-full` fail with `Error::Tls` ("built without its `tls` feature") before anything is sent, `disable` is unchanged and `prefer` (the default) stays on a plain connection instead of sending an `SSLRequest`. Use a build with `tls` for any connection that crosses a network you do not control.

## Connecting

`Config::from_url("postgres://user:password@host:port/database?sslmode=require&application_name=svc")` — user, password and database are percent-decoded; `[::1]` for IPv6; a host starting with `/` is a Unix socket directory. Query parameters: `sslmode`, `sslrootcert` (a PEM file), `application_name`, `options`, `connect_timeout` (seconds), `statement_timeout` (milliseconds, client side).

| `sslmode` | Meaning |
|---|---|
| `disable` | plain text |
| `prefer` (default; `allow` is the same) | try TLS, fall back to plain text if the server declines; the certificate is not verified |
| `require` | TLS or fail; the certificate is not verified |
| `verify-full` (`verify-ca` too) | TLS or fail, certificate chain and server name verified against the public roots, the system bundle and `sslrootcert` |

Authentication is SCRAM-SHA-256 (what PostgreSQL 14+ uses) or cleartext. MD5 is refused with a clear message; channel binding (`-PLUS`) is not used. The session is set to UTC, ISO dates and UTF-8 so results convert predictably.

## Queries

- `Connection::query(sql, &[&dyn ToSql]) -> Vec<Row>`, `query_one`, `query_opt`, `execute -> u64` (rows affected): the extended protocol, parameters `$1`, `$2`, ... typed by the server from context.
- `Connection::simple_query(sql)` / `batch_execute(sql)`: the simple protocol, **no parameters, any number of `;`-separated statements**. This is what migration scripts need: a prepared statement refuses `CREATE TABLE ...; INSERT ...`.
- `begin()` returns a `Transaction` with the same methods plus `commit()` / `rollback()`. Dropping one unfinished leaves a flag that makes the connection roll back before its next statement; the pool drops a connection returned inside a transaction.
- `ping()`, `transaction_status()`, `process_id()`, `parameter("server_version")`.

An error from the server is `Error::Server` with the SQLSTATE (`error.code()`, `is_unique_violation()`), message, detail, hint, position, schema, table, column and constraint. The connection stays usable afterwards. `NOTICE`s are logged at debug level; `LISTEN`/`NOTIFY` traffic is ignored.

A statement whose future is dropped or timed out leaves the conversation half-finished, so the connection reports itself closed (`Error::Closed`) rather than misreading the next reply; the pool discards it.

## Types

Everything travels as text. Parameters implement `ToSql` (`String`/`&str`, integers, floats, `bool`, `Option<T>` for NULL, `Vec<u8>`/`&[u8]` as `bytea`, `quench_core` `Time` and `Uuid`, `quench_serde::Value` for `json`/`jsonb`, `PgArray(vec![..])` for arrays). A column converts by its type OID:

| PostgreSQL | `row.get::<T>` | `row.to_value()` |
|---|---|---|
| `bool`, `int2/4/8`, `oid`, `float4/8`, `numeric` | `bool`, `i16/i32/i64/u32/u64`, `f32/f64` | `Bool`, integers, `Float` (`numeric` is an integer or a float) |
| `text`, `varchar`, `uuid`, `date`, anything else | `String`, `Uuid` | `String` |
| `json`, `jsonb` | `Value` | the parsed value |
| `timestamptz`, `timestamp` | `Time` | RFC 3339 UTC string |
| `bytea` | `Vec<u8>` | array of byte values |
| one- and multi-dimensional arrays of the above | via `Value` | `Array` |

`row.get::<T>(index_or_name)` fails with `Error::Decode` on a NULL read as a non-`Option` type or a value that does not fit. `row.deserialize::<T>()` reads the whole row into any `quench_serde::Deserialize` struct (fields are column names), `row.deserialize_tuple::<(A, B)>()` reads it positionally.

## Pool

`Pool::connect(url)` / `connect_with(config, PoolOptions)` / `lazy(..)`. `get()` hands out a `PooledConnection` (it dereferences to `Connection`), returned on drop. Options: `max_size` (5), `acquire_timeout` (30 s), `idle_timeout` (10 min), `max_lifetime` (30 min), `test_after_idle` (a connection idle longer than this is checked with `SELECT 1` before use). Dead idle connections (the server restarted, a firewall dropped them) are noticed and replaced. `pool.query(..)`, `execute(..)`, ... borrow a connection for one call.

## Testing

Unit tests cover the message codec, SCRAM against the RFC 7677 vector, text decoding of every supported type, URL parsing and the error type. `tests/fake.rs` runs the client against a scripted server (authentication variants including an independent SCRAM server implementation, TLS negotiation with the fixture CA, malformed messages, hang-ups) and `tests/live.rs` runs it against a real server when `DB_TEST_POSTGRES_URL` is set (`scripts/test-env.fish` after `foreman db`): types round trip, constraint errors, transactions, multi-statement scripts, statement timeouts, cancellation, pool limits and failover.

[Home](../README.md)
