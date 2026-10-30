---
title: Testing
parent: quench-pg
grand_parent: Crates
nav_order: 10
---

## How the crate is tested

The manifest sets `autotests = false`, the library target `harness = false` (unit tests use `quench-test`; `src/lib.rs` has `#[cfg(test)] quench_test::main!();`), and declares two integration targets, both `harness = false`:

| Target | File | Needs a server | Covers |
|---|---|---|---|
| lib unit tests | `src/*.rs` (`#[cfg(test)]`) | no | message codec (`message`), SCRAM against the RFC 7677 vector (`scram`), text decoding of every supported type (`types`), URL parsing (`config`), `ServerError` parsing and `Error` helpers (`error`), `Row` accessors (`row`) |
| `fake` | `tests/fake.rs` | no | the client against a scripted in-process server: startup parameters, cleartext/SCRAM/MD5/unsupported authentication, an independent SCRAM server implementation, wrong passwords, malformed messages, hang-ups, extended-protocol results, too many parameters, TLS negotiation with the fixture CA, `prefer` vs `require` against a server without TLS, connect timeout, Unix sockets, `looks_dead` |
| `live` | `tests/live.rs` | yes | against a real PostgreSQL: server details, types round trip, UTC timestamps, numeric types, affected row counts and constraint codes, row-count helpers, multi-statement scripts, transactions, notices, bad credentials/databases, TLS, statement timeouts, cancellation, pool limits, discard of unclean connections, killed idle connections, lazy pools, `max_lifetime`, `idle_timeout` |

`cargo test -p quench-pg` runs everything; every `live` test returns early (printing `skipped: DB_TEST_POSTGRES_URL is not set`) unless `DB_TEST_POSTGRES_URL` is set. Use a throwaway server (the repository's `foreman db` starts one; `scripts/test-env.fish` exports the URL). The live tests create uniquely named tables and use `pg_terminate_backend` on their own connections.

## Testing code built on quench-pg

* `Connection::from_stream(Box<dyn Stream>, Arc<Config>)` + `startup()` let you run the client over `tokio::io::duplex` against a hand-written server, as `tests/fake.rs` does. Build server frames with the same layout as [protocol](api-protocol.md): `tag, u32 length, body`.
* `quench_pg::message::decode` and the builders are public, so a fake server can parse the client's frames.
* `Scram::with_nonce` gives deterministic exchanges.
