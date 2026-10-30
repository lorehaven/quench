---
title: Usage
parent: quench-fullstack
grand_parent: Crates
nav_order: 1
---

## 1. Render a page on the server and hydrate it

```rust
use quench_fullstack::{Page, Ui, UiApp};

let mut ui = Ui::new(std::fs::read("app.wasm")?);
let css = ui.add_asset("site.css", STYLE, "text/css");
let app = UiApp::new(ui).page("/", move |_req| {
    let state = my_app::State::load();
    Page::new("My app", my_app::view(Some(state.clone())))
        .stylesheet(&css)
        .state(state.to_json())       // embedded; the browser reads it with quench_ui_web::initial_state()
});
```

`UiApp` is a `quench-http` `Endpoint`. A page closure runs inside one synchronous call, because a page holds `Rc`s. Code that only makes sense in the browser (`spawn`, `fetch`) should run only when the state is absent, so the server never reaches it.

## 2. An app that routes in the browser

```rust
# use quench_fullstack::{Page, Ui, UiApp};
let app = UiApp::new(ui).pages_under("/app", |req| {
    let url = req.uri().path_and_query().map_or("/app", |p| p.as_str()).to_string();
    Page::routed("App", &url, |router| my_app::routes_view(router))
});
```

`pages_under("/app", ..)` matches `/app` and `/app/...` (whole segments only; `/application` does not match). `Page::routed` renders the router at the request URL and the response becomes a `302` to where the router ended up (a guard redirect), a `404` if no route matched, or the page.

## 3. Server functions

```rust
use quench_fullstack::{server, ServerError, user};

#[server]                                                   // POST /_fn/list_notes, public
pub async fn list_notes(owner: String) -> Result<Vec<Note>, ServerError> { /* ... */ }

#[server(auth, path = "/api/notes/add")]                   // session required
pub async fn add_note(text: String) -> Result<Note, ServerError> {
    let me = user()?;
    /* ... */
}
```

Call them from anywhere (a component, shared code) as plain `async fn`s:

```rust
spawn(async move {
    match add_note(text).await {
        Ok(note) => notes.update(|list| list.push(note)),
        Err(e) => message.set(e.message().to_string()),
    }
});
```

The binary that serves them must link the crate that defines them (use something from it), otherwise the linker can drop the registrations. `UiApp::new` panics if two functions have the same path.

## 4. Forms and validation errors

```rust
use quench_fullstack::{FormExt, ServerError};

// in the submit handler, with `form: quench_ui::form::Form`
let created = form.finish(create_note(draft).await);   // Option<Note>
```

`finish` turns `submitting` off; on success it clears errors and returns `Some(value)`; on `ServerError::Validation` it puts each `FieldError` on its field; any other error becomes the form's own message.

## 5. Cookie sign-in (browser)

```rust
use quench_fullstack::client;

client::on_unauthorized(|| show_sign_in());
client::sign_in("/api/v1/auth", "alice", "secret").await?;   // POST /api/v1/auth/login, cookies are set by the answer
// ...
client::sign_out("/api/v1/auth").await?;                     // POST /api/v1/auth/logout
```

## 6. Bearer sign-in (native shell)

```rust
use std::rc::Rc;
use quench_fullstack::client::{self, Bearer, MemoryStore};

let bearer = Bearer::new("/api/v1/auth", Rc::new(MemoryStore::default()));
bearer.login("alice", "secret").await?;
client::use_bearer(bearer);
```

Every call then carries `Authorization: Bearer <access>`. On a 401 the tokens are refreshed once and the call retried; concurrent calls that meet the expiry share one refresh. A host implements `TokenStore` over its secure storage instead of `MemoryStore`.

## 7. Pages that depend on the user

```rust
# use quench_fullstack::{Page, UiApp};
let app = app.page_for_user("/", |_req, user| match user {
    Some(u) => Page::new("Notes", signed_in_view(&u)),
    None => Page::new("Sign in", sign_in_view()),
});
```

`user` is `None` when no valid session or token is found.

## 8. Development reload

```rust
let mut ui = Ui::new(wasm);
if std::env::var_os("QUENCH_DEV").is_some() {
    ui = ui.dev_reload();
}
```

`QUENCH_DEV` here is the application's own convention, not read by the crate.

## 9. Static export for a native shell

```rust
ui.export(std::path::Path::new("bundle"), Page::new("My app", view))?;
```

Writes `bundle/index.html` and each asset at `bundle/_quench/<name>.<hash>.<ext>`. Scripts carry the fixed nonce `bundle`.

## 10. Test code that calls server functions

Use `testing::StubGatehouse` as the token service and provide `stub.jwt()` in the container; see [Testing](testing.md).
