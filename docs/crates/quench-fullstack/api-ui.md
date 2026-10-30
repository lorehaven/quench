---
title: API - pages, assets, Ui, UiApp
parent: quench-fullstack
grand_parent: Crates
nav_order: 5
---

Everything here is `not(target_arch = "wasm32")` and re-exported at the crate root.

## `document` (`src/document.rs`)

```rust
pub const ROOT_ID: &str = "app";
pub const STATE_ID: &str = "__q_state";

pub struct Page { /* title, lang, stylesheets, view, state, status, redirect, routed */ }   // Debug: title and status
impl Page {
    #[must_use] pub fn new(title: &str, view: impl IntoView) -> Self;
    #[must_use] pub fn redirect(location: &str) -> Self;
    #[must_use] pub fn routed(title: &str, url: &str, build: impl FnOnce(&Router) -> View) -> Self;
    #[must_use] pub fn lang(self, lang: &str) -> Self;
    #[must_use] pub fn stylesheet(self, href: &str) -> Self;
    #[must_use] pub fn stylesheet_for(self, href: &str, media: &str) -> Self;
    #[must_use] pub fn state(self, json: impl Into<String>) -> Self;
    #[must_use] pub const fn status(self, status: StatusCode) -> Self;
}

#[must_use] pub fn escape_json(json: &str) -> String;
#[must_use] pub fn content_security_policy(nonce: &str) -> String;
#[must_use] pub fn new_nonce() -> String;
#[must_use] pub fn render_document(page: Page, nonce: &str, script: &str, wasm: &str) -> String;
```

`StatusCode` is `quench_http::prelude::http::StatusCode`; `View`, `IntoView` come from `quench_ui`, `Router` from `quench_ui::router`.

| Item | Behaviour |
|---|---|
| `ROOT_ID` | Id of the element the app mounts into and the server renders inside. |
| `STATE_ID` | Id of the `<script type="application/json">` element carrying the state. |
| `Page::new` | Page rendering `view`; language `en`, status `200 OK`, no stylesheets, no state. |
| `Page::redirect` | Not rendered: the response is a `302` with `location` and `cache-control: no-store`. |
| `Page::routed` | Creates a `Router` over a `MemoryHistory` at `url` (the request path and query), calls `build(&router)`. After rendering, if the router's location URL differs from `url` the response is a `302` to it; if `router.is_not_found()` the status becomes `404`. |
| `lang`, `stylesheet`, `stylesheet_for`, `state`, `status` | Builders. `stylesheet` links a file from this origin (the policy allows no inline styles); `stylesheet_for` adds a `media` attribute. `state` JSON is escaped but not validated. `stylesheets` accumulate. |
| `escape_json` | `<` `>` `&` become `<` `>` `&`; U+2028 and U+2029 become ` ` ` `. Only these occur inside JSON strings, so valid JSON stays valid. |
| `content_security_policy(nonce)` | The policy string (see [Internals](internals.md)). |
| `new_nonce()` | 128 random bits (`quench_core::random::bytes::<16>`) as 32 hex characters. |
| `render_document(page, nonce, script, wasm)` | The HTML of a page; `script` and `wasm` are the hashed public paths. Layout in [Internals](internals.md). |

## `assets` (`src/assets.rs`)

```rust
pub const PREFIX: &str = "/_quench/";

#[derive(Clone, Debug)]
pub struct Asset { /* path, etag, content_type, body, gzip */ }
impl Asset {
    #[must_use] pub fn path(&self) -> &str;
    #[must_use] pub fn bytes(&self) -> &[u8];
    #[must_use] pub fn content_type(&self) -> &str;
}

#[derive(Clone, Debug, Default)]
pub struct Assets { /* items */ }
impl Assets {
    #[must_use] pub const fn new() -> Self;
    pub fn add(&mut self, name: &str, body: impl Into<Vec<u8>>, content_type: &str) -> &str;
    #[must_use] pub fn path(&self, name: &str) -> Option<&str>;
    pub fn iter(&self) -> impl Iterator<Item = &Asset>;
    #[must_use] pub fn get(&self, path: &str) -> Option<&Asset>;
    #[must_use] pub fn serve(&self, path: &str, accept_encoding: Option<&str>, if_none_match: Option<&str>) -> Option<Response>;
}

#[must_use] pub fn content_hash(bytes: &[u8]) -> String;
```

| Item | Behaviour |
|---|---|
| `PREFIX` | Where hashed assets are served from. |
| `Asset::path` | Public path, such as `/_quench/app.3f9a1c0b7d2e4a65.wasm`. |
| `content_hash` | First 16 hex digits (8 bytes) of the SHA-256 of `bytes`. |
| `Assets::add(name, body, content_type)` | Adds an asset called `name` (`app.wasm`) and returns its hashed path: `/_quench/<stem>.<hash>.<ext>` (split at the last `.`), or `/_quench/<name>.<hash>` when there is no `.`. Adding the same name again replaces the earlier asset (matched by stem, a 16-hex-digit hash and the same extension). A gzip copy is made when the type is compressible, the body is at least 256 bytes, and the gzip copy is smaller. Compressible: `text/*`, `application/javascript`, `application/json`, `application/wasm`, `image/svg+xml`, `application/xml` (parameters after `;` ignored). The etag is the hash in double quotes. |
| `Assets::path(name)` | Hashed path of the asset added as `name`. |
| `Assets::iter` | In the order added (a replaced asset moves to the end). |
| `Assets::get(path)` | By public path. |
| `Assets::serve` | `None` when `path` does not start with `PREFIX`. A path under the prefix that is not an asset: `404` with `cache-control: no-store`. A matching `If-None-Match` (comma list; `*`; weak `W/` validators match): `304` with `cache-control`, `etag`, `vary: accept-encoding`. Otherwise `200` with `content-type`, `cache-control: public, max-age=31536000, immutable`, `etag`, `vary: accept-encoding`, `x-content-type-options: nosniff`, and `content-encoding: gzip` with the gzip body when a gzip copy exists and `accept_encoding` lists `gzip` (or `*`) with a quality not 0. |

## `app` (`src/app.rs`)

```rust
#[derive(Clone, Debug)]
pub struct Ui { /* assets, script, wasm, dev */ }
impl Ui {
    #[must_use] pub fn new(wasm: impl Into<Vec<u8>>) -> Self;
    #[must_use] pub fn with_script(script: impl Into<Vec<u8>>, wasm: impl Into<Vec<u8>>) -> Self;
    #[must_use] pub const fn dev_reload(self) -> Self;
    pub fn add_asset(&mut self, name: &str, body: impl Into<Vec<u8>>, content_type: &str) -> String;
    #[must_use] pub const fn assets(&self) -> &Assets;
    #[must_use] pub fn paths(&self) -> (&str, &str);
    pub fn export(&self, dir: &std::path::Path, page: Page) -> std::io::Result<()>;
    #[must_use] pub fn render(&self, page: Page) -> Response;
    #[must_use] pub fn render_with_nonce(&self, page: Page, nonce: &str) -> Response;
}

pub struct UiApp { /* ui: Arc<Ui>, pages, prefixes, functions */ }     // Debug: pages and function paths
impl UiApp {
    #[must_use] pub fn new(ui: Ui) -> Self;
    #[must_use] pub fn page(self, path: &str, render: impl Fn(&Request) -> Page + Send + Sync + 'static) -> Self;
    #[must_use] pub fn page_for_user(self, path: &str, render: impl Fn(&Request, Option<User>) -> Page + Send + Sync + 'static) -> Self;
    #[must_use] pub fn pages_under(self, prefix: &str, render: impl Fn(&Request) -> Page + Send + Sync + 'static) -> Self;
    #[must_use] pub fn pages_under_for_user(self, prefix: &str, render: impl Fn(&Request, Option<User>) -> Page + Send + Sync + 'static) -> Self;
    #[must_use] pub fn ui(&self) -> &Ui;
}
impl Endpoint for UiApp {}
```

### `Ui`

| Method | Behaviour |
|---|---|
| `new(wasm)` | `with_script(quench_ui_web::QUENCH_JS, wasm)`. |
| `with_script(script, wasm)` | Adds the loader script as asset `quench.js` (`text/javascript; charset=utf-8`) and the module as `app.wasm` (`application/wasm`). |
| `dev_reload()` | Turns on development reload: a `<script src="/_quench/dev.js">` is inserted before `</body>` of every page and the endpoint serves `/_quench/dev.js` and `/_quench/dev` (see [Internals](internals.md)). Not for production. |
| `add_asset(name, body, content_type)` | `Assets::add`, returns the path as `String`. |
| `assets()` | The asset set. |
| `paths()` | `(hashed quench.js path, hashed app.wasm path)`. |
| `export(dir, page)` | Renders `page` with the fixed nonce `bundle`, creates `dir`, writes `index.html`, and writes each asset to `dir/<path without leading '/'>`. `io::Error` if any write fails. Note: `dev_reload` is not applied to the exported page. |
| `render(page)` | `render_with_nonce(page, &new_nonce())`. |
| `render_with_nonce(page, nonce)` | A page with `redirect` set: `302`. Otherwise the HTML (with the dev script when enabled), status from the page; for routed pages, `302` if the router landed elsewhere, `404` if not found. Headers: `cache-control: no-store`, `content-security-policy`, `x-content-type-options: nosniff`, `referrer-policy: same-origin`. A fixed nonce protects nothing; it is for tests. |

### `UiApp`

An `Endpoint` combining assets, server functions and pages.

| Method | Behaviour |
|---|---|
| `new(ui)` | Collects the `#[server]` functions linked into the binary (`server::functions()`). **Panics** if two have the same path. |
| `page(path, render)` | Page at the exact path. |
| `page_for_user(path, render)` | As `page`; `render` receives `Some(User)` when a valid session or bearer token exists, `None` otherwise (the lookup is `User::from_request`, errors become `None`). |
| `pages_under(prefix, render)` | Page for `prefix` and every path under it. The prefix has trailing `/` removed; matching is by whole segments (rest is empty or starts with `/`). Exact pages win over prefixes; prefixes are tried in registration order. |
| `pages_under_for_user(prefix, render)` | As above with the user. |
| `ui()` | The `Ui`. |

Request routing in `call`, in order: (1) with dev reload on, `/_quench/dev.js` and `/_quench/dev`; (2) paths starting with `/_quench/` go to `Assets::serve` (`404` when not an asset; note this includes unmatched dev paths when dev reload is off); (3) the path of a `#[server]` function (any method; the function's own `guard` rejects non-POST); (4) exact pages, then prefix pages; (5) `/favicon.ico` with no page: `204` with `cache-control: public, max-age=86400`; (6) `404`. The same page path registered twice: the first registered wins.
