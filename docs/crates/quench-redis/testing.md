---
title: Testing
parent: quench-redis
grand_parent: Crates
nav_order: 8
---

The crate uses the `quench-test` harness (`harness = false` in every test target, `quench_test::main!()`), not libtest.

## What exists

| Target | File | Needs a server | Covers |
|---|---|---|---|
| unit tests (lib) | `src/*.rs` (`resp` 8, `cluster` 9, `config` 3, `cmd` 2, `error` 2 tests) | no | codec for every type, partial input, limits, malformed input; URL parsing and bad URLs; command encoding and `FromValue`; CRC16 vectors, hash tags, slot maps, redirect and `CLUSTER NODES` parsing; error text and `source()` |
| `tests/cluster.rs` | scripted nodes in the test process | no (nodes are fake TCP servers) | routing by slot, hash tags, `MOVED`, `ASK`, redirect loops, `CLUSTERDOWN` refresh, pass-through of other errors, dead-node relearning, `exec_on`, `primaries`, unreachable seed |
| `tests/live.rs` | a real Redis | yes: `CACHE_TEST_REDIS_URL` | strings and expiry, binary and megabyte values, sets, lists, `SCAN`, typed server errors and surviving connection, pipelines with errors, RESP3 replies, databases and `AUTH`, dead connection replacement, command timeouts, cancellation, pool limits and reuse, lazy clients, unreachable server |
| `tests/no_tls.rs` | local listener | no | only built without `tls` (`--no-default-features`): a `rediss://` URL fails with `Error::Tls` containing "`tls` feature" |

`tests/live.rs` tests return early (printing `skipped: CACHE_TEST_REDIS_URL is not set`) when the variable is unset or empty. Point it at a throwaway server; the tests use random key prefixes but do write to the database in the URL.

## Testing your own code

`Connection::from_stream(Box<dyn Stream>, &Config)` accepts any `AsyncRead + AsyncWrite + Unpin + Send` stream, for example one half of `tokio::io::duplex`, so a test can play the server by hand with `resp::encode_command`/`resp::decode` and no network. `tests/cluster.rs` shows the pattern with real sockets bound to `127.0.0.1:0`.
