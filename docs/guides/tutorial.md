---
title: "Tutorial: your first app"
parent: Guides
nav_order: 15
---

This tutorial builds a small full-stack app from nothing: a shared pantry list. You write the data type, the server functions that change it, the screen, and the server that renders and serves it, and you test each part. It takes about an hour and ends with a page that is rendered on the server and taken over by WebAssembly in the browser.

The finished code is in the workspace as `examples/tutorial_app` (the part that runs in both places) and `examples/tutorial_server` (the part that serves it). Every listing below is from those files, which build and pass their tests. If a step does not work for you, diff against them.

## What you need

* The Rust toolchain from [Getting started](getting-started.md) and the `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`.
* Nothing else. No Node, no bundler, no database: the list lives in memory so that the tutorial stays about the framework. [Data access](data-access.md) shows the database step.

## The plan

One crate is built twice.

```text
tutorial_app      lib (cdylib + rlib)   the Item type, the #[server] functions, the screen
                  built for the server  -> renders the page, answers the function calls
                  built for the browser -> takes the page over, calls the same functions over HTTP
tutorial_server   bin                   serves the page and the files with quench-http
```

The idea that makes this work: a function marked `#[server]` is an ordinary `async fn` to the code that calls it. In the browser the call is a `POST`; on the server it is a function call. The screen does not know which.

## 1. The shared crate

`examples/tutorial_app/Cargo.toml`:

```toml
[lib]
name = "quench_example_tutorial_app"
path = "src/lib.rs"
crate-type = ["cdylib", "rlib"]     # cdylib: the .wasm module; rlib: the server and the tests link it

[dependencies]
quench-fullstack = "0.1"            # #[server], the page and the client
quench-serde = "0.1"                # the #[server] expansion names it by path
quench-ui = "0.1"                   # signals, views, forms
quench-ui-web = "0.1"               # spawn, the browser entry point

[dev-dependencies]
quench-test = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

Inside the workspace the versions are `{ workspace = true }`.

## 2. The data

`src/lib.rs` starts with the type that crosses the wire. `Serialize` and `Deserialize` come from `quench-serde`, not from a third-party crate:

```rust
use quench_serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Item {
    pub id: u32,
    pub name: String,
    pub bought: bool,
}
```

The list itself exists only on the server, so it is compiled out for the browser:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod store {
    use super::Item;
    use std::sync::Mutex;

    static ITEMS: Mutex<Vec<Item>> = Mutex::new(Vec::new());

    pub fn add(name: &str) -> Item { /* next id, push, return a copy */ }
    pub fn set_bought(id: u32, bought: bool) -> Option<Item> { /* find, change, return a copy */ }
    pub fn all() -> Vec<Item> { /* a copy of the list */ }
}
```

The bodies are in the example. Nothing here knows about HTTP.

## 3. Server functions

```rust
use quench_fullstack::{ServerError, server};

#[server]
pub async fn list_items() -> Result<Vec<Item>, ServerError> {
    Ok(store::all())
}

#[server]
pub async fn add_item(name: String) -> Result<Item, ServerError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerError::invalid("name", "write something"));
    }
    if name.chars().count() > MAX_LEN {
        return Err(ServerError::invalid("name", "too long"));
    }
    Ok(store::add(name))
}

#[server]
pub async fn set_bought(id: u32, bought: bool) -> Result<Item, ServerError> {
    store::set_bought(id, bought).ok_or_else(|| ServerError::not_found("no such item"))
}
```

Three things to notice:

* The body is only ever compiled into the server build. In the browser build the same signature becomes a client stub that `POST`s to `/_fn/<name>`.
* `ServerError::invalid("name", ...)` is a validation error that names a field. It travels to the browser as a `422` and lands on the form field of that name (step 4).
* These three are public. A function marked `#[server(auth)]` needs a signed-in user and gets it from `quench_fullstack::user()?`; see [Authentication](authentication.md) and `examples/ui_web_app/src/notes.rs`.

### Test them

Because they are plain `async fn`s, tests call them directly. `tests/unit.rs`:

```rust
use quench_example_tutorial_app::{Item, add_item, list_items, set_bought, store};

#[quench_test::test]
async fn server_functions_are_plain_async_functions() {
    store::clear();
    let milk = add_item("  milk ".into()).await.unwrap();
    assert_eq!(milk, Item { id: 1, name: "milk".into(), bought: false });
    assert!(set_bought(1, true).await.unwrap().bought);
    assert_eq!(list_items().await.unwrap().len(), 1);
}

quench_test::main!();
```

`quench_test::main!()` must end the file, and the test target needs `harness = false` in `Cargo.toml` (`[[test]] name = "unit"`); without them the tests silently do not run. See [Testing](testing.md). The store is one list per process and tests run in parallel, so the example keeps every test that touches it in one function.

```bash
cargo test -p quench-example-tutorial-app
```

## 4. The screen

A screen is a function that returns a `View`. State lives in signals; a closure that reads a signal is re-run when it changes, and only the text or attribute that read it is patched.

```rust
use quench_ui::form::{Field, Form, rules};
use quench_ui::prelude::*;

pub fn pantry_view(initial: Vec<Item>) -> View {
    let items = create_signal(initial);
    let message = create_signal(String::new());

    let name = Field::new("name", "").rule(rules::required("write something"));
    let form = Form::new().field(&name);
    // ... the form, the message and the list (below)
}
```

The form checks what it can in the browser (`required`) and shows the server's verdict on the same field when the server refuses something else. `FormExt::finish` does that hand-over:

```rust
let add_form = form
    .element({
        let (items, name) = (items.clone(), name.clone());
        move |form| {
            form.set_submitting(true);
            let (form, items, name) = (form.clone(), items.clone(), name.clone());
            let value = name.value_untracked();
            spawn(async move {
                if let Some(item) = form.finish(add_item(value).await) {
                    items.update(|list| list.push(item));
                    name.reset("");
                }
            });
        }
    })
    .child(name.input().attr("placeholder", "milk, bread, ..."))
    .child(name.error_view())
    .child(form.error_view())
    .child(button().attr("type", "submit").text("Add"));
```

`add_item(value).await` is the server function. On `Ok` you get the new `Item`; on a validation error `finish` puts the message under the `name` field and returns `None`.

The list is a keyed list. Each row is rebuilt only when its key changes, so the key includes everything that the row shows:

```rust
ul().attr("id", "items").child(View::keyed(
    move || list.get(),
    |i| format!("{}:{}", i.id, i.bought),
    move |i| row(i, &row_items, &row_message),
))
```

`row` is a `li` with the name and a button whose click calls `set_bought(id, !bought)` and replaces the item in the signal. Read it in the example; it is the same pattern as the form.

### Test the screen without a browser

`TestRenderer` mounts a view into an in-memory tree and delivers events synchronously:

```rust
use std::rc::Rc;
use quench_ui::prelude::*;

#[quench_test::test]
fn an_empty_name_is_refused_before_the_server_is_asked() {
    let renderer = Rc::new(TestRenderer::new());
    let root = renderer.root();
    let (mounted, scope) = create_root(|| mount(&renderer, &root, pantry_view(Vec::new())));
    let form = renderer.find_by_tag("form")[0];
    renderer.fire(form, &Event::Submit);
    assert!(renderer.text().contains("write something"));
    assert!(renderer.find_by_tag("li").is_empty());
    mounted.unmount();
    scope.dispose();
}
```

Create signals and forms inside `create_root`: they need a reactive scope. More in [Building a UI](building-a-ui.md).

## 5. The browser entry

At the end of `lib.rs`, only for the browser build:

```rust
#[cfg(target_arch = "wasm32")]
fn app() -> View {
    let initial = quench_ui_web::initial_state()
        .and_then(|json| quench_serde::json::from_str::<Vec<Item>>(&json).ok())
        .unwrap_or_default();
    pantry_view(initial)
}

#[cfg(target_arch = "wasm32")]
quench_ui_web::app!(app);
```

`initial_state()` is the JSON the server put into the page (step 6). Starting from it, the browser builds exactly the tree the server rendered and takes the existing nodes over instead of redrawing them. Build the module:

```bash
cargo build --target wasm32-unknown-unknown --profile wasm -p quench-example-tutorial-app
```

The result is `target/wasm32-unknown-unknown/wasm/quench_example_tutorial_app.wasm`.

## 6. The server

`examples/tutorial_server/src/main.rs`:

```rust
use std::sync::Arc;

use quench_example_tutorial_app::{pantry_view, store};
use quench_fullstack::{Page, Ui, UiApp};
use quench_http::prelude::*;

#[tokio::main]
async fn main() {
    let wasm = std::env::var("WASM").unwrap_or_else(|_| {
        "target/wasm32-unknown-unknown/wasm/quench_example_tutorial_app.wasm".to_string()
    });
    let bytes = std::fs::read(&wasm)
        .unwrap_or_else(|e| panic!("cannot read {wasm}: {e} - build the app first"));

    let mut ui = Ui::new(bytes);
    let css = ui.add_asset("site.css", STYLE, "text/css");   // -> /_quench/site.<hash>.css
    let app = UiApp::new(ui).page("/", move |_req| {
        let items = store::all();
        Page::new("Pantry", pantry_view(items.clone()))       // rendered on the server
            .stylesheet(&css)
            .state(quench_serde::json::to_string(&items))     // what initial_state() reads
    });

    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let addr = "127.0.0.1:3000".parse().unwrap();
    serve_http(Arc::new(app), container, addr).await.unwrap();
}
```

`UiApp` is an `Endpoint` of `quench-http`. It serves the page, the module and `quench.js` (the small loader from `quench-ui-web`) under content-hashed names with a strict Content-Security-Policy, and it answers the `#[server]` functions that are linked into the binary. That last part is why the server imports from the app crate: if nothing from it is used, the linker may drop the function registrations.

Run it:

```bash
cargo run -p quench-example-tutorial-server
```

Open `http://127.0.0.1:3000`. Add milk; reload: the list is rendered by the server before any script runs (view the source). `PORT` and `WASM` change the port and the module path.

The same thing from a terminal, which shows what the browser sends. The `x-quench-rpc` header is required: a call without it is refused with `403`.

```bash
curl -s -X POST localhost:3000/_fn/add_item -H 'content-type: application/json' -H 'x-quench-rpc: 1' -d '{"name":"milk"}'
# {"id":1,"name":"milk","bought":false}
curl -s -X POST localhost:3000/_fn/add_item -H 'content-type: application/json' -H 'x-quench-rpc: 1' -d '{"name":""}'
# {"kind":"validation","message":"name: write something","fields":[{"field":"name","message":"write something"}]}   (status 422)
```

*Not verified here:* the steps above were run as shown with `curl`, and the tests pass, but the hydration in an actual browser was not driven for this tutorial. The same mechanism is exercised by the browser tests of `examples/ui_web_app` (`scripts/ui-web-test.fish`).

## 7. Where to go from here

You now have the shape of every Forge service: a shared crate with the model and the screens, a server that renders and serves it. The next steps, each with its own page:

| To add | Read |
|---|---|
| A database instead of memory | [Data access](data-access.md), [quench-db](../crates/quench-db/index.md) |
| Sign-in, so that each person has their own list (`#[server(auth)]`) | [Authentication](authentication.md), [quench-fullstack](../crates/quench-fullstack/usage.md) |
| Several pages and links | the router in [Building a UI](building-a-ui.md), `Page::routed` and `pages_under` |
| Translations | `I18n` in [Building a UI](building-a-ui.md) |
| Plain JSON routes next to the screens (for other clients) | [Writing an HTTP service](writing-an-http-service.md) |
| Health checks, `BASE_PATH`, TLS, logging: the boilerplate every service repeats | [quench-starter](../crates/quench-starter/index.md) |
| A command-line tool instead of a page | [Building CLI tools](building-cli-tools.md) |
| Packaging and releasing | [Packaging](packaging.md) |
| How a real service is laid out (core, app, web, service) | the Forge book: the browser-client guide |
