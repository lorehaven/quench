# quench-fullstack-macros
The `#[server]` attribute for `quench-fullstack`.

## What it is
`quench-fullstack-macros` is a proc-macro crate with one attribute, `#[server]`. Depend on `quench-fullstack` instead of this crate: it re-exports the attribute as `quench_fullstack::server`, and the expansion refers to `::quench_fullstack` by path.

The annotated `async fn` stays an ordinary function to its callers. On the server it is registered as a route; in the browser (`wasm32`) it becomes a stub that makes the call. The runtime half (registry, dispatch, client call) lives in `quench-fullstack`.

## Use cases
- Declare a public server function reachable at `POST /_fn/<name>`.
- Require a session with `auth`, or make it optional with `auth = "optional"`.
- Pick a custom route with `path = "..."`.
- Share one function definition between server and browser crates.

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

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-fullstack-macros/
