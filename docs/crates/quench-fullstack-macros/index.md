---
title: quench-fullstack-macros
parent: Crates
nav_order: 22
has_children: true
permalink: /crates/quench-fullstack-macros/
---

`quench-fullstack-macros` (library `quench_fullstack_macros`, `proc-macro = true`) provides one attribute macro, `#[server]`. Depend on `quench-fullstack` instead of this crate: it re-exports the attribute as `quench_fullstack::server`, and the expansion refers to `::quench_fullstack` by path.

* Version: `0.1.0` (`quench-fullstack-macros/Cargo.toml`).
* Dependencies: `proc-macro2`, `quote`, `syn` (workspace versions). No features.
* Public items: one, `server`.

## Install

Normally not directly. Through the runtime crate:

```toml
[dependencies]
quench-fullstack = "0.1"
quench-serde = "0.1"
```

## Example

```rust
use quench_fullstack::{server, ServerError, user};

#[server]                                   // POST /_fn/list_notes, anyone may call it
async fn list_notes(owner: String) -> Result<Vec<Note>, ServerError> { /* server only */ }

#[server(auth, path = "/api/notes/add")]    // a valid session is required
async fn add_note(text: String) -> Result<Note, ServerError> {
    let me = user()?;
    /* ... */
}
```

## Pages

* [Usage](usage.md): the options and what each one does.
* [API](api.md): the attribute, its arguments, constraints and compile errors.
* [Expansion](internals.md): the generated code for the server and for `wasm32`.
* [Testing](testing.md).

The runtime half (registry, `guard`, `dispatch`, client call) is documented under [quench-fullstack](../quench-fullstack/index.md).
