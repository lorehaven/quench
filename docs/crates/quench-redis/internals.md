---
title: Internals
parent: quench-redis
grand_parent: Crates
nav_order: 7
---

Design notes that matter to users.

## One connection

A `Connection` owns a boxed stream, a growable read buffer and two flags: `busy` (a command was written and its reply is not fully read) and `closed`. `exchange` sets `busy`, writes and flushes the request, reads exactly the expected number of replies, then clears `busy` and stamps `last_used`. A write failure, read failure, EOF or protocol error sets `closed`. A command timeout (`Error::Timeout`) sets `closed`; a cancelled future leaves `busy` set. `is_closed()` is `closed || busy`, and a closed or busy connection refuses new commands with `Error::Closed`. The effect: a connection is never reused while replies could still be in flight, so a reply can never be attributed to the wrong command.

The read loop calls `resp::decode` on the buffered bytes, and reads more (16 KiB reserved per read) when it returns `None`. Consumed bytes are skipped by position and the buffer is cleared when fully consumed, or compacted once more than 64 KiB are consumed.

Pushes and attributes are not delivered out of band: a `Push` value would be read as the reply of the next command. Do not enable RESP3 client tracking or Pub/Sub on a pooled connection.

## Pool

`Client` holds an `Arc` with the config, options, a `Mutex<VecDeque<Connection>>` of idle connections and a `Semaphore` with `max_size` permits. `get` acquires a permit (bounded by `acquire_timeout`), then pops idle connections from the **front** until one is not `looks_dead()` and not older than `idle_timeout` (since last use); dead ones are dropped. Returned connections are pushed on the back. If none qualify a new one is opened while holding the permit.

`looks_dead` polls the stream once with a no-op waker: a closed peer (EOF), stray bytes or an error all count as dead; only `Pending` is alive. There is no background reaper or keep-alive; staleness is detected at checkout.

### Retry rule

`Client::exec_value` makes at most two attempts. The first attempt's failure is retried only if the connection was **reused** (taken from idle) and the error was `Io`, `Closed` or `Protocol`. Consequence: when a pooled connection breaks after the server executed the command but before the reply arrived, the command runs again. Use idempotent commands (or accept at-least-once) across server restarts. `Timeout` is never retried. Pipelines are never retried.

### Command timeout

The limit applies to the write plus all replies of one `request_value`/`request`/`pipeline`. On expiry the future is dropped and the connection is closed, so the pool discards it.

## Cluster routing

* `ClusterClient` keeps a `RwLock<SlotMap>` of sorted `(first, last, address)` ranges built from `CLUSTER SLOTS`, using only the first node (the primary) of each entry, and a `HashMap<String, Client>` of lazy per-node pools. The `db` of the template config is overridden with 0 for each node.
* Routing: `key_slot(cmd.key())` where `key()` is argument 1; the owning node is looked up in the map. A keyless command goes to the first address.
* `MOVED slot host:port`: the map entry for `slot` is split and reassigned (a single-slot range), and the command is resent. `ASK`: the command goes to the target after `ASKING`, in one pipelined write; the map does not change. The redirect budget is 5 (6 attempts in total).
* On `Io`/`Closed`/`Timeout` from a node the topology is re-read once per command and the command re-routed. On a `CLUSTERDOWN` reply it is re-read once and the command is retried at the same node as before (no re-route); its refresh error, if any, is ignored.
* Replicas are never read from; `READONLY` is not used.
* Multi-key commands are routed by their first key only; the cluster answers `CROSSSLOT` or `MOVED` itself.
* Pools for nodes that leave the topology are kept (never removed).

## Security properties

* Passwords travel in `AUTH`; without TLS they are sent in plain text. `Config`'s `Debug` output (derived) includes the password, so do not log a `Config`.
* `#insecure` disables certificate verification; the default verifies the chain and the host name.
* Size limits bound memory for hostile servers (see [API: RESP codec](api-resp.md#limits)). A server can still make the client buffer up to 512 MiB for one bulk.
* No `unsafe` code.
