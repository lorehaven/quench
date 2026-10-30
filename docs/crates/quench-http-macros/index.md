---
title: quench-http-macros
parent: Crates
nav_order: 25
has_children: true
permalink: /crates/quench-http-macros/
---

`quench-http-macros` is the procedural-macro crate behind `quench-http`'s annotation-driven discovery: five route attributes (`#[get]`, `#[post]`, `#[put]`, `#[delete]`, `#[patch]`) and `#[injectable]`. You do not depend on it directly: `quench_http::prelude` re-exports all six.

* Version: `0.1.3` (`quench-http-macros/Cargo.toml`); `[lib] proc-macro = true`.
* Dependencies: `proc-macro2`, `quote`, `syn` (workspace versions). Nothing else.
* Generated code refers only to `::quench_http::...` paths, so the using crate needs `quench-http` as a direct dependency under that name (a renamed dependency breaks the expansion).

```toml
[dependencies]
quench-http = { registry = "ennor", version = "0.1" }
```

```rust
use quench_http::prelude::*;

#[get("/ping")]
async fn ping() -> &'static str { "pong" }
```

Pages: [API (the six attributes)](api.md), [generated code and internals](internals.md), [compile errors](errors.md), [testing](testing.md). Runtime behaviour of what they register is documented in [quench-http](../quench-http/index.md).

Public items: 6 attribute macros (`get`, `post`, `put`, `delete`, `patch`, `injectable`). No features, no derives.
