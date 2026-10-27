# Quench Fullstack

`quench-fullstack` (crate `quench_fullstack`) serves [`quench-ui`](./quench-ui.md) apps from [`quench-http`](./quench-http.md) and lets them call the server. The page is rendered on the server from the same components the browser runs, the browser hydrates it ([`quench-ui-web`](./quench-ui-web.md)), and everything goes out with content-hashed asset names and a strict Content-Security-Policy. A function marked [`#[server]`](./quench-fullstack-macros.md) is an ordinary `async fn` to the code that calls it, in the browser or on the server; sessions work by cookie in the browser and by bearer token in a native shell.

The crate builds for both targets. For `wasm32` it is small (`quench-core`, `quench-serde`, `quench-ui`, `quench-ui-web` and the macro): the typed call, `ServerError`, `User`, the client sessions. Everything that needs `quench-http`, `ring` or the file system (pages, assets, the server half of calls, the stub token service) is server-only.

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

## Server functions

```rust
// in a crate the server and the browser both build
#[server(auth)]
pub async fn create_note(text: String) -> Result<Note, ServerError> {
    if text.trim().is_empty() {
        return Err(ServerError::invalid("text", "write something"));
    }
    Ok(store().add(&user()?.sub, text.trim()))
}

// in a component, in the browser or anywhere
spawn(async move {
    match create_note(draft).await {
        Ok(note) => notes.update(|list| list.push(note)),
        Err(e) => message.set(e.message().to_string()),
    }
});
```

One signature on both targets, so shared code calls it without `cfg`. On the server the body is the function and a `POST` route is registered for it (`/_fn/<name>`, or `#[server(path = "/api/..")]`), found by `UiApp` through the same before-`main` registry the route macros use. In the browser the body is replaced by a call to that route. Arguments travel as a JSON object by name and the result as JSON, using `quench-serde` (`#[derive(Serialize, Deserialize)]`, which the shared crate needs as a dependency anyway); enums with data, `Option`, maps, nested vectors, unicode and the full `i64`/`u64` range round-trip (tests check them). `#[server(auth)]` requires a session and `#[server(auth = "optional")]` looks one up; inside, `user()` returns the [`User`](#sessions).

What a call must be, checked before anything else runs (`server::guard`): a `POST` of `application/json` carrying `x-quench-rpc: 1`. A cross-origin page cannot add a custom header without a CORS preflight the server never grants, so the header is the protection against cross-site request forgery for cookie sessions; as a second check a call that relies on cookies (no `Authorization` header) must have `Sec-Fetch-Site: same-origin` and an `Origin` equal to the `Host`. Calls with a bearer token are exempt from the origin checks, since a token in a header is not something a hostile page can attach. Bodies over 1 MiB and arguments the function cannot read are refused with a 422 and an error body, not a crash.

A binary that links the crate defining server functions registers them; one that only depends on the crate without referencing it can have them dropped by the linker (as for routes), and two functions with one path stop `UiApp::new` with a message naming the path.

## Errors

A server function returns `Result<T, ServerError>`:

| Variant | Status | Meaning |
|---|---|---|
| `Unauthorized(msg)` | 401 | not signed in, or the session ended |
| `Forbidden(msg)` | 403 | signed in but not allowed (also the protocol checks above) |
| `NotFound(msg)` | 404 | there is no such thing |
| `Validation { message, fields }` | 422 | the input is not acceptable; `ServerError::invalid("title", "too short")` names the field |
| `Internal(msg)` | 500 | the server broke; the cause is logged and the browser sees only `internal server error` |
| `Network(msg)`, `BadResponse(msg)` | none | browser only: the request could not be made, or the answer was not ours (a proxy's HTML page) |

The body is `{"kind":"not_found","message":"...","fields":[{"field","message"}]}`; the browser decodes it back to the same variant, and when a body names no kind it falls back on the status. Extractor failures from `quench-http` (`HttpError`) map the same way.

## Sessions

**Cookie, in the browser.** The browser sends the session cookie with every same-origin call; nothing in the page holds a token. `client::sign_in(base, user, password)` posts to `{base}/login` and lets the answer set the cookies; `client::sign_out(base)` clears them. When a call ends as `Unauthorized` and no refresh can help, the handler registered with `client::on_unauthorized` runs: the place to show the sign-in form.

**Bearer, in a native shell (Android, desktop).** `client::Bearer::new("/api/v1/auth", store)` keeps an access and a refresh token in a `TokenStore` the host provides (its secure storage; `MemoryStore` for tests), and `client::use_bearer(bearer)` makes every call carry `Authorization: Bearer ..`. It speaks gatehouse's contract (`POST {base}/login` with `{username, password}`, `POST {base}/refresh` with `{refresh_token}`, each answering `{access_token, refresh_token, token_type, expires_in}`). When a call meets a 401 the tokens are refreshed once and the call retried. The refresh token **rotates**, so any number of calls that meet the expiry together share one refresh (one runs, the rest wait for it); a refresh token the server no longer accepts ends the session: the store is cleared, the error is `Unauthorized` and `on_unauthorized` runs.

**On the server**, `User` is built from the claims `quench-auth`'s `Auth` middleware already verified, else from a bearer token (it must verify, be for this service and belong to a live session), else from the session cookie, using the `JwtConfig` in the container; with `SERVICE_AUTH_ENABLED` off, as in development, everyone is an administrator. `UiApp::page_for_user(path, |request, user| Page)` renders a page for whoever is asking, so the first response is already the signed-in view or the sign-in form. Token validation allows 60 seconds of clock skew.

`testing::StubGatehouse` is a gatehouse-compatible token service for development and tests (`/api/v1/auth/login`, `/refresh`, `/logout`, real signed tokens, rotating refresh tokens that work once, cookies set on login). It is what the notes example and the session tests run against; never put it in front of real users.

`/favicon.ico` is answered with an empty, cacheable `204` unless a page is registered at that path, so a browser's first-visit request does not log a 404.

## Development reload

`Ui::dev_reload()` is for a server run by `quench dev` ([`quench-tool`](./quench-tool.md)): every page gets `<script src="/_quench/dev.js">`, and `/_quench/dev` is a server-sent events stream that never carries a message. The script reloads the page when the stream drops and the server answers again, so a rebuilt and restarted server shows up in the browser by itself. The policy already allows a script from this origin. Leave it off in production: it adds a request to every page.

```rust
let mut ui = Ui::new(wasm);
if std::env::var_os("QUENCH_DEV").is_some() {
    ui = ui.dev_reload();
}
```

## Server-sent events

`quench-http` streams server-sent events (`Response::sse`), and `quench_ui_web::EventSource` reads them in the browser:

```rust
let source = Rc::new(EventSource::open("/api/notes/stream", &["note"]));   // same origin, with the cookie
let reader = source.clone();
spawn(async move {
    while let Some(event) = reader.next().await {
        if let SseEvent::Message { data, .. } = event { activity.update(|log| log.push(data)); }
    }
});
on_cleanup(move || source.close());   // the view that opened it closes it
```

The browser reconnects by itself (and says so with `SseEvent::Reconnecting`), and dropping or closing the source ends it. The example serves `/api/notes/stream`, which pushes an event when the signed-in user's notes change (and is 401 without a session); the signed-in notes view subscribes from Rust and shows the activity, including a note added from another device, which the browser test does. While building this, a streaming response was found to hold back its status line and headers until the first event, so an `EventSource` never opened; `quench-proto` now sends the head as soon as it is clear the first chunk is not ready.

## A page

`Page::new(title, view)` with `.lang(..)`, `.stylesheet(href)`, `.stylesheet_for(href, media)` (for a theme per `prefers-color-scheme`), `.state(json)` and `.status(..)`. `Page::redirect(location)` is a `302` instead of a page, and `Page::routed(title, url, |router| view)` renders an app that routes: the router starts at `url`, and after rendering the response is a `302` if the routes redirected (a guard), a `404` if none matched, and the page otherwise, so the browser's router starts at the same place. It renders to

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
- **No inline handlers or styles.** Views cannot produce `on*` attributes (`quench-ui` drops them) and `Page::stylesheet` links a file. `Element::style(..)` writes a `style` attribute, which this policy blocks: use classes and a stylesheet, or `Element::css(..)` (the style object) for values that depend on layout. Server-rendered markup does not carry `css` values; they are applied in the browser.
- **Escaping** is done where the markup is made: text, attribute values, the title and language, and the state block each have tests with hostile input (`</script>`, `"` in attributes, U+2028).
- Also sent: `x-content-type-options: nosniff`, `referrer-policy: same-origin`.

## `UiApp`

An `Endpoint` that answers `/_quench/..` from the assets, `#[server]` functions, exact paths from the pages registered with `.page(path, |request| Page)` / `.page_for_user(..)`, and every path under a prefix from `.pages_under(prefix, ..)` / `.pages_under_for_user(..)` (the deep links of an app that routes in the browser: `/app`, `/app/users/7`; whole segments only, exact pages win); anything else is a 404. A page holds `Rc`s, so it is built and rendered inside one synchronous call.

## Testing

64 unit tests (96% line coverage). Pages and assets: hashing (including a published SHA-256 vector), path layout and replacement, the caching headers, 304s, gzip and its round trip through `quench-archive`, the document golden file, nonce freshness, the policy, escaping with hostile input, the endpoint end to end. Server functions: every data shape through the real client against the real server in one process (the client runs on `quench-ui-web`'s in-memory host, whose `fetch`es are handed to the endpoint and answered), arguments the function cannot read, every error kind with its status, the generic internal error, the protocol checks (method, content type, the custom header, `Sec-Fetch-Site`, `Origin` with ports and lookalikes, bearer exemption, oversize and empty bodies), and sessions: cookie and bearer, expired, tampered, foreign-signed and wrong-audience tokens, `auth` and `auth = "optional"`, one refresh for five concurrent calls, a dead refresh token ending the session, sign-in and sign-out, and the stub token service itself. The macro has its own tests for the expansion on each target and for every mistake it reports.

The browser run (`scripts/ui-web-test.fish`, headless Chromium, 75 checks) adds the real thing: view-source shows the content, the nodes the server sent are the nodes the page keeps, damaged server HTML ends as the correct page, the headers of the real assets, the policy blocking an injected script, and the notes app end to end. There, a call without the header is 403, without a session 401, from another site 403; a bearer token works without a cookie and its refresh token rotates and cannot be reused; the sign-in form is rendered by the server for a signed-out visitor; a wrong password shows the server's reason; signing in lists the user's notes; an empty note is refused naming the field; notes are added and marked done; the session cookie is invisible to the page; after a reload the server's page already has the user's notes and hydration keeps its nodes; a server-sent event arrives when a note is added; signing out ends the session on the server; and another user sees none of it and cannot change it (404).

[Home](../README.md)
