# quench-fullstack
Serves `quench-ui` apps from `quench-http` and lets them call server functions: server-side rendering, hashed assets, typed calls, sessions.

## What it is
`quench-fullstack` serves `quench-ui` apps from `quench-http` and lets them call server code. The page is rendered on the server from the same components the browser runs, the browser hydrates it (`quench-ui-web`), and everything goes out with content-hashed asset names and a strict Content-Security-Policy.

A function marked `#[server]` (from `quench-fullstack-macros`, re-exported here) is an ordinary `async fn` to the code that calls it, in the browser or on the server. Sessions work by cookie in the browser and by bearer token in a native shell. The server half is compiled only for non-`wasm32` targets.

## Use cases
- Render a `quench-ui` page on the server and hydrate it in the browser.
- Expose typed server functions that components call as plain `async fn`s.
- Sign in with cookies in the browser or bearer tokens in a native shell.
- Show different pages depending on the signed-in user.
- Export a static bundle for a native shell, or reload during development.

## Example
Server:

```rust
use quench_fullstack::{Page, Ui, UiApp};

let mut ui = Ui::new(std::fs::read("app.wasm")?);
let css = ui.add_asset("site.css", STYLE, "text/css");
let app = UiApp::new(ui).page("/", move |_req| {
    Page::new("My app", my_app::view(None)).stylesheet(&css)
});
```

Shared crate:

```rust
use quench_fullstack::{server, ServerError, user};

#[server(auth)]
pub async fn add_note(text: String) -> Result<Note, ServerError> {
    if text.trim().is_empty() {
        return Err(ServerError::invalid("text", "write something"));
    }
    Ok(store().add(&user()?.sub, text))
}
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-fullstack/
