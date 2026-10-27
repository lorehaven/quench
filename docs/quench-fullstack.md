# Quench Fullstack

`quench-fullstack` (crate `quench_fullstack`) serves [`quench-ui`](./quench-ui.md) apps from [`quench-http`](./quench-http.md): the page is rendered on the server from the same components the browser runs, the browser hydrates it ([`quench-ui-web`](./quench-ui-web.md)), and everything goes out with content-hashed asset names and a strict Content-Security-Policy. It is where server functions, shared types and authentication will be added.

```rust
use quench_fullstack::{Page, Ui, UiApp};
use quench_http::prelude::*;

let mut ui = Ui::new(std::fs::read("app.wasm")?);          // quench.js is added from quench-ui-web
let css = ui.add_asset("site.css", STYLE, "text/css");      // -> /_quench/site.<hash>.css
let app = UiApp::new(ui).page("/", move |_req| {
    let state = my_app::State::load();
    Page::new("My app", my_app::view(Some(state.clone())))  // rendered on the server
        .stylesheet(&css)
        .state(state.to_json())                              // embedded; the browser does not refetch it
});
serve_http(Arc::new(app), container, addr).await?;
```

The app crate builds twice: for the browser (`wasm32`, with `quench_ui_web::app!`), and as an ordinary library the server calls to render. Code that only makes sense in the browser (`spawn`, `fetch`) runs only when the state is absent, so the server never reaches it. `examples/ui_web_app` and `examples/ui_web_server` are a complete pair.

## A page

`Page::new(title, view)` with `.lang(..)`, `.stylesheet(href)`, `.state(json)` and `.status(..)`. It renders to

```html
<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" ...>
<title>My app</title><link rel="stylesheet" href="/_quench/site.<hash>.css"></head>
<body><div id="app"> ...the view, as HTML... </div>
<script type="application/json" id="__q_state" nonce="N">{...}</script>
<script type="module" nonce="N" src="/_quench/quench.<hash>.js?wasm=/_quench/app.<hash>.wasm"></script>
</body></html>
```

The view is rendered with `quench_ui::render_to_string`: reactive text shows its current value, listeners and effects are dropped, and each dynamic or keyed region leaves an empty comment (`<!---->`) that ends it. The golden test `tests/fixtures/page.html` pins the whole document.

### Embedded state

`.state(json)` puts JSON in a `<script type="application/json">` block, which the browser hands to the app before it starts (`quench_ui_web::initial_state()`), so the first render has the data the server already had. The text is escaped so it cannot end its element: `<`, `>`, `&` and the two Unicode line separators become `\u` escapes, which only occur inside JSON strings, so valid JSON stays valid and parses to the same value (`escape_json`). The JSON is not validated.

## Assets

`Assets` (and `Ui::add_asset`) serve files under `/_quench/` with a name that carries the first 16 hex digits of the SHA-256 of the bytes: `app.wasm` becomes `/_quench/app.3f9a1c0b7d2e4a65.wasm`. Changing the bytes changes the path, so:

- responses carry `cache-control: public, max-age=31536000, immutable` and an `etag`; a matching `If-None-Match` (also `*`, or a weak validator) gets a 304 with no body;
- text, JavaScript, JSON, SVG and WebAssembly of 256 bytes or more are also stored gzipped at startup (`quench-archive`) and sent to clients that accept it (`gzip` with a quality above 0, or `*`), only when that is smaller; `vary: accept-encoding` is always set;
- a path under `/_quench/` that is not an asset (a build from before the last deploy, `..` tricks, a query on the path) is a 404 with `no-store`, never a fallback to a page.

## Security

- **Policy.** Each document gets `default-src 'self'; script-src 'self' 'nonce-N' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'self'; img-src 'self' data:; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'`, with `N` a fresh 128-bit random value per response (`new_nonce`), repeated on both `<script>` elements. `'wasm-unsafe-eval'` is what lets WebAssembly compile; it does not allow `eval`. There is no `'unsafe-inline'`, so an injected inline script is blocked (the browser test injects one and checks it does not run).
- **Documents are never cached** (`no-store`), because they carry a nonce.
- **No inline handlers or styles.** Views cannot produce `on*` attributes (`quench-ui` drops them) and `Page::stylesheet` links a file. `Element::style(..)` writes a `style` attribute, which this policy blocks: use classes and a stylesheet.
- **Escaping** is done where the markup is made: text, attribute values, the title and language, and the state block each have tests with hostile input (`</script>`, `"` in attributes, U+2028).
- Also sent: `x-content-type-options: nosniff`, `referrer-policy: same-origin`.

## `UiApp`

An `Endpoint` that answers `/_quench/..` from the assets and exact paths from the pages registered with `.page(path, |request| Page)`; anything else is a 404. A page holds `Rc`s, so it is built and rendered inside one synchronous call. Pattern routes, a router and server functions come with later stages.

## Testing

15 unit tests (100% line coverage): hashing (including a published SHA-256 vector), path layout and replacement, the caching headers, 304s with every validator form, gzip and its round trip through `quench-archive`, assets that must not be compressed, 404s for foreign paths, the document golden file, nonce freshness and shape, the policy, absence of inline handlers and styles, state and title escaping with hostile input, and the endpoint end to end.

The browser run (`scripts/ui-web-test.fish`, headless Chromium) covers the rest: `view-source` shows the content, the nodes the server sent are the nodes the page keeps, the markup is unchanged by hydration, the embedded state saves a request, damaged server HTML (`?corrupt=tag|text|drop|extra|attr|empty` on the example server) ends as the correct page, the headers of the real assets, and the policy blocking an injected script.

[Home](../README.md)
