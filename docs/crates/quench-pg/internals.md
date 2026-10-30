---
title: Internals
parent: quench-pg
grand_parent: Crates
nav_order: 9
---

Design notes that matter to users.

## Protocol use

* Startup: `StartupMessage` with `user`, `database`, `client_encoding=UTF8`, `TimeZone=UTC`, `DateStyle=ISO, MDY` (+ `application_name`, `options`). Authentication: cleartext, or SASL `SCRAM-SHA-256` without channel binding. MD5 and everything else is refused.
* Simple queries: one `Query` message; the response (`RowDescription`, `DataRow`*, `CommandComplete` per statement, `ReadyForQuery`) is read to the end even after an error, and the first error is returned.
* Extended queries: always the unnamed statement and portal. Messages sent in one write: `Parse`, `Bind`, `Describe(portal)`, `Execute(0)`, `Sync`. No statement cache: every call re-parses on the server.
* Text format both ways. Parameters have type OID 0 (server-inferred); results are interpreted by OID on the client.
* Not implemented: binary format, `COPY`, `LISTEN`/`NOTIFY` delivery (notifications are logged and ignored), cursors/portal suspension (`Execute` always asks for all rows), query cancellation requests (`BackendKeyData` is stored and exposed as `process_id` only), GSS/SSPI/Kerberos/MD5 authentication, replication, multiple hosts.

## Cancellation safety

A connection tracks whether it is mid-statement (`busy`). If a query future is dropped (for example by `tokio::time::timeout` or `select!`), the connection stays `busy`, `is_closed()` is true, further calls return `Error::Closed` and the pool discards it, rather than misreading the next reply as the abandoned statement's. The same applies after a client-side `statement_timeout`.

## Transactions

`begin` sends `BEGIN` and returns a guard borrowing the connection mutably. Dropping the guard sets `needs_rollback`; the next call on that connection sends `ROLLBACK` first. The pool does not reuse a connection whose transaction status is not idle.

## Pool

* A `Semaphore` with `max_size` permits bounds concurrent checkouts; an idle `VecDeque` holds ready connections (FIFO: oldest idle first).
* On checkout: stale or dead idle connections are dropped; the liveness test is a non-blocking read (`looks_dead`: EOF or stray bytes mean dead) plus `SELECT 1` if idle longer than `test_after_idle`.
* On return: no I/O; unhealthy, in-transaction or over-age connections are dropped.
* Dropped connections are not sent `Terminate`; the socket simply closes.
* The pool has no background tasks and no reaper: idle connections that are never checked out again stay open until the pool is dropped or `close()` is called.

## Types

* Timestamps: with `TimeZone=UTC` the server prints `timestamptz` with a `+00` offset. `parse_timestamp` also handles other offsets. `timestamp` (no zone) text is taken as UTC. Output of `timestamptz` and `timestamp` columns as `Value` is an RFC 3339 UTC string, which `quench_core::time::Time` deserialises.
* `date`, `time`, `interval`, `uuid`, `text` stay `String` in `Value` form; `Uuid` and `Time` have `FromSql`.
* Numeric: `numeric` becomes an integer when it fits `i64`, otherwise an `f64` (precision may be lost), otherwise stays a string.
* `bytea`: only the hex output format is parsed; the session does not set `bytea_output`, so servers with the non-default `escape` format would yield `String`.

## Limits and safety

* `max_message_size` (default 256 MiB) bounds server messages; larger or shorter-than-4 length fields close the connection.
* SCRAM iteration counts above 5,000,000 are rejected. The server's nonce must extend the client's, and the server signature is verified in constant time.
* TLS: `Require` and `Prefer` do not verify certificates; `VerifyFull` verifies the chain and the host name (`quench_tls::server_name(host)`; a bracketed IPv6 host is un-bracketed).
* Debug output: `Config` prints the password; `Connection`, `Pool` do not print credentials.
* No `unsafe`.
