---
title: Usage
parent: quench-fullstack-macros
grand_parent: Crates
nav_order: 1
---

## Public function, default path

```rust
#[server]
pub async fn list_notes(owner: String) -> Result<Vec<Note>, ServerError> {
    Ok(store().notes_of(&owner))
}
```

Route: `POST /_fn/list_notes`. No user lookup; `user()` returns `Err(Unauthorized)` inside it.

## Session required

```rust
#[server(auth)]
pub async fn my_notes() -> Result<Vec<Note>, ServerError> {
    let me = user()?;
    Ok(store().notes_of(&me.sub))
}
```

`auth` is the same as `auth = "required"`. The user is looked up before the arguments are read; without one the response is the error of the failed lookup (401 `sign in required`) and the function does not run.

## Optional session

```rust
#[server(auth = "optional")]
pub async fn greeting() -> Result<String, ServerError> {
    Ok(match user() { Ok(u) => format!("hello {}", u.sub), Err(_) => "hello".into() })
}
```

## Custom path

```rust
#[server(auth, path = "/api/notes/add")]
pub async fn add_note(text: String) -> Result<Note, ServerError> { /* ... */ }
```

The path must start with `/` and contain no whitespace. Two functions with one path make `UiApp::new` panic at startup.

## Parameters

Every parameter becomes a field of the JSON argument object by name. Types must implement `quench_serde::Deserialize` (server) and `Serialize` (browser). The return value `T` of `Result<T, ServerError>` must be `Serialize` (server) and `Deserialize` (browser).

```rust
#[server]
pub async fn rename(id: u64, to: String, tags: Vec<String>) -> Result<(), ServerError> { Ok(()) }
// request body: {"id":1,"to":"x","tags":["a"]}
```

## Doc comments and attributes

Attributes on the function are kept on both the server function and the browser stub.
