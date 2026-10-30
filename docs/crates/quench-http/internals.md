---
title: Internals
parent: quench-http
grand_parent: Crates
nav_order: 9
---

## Link-time discovery

`#[get]` & co. expand to a `::quench_http::submit!(ROUTES, RouteRegistration, ...)` item; `#[injectable]` to `submit!(COMPONENTS, ComponentFactory, ...)`. `submit!` and `Registry` are in `quench-core::registry`: statics collected by the linker, read at runtime by iteration. Consequences:

* **Order** is link order: source order within one crate; between crates it is not under your control. When two patterns can match the same path, the first one in this order wins.
* **Library crates**: an `.rlib` member that nothing in the final binary references is not linked, and its registrations vanish with no error. A binary that only talks to `quench-http` and expects routes from another crate must reference something real from that crate (`quench-starter` exposes `force_link()` functions for this). See the doc comment on `RouteRegistration` (`src/route.rs`).
* The registries are process-global: every route and component in the binary, including those of test targets, is in `ROUTES`/`COMPONENTS`.

## Route patterns

`pattern.rs` (private). Grammar: literal text, `{name}`, `{name:.+}`, `{name:.*}`.

| Form | Matches |
|---|---|
| literal | exactly that text; matching is anchored at both ends; trailing slash significant (`/ui` vs `/ui/`); `""` matches only the empty path |
| `{name}` | one **non-empty** run of characters without `/`; may share a segment with literal text (`/f/{name}.json`) |
| `{name:.+}` | one or more characters, `/` included |
| `{name:.*}` | zero or more characters, `/` included |

Matching is greedy with backtracking (longest capture first, shrinking until the rest matches), at char boundaries. Anything else after `:` panics at `discover_routes()`/`RouterBuilder::finish` with ``route pattern `P`: unsupported constraint `X` (only `.+` and `.*`)``; an unclosed `{` panics with ``route pattern `P`: unclosed `{` ``. Captures are matched against the raw (still percent-encoded) path; `Path<T>` decodes them afterwards (so `%2f` never splits a `{name}` segment). Dot-segments (`..`) are not normalised; wildcards capture them verbatim (`security_http` test).

## Router

`Router` holds `Vec<(Pattern, RouteEntry)>`; dispatch is a linear scan, the first matching pattern wins, then a linear scan of its `(Method, Endpoint)` list. Cost is O(number of routes).

## DI resolution

`ContainerBuilder::build`:

1. Collect all factories from `COMPONENTS`; index by `type_id`.
2. For each factory, for each dependency: skip it if the container already holds it (provided); fail with `MissingDependency` if no factory produces it; otherwise count one in-degree and record the dependent.
3. Kahn's algorithm: start with factories of in-degree 0 (HashMap iteration order, i.e. unspecified), call `construct(&container)`, insert the result, decrement dependents.
4. If `constructed != factories.len()` return `Cycle(names of factories whose in-degree is still > 0)`.

Each component is constructed once, sequentially (no parallel construction). Constructors receive a clone of the container and read their `Arc<Dep>` with `container.get::<Dep>()`.

## Server loop

`serve_loop` (private):

* Logs `listening on {http|https}://ADDR`.
* `ConnConfig` = `quench_proto` default with `header_read_timeout = 30 s`.
* Loops `select!` between `listener.accept()` and the shutdown future. Each accepted socket gets `TCP_NODELAY` and its own task; for TLS the handshake must finish within `header_read_timeout` (failure and timeout only log at `debug`).
* On shutdown: stop accepting, signal all connections via `shutdown_channel` (the protocol driver finishes the in-flight request and closes), then wait until every connection task has dropped its sender, at most `DEFAULT_GRACEFUL_TIMEOUT` (25 s), then return `Ok(())`. The listener is dropped on return, so new connections are refused afterwards.
* Protocol: HTTP/1.1 only (keep-alive, pipelining, chunked bodies, `Expect: 100-continue`, upgrade) from `quench-proto`; the request is converted into a `Request` with the shared container.

## Path parameter deserialisation

`path_de::from_params` (see [API: extractors](api-extractors.md#extract)): values are percent-decoded exactly once; invalid UTF-8 after decoding is an error; no `+`-to-space conversion. Requires the `quench_serde::private::Lenient` mode so strings parse as numbers/booleans.

## Security-relevant properties (tested in `tests/security_http.rs`)

* A `{name}` never spans `/`; encoded separators arrive decoded exactly once.
* Hostile percent encodings do not cause `5xx`.
* `X-HTTP-Method-Override` style headers and unknown methods are not honoured.
* An over-size JSON body is refused without being fully buffered (`collect_limited`).
* CR/LF in user input cannot inject response headers (header values are validated by `HeaderValue::from_str`).
* A panicking handler does not stop the server.
* Pipelined requests get one response each, in order.
