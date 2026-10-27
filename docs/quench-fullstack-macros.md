# Quench Fullstack Macros

`quench-fullstack-macros` (crate `quench_fullstack_macros`) provides `#[server]`, re-exported as `quench_fullstack::server`; depend on `quench-fullstack` rather than on this crate. What a server function is, how it travels and what it needs is in [`quench-fullstack`](./quench-fullstack.md).

```rust
#[server]                                   // POST /_fn/list_notes, anyone may call it
async fn list_notes(owner: String) -> Result<Vec<Note>, ServerError> { /* server only */ }

#[server(auth, path = "/api/notes/add")]    // a valid session is required
async fn add_note(text: String) -> Result<Note, ServerError> {
    let me = user()?;
    /* ... */
}
```

| Option | Meaning |
|---|---|
| `path = "/..."` | the route; default `/_fn/<function name>`. It must start with `/` and have no spaces |
| `auth` or `auth = "required"` | the call is answered 401 before the function runs unless there is a session; `user()` returns the user |
| `auth = "optional"` | the user is looked up and `user()` returns them if there is one |

## What it expands to

- **On the server** (every target but `wasm32`): the function, unchanged; a private argument struct (`#[derive(Deserialize)]`); and an endpoint that checks the call (`server::guard`), finds the user if asked to, reads the arguments, runs the function (in `server::with_user` when it has `auth`) and sends the result or the error. The endpoint is registered in `server::FUNCTIONS` before `main`.
- **In the browser** (`wasm32`): the same signature with a body that serialises the arguments (`#[derive(Serialize)]`) and awaits `client::call::<_, T>(path, &args)`.

The function must be `async`, not generic, take no `self`, name each parameter with a plain identifier, and return `Result<T, ServerError>`; anything else is a compile error that points at it. The shared crate needs `quench-serde` and `quench-fullstack` as dependencies, since the expansion names them by path. The `unused_async` (server) and `future_not_send` (browser) lints are allowed on the generated items.

## Testing

Six unit tests on the expansion (what each target gets, what `auth` adds, the options, and the message for each mistake: not `async`, generic, `self`, a pattern parameter, a return type that is not `Result`, a bad path, an unknown option, a bad `auth` value). The behaviour is tested in `quench-fullstack` with real functions, and in a browser through the notes example.

[Home](../README.md)
