---
title: Router API
parent: quench-ui
nav_order: 9
---

Reference of `quench_ui::router` (source: `quench-ui/src/router.rs`): URL patterns with parameters, nested layouts that stay mounted, guards with redirects, and a history that the browser, a test or the server provides. Worked examples are in [usage-app](usage-app.md).

Everything here is single-threaded: `Router`, `Routes`, `Layout`, `RouteContext` and `MemoryHistory` hold `Rc`/`RefCell` and are `!Send` and `!Sync`. A `Router` is cheap to clone (a shared `Rc`). The browser's `History` implementation (`BrowserHistory`) lives in `quench-ui-web`, not in this crate.

```rust
use quench_ui::router::{
    Guard, History, Layout, Location, MemoryHistory, Params, RouteContext, Router, Routes, Visit,
    layout, link, percent_decode, use_router,
};
```

## Location

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Location {
    pub path: String,
    pub query: String,
    pub hash: String,
}
```

A URL split into the parts a router works with. `query` and `hash` are kept as written, without their `?` and `#`. `Location::default()` has empty strings (its `path` is empty, not `/`; only `parse` substitutes `/`).

| Item | Signature | Behaviour |
|---|---|---|
| `parse` | `#[must_use] pub fn parse(url: &str) -> Self` | Splits at the first `#` (hash), then the first `?` of the remainder (query). An empty path becomes `/`. Nothing is decoded or validated: `Location::parse("a/b").path` is `"a/b"`. |
| `url` | `#[must_use] pub fn url(&self) -> String` | `path`, then `?query` and `#hash` each only when not empty. `parse(u).url()` equals `u` for a URL that has a path (an empty query after a bare `?` is lost: `/p?` gives `/p`). |
| `query_pairs` | `#[must_use] pub fn query_pairs(&self) -> Vec<(String, String)>` | Splits `query` at `&`, skips empty pairs, splits each at the first `=` (no `=` gives an empty value) and percent-decodes both sides, with `+` as a space. |
| `query_get` | `#[must_use] pub fn query_get(&self, name: &str) -> Option<String>` | The first value of the parameter `name` (decoded), or `None`. A bare `?flag` gives `Some("")`. |

```rust
use quench_ui::router::Location;

let l = Location::parse("/a/b?x=1&y=two%20words+more#top");
assert_eq!((l.path.as_str(), l.hash.as_str()), ("/a/b", "top"));
assert_eq!(l.query_get("y").as_deref(), Some("two words more"));
assert_eq!(l.url(), "/a/b?x=1&y=two%20words+more#top");
```

## percent_decode

```rust
#[must_use]
pub fn percent_decode(text: &str) -> String
```

Decodes `%XX` escapes (upper- or lower-case hex). A `%` that is not followed by two hex digits stays as it is. The decoded bytes are read as UTF-8; invalid sequences become U+FFFD. `+` is **not** turned into a space here (only inside `Location::query_pairs`/`query_get`). `percent_decode("a%20b%zz%4")` is `"a b%zz%4"`; `percent_decode("%C5%82")` is `"ł"`.

## Params

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Params(/* private */);
```

The values that the `:name` and `*name` segments of the matched route took, percent-decoded. A pattern that is nested in a layout contributes its parameters after the layout's own.

* `#[must_use] pub fn get(&self, name: &str) -> Option<&str>`: the first parameter called `name`.

## Guard and Visit

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Guard {
    Allow,
    /// Go to this URL instead.
    Redirect(String),
    /// Show the router's forbidden view.
    Deny,
}

#[derive(Clone, Debug)]
pub struct Visit {
    pub location: Location,
    pub params: Params,
}
```

A guard is a `Fn(&Visit) -> Guard`. `Visit` is the navigation it decides on: `location` is the location being resolved (after a redirect, the redirect target) and `params` the parameters the match produced.

Guards are evaluated during route resolution, which is a memo over the router's location: a guard that reads a signal (a "signed in" flag) is re-run when that signal changes, so signing out redirects at once. The guards of a matched chain run outermost layout first, then inner layouts, then the page; the first verdict that is not `Allow` wins and the rest do not run.

* `Allow`: show the route.
* `Redirect(url)`: resolve `url` instead. Up to `MAX_REDIRECTS = 8` redirects are followed in one resolution (the guards of each target run too). The final location **replaces** the current history entry (`Router::replace`). If a redirect target matches no route, or the 8 hops are used up, the result is not-found (see below for the exact cases).
* `Deny`: show the `forbidden` view; the location is not changed.

## Routes

```rust
pub struct Routes { /* private */ }
impl Default for Routes   // = Routes::new()
impl Debug for Routes     // opaque
```

The route table under construction. All builder methods are `#[must_use]` and take `self` by value.

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `#[must_use] pub const fn new() -> Self` | Empty table. |
| `route` | `pub fn route(self, pattern: &str, page: impl Fn(&RouteContext) -> View + 'static) -> Self` | A page for URLs matching `pattern`. Earlier routes win. |
| `guarded_route` | `pub fn guarded_route(self, pattern: &str, guard: impl Fn(&Visit) -> Guard + 'static, page: impl Fn(&RouteContext) -> View + 'static) -> Self` | As `route`, with a guard. |
| `layout` | `pub fn layout(self, layout: Layout) -> Self` | Adds a layout built with the free function `layout`. |
| `not_found` | `pub fn not_found(self, page: impl Fn(&RouteContext) -> View + 'static) -> Self` | The view for a URL that no route matches. Without it the text `Not found` is shown. |
| `forbidden` | `pub fn forbidden(self, page: impl Fn(&RouteContext) -> View + 'static) -> Self` | The view for `Guard::Deny`. Without it the text `Forbidden` is shown. |
| `view` | `#[must_use] pub fn view(self, router: &Router) -> View` | The view that shows the matching route and rebuilds as the router's location changes. Calls `router.provide()`, so `use_router` and `link` work for everything inside. |

### Pattern syntax

A pattern is split at `/`; empty segments are dropped (so a leading, trailing or doubled slash does not matter, and `""` and `"/"` both mean "no segments"). Each segment is:

* `name`: a literal; compared with the **percent-decoded** URL segment, case-sensitively.
* `:name`: any one segment, bound to the parameter `name`.
* `*name`: the rest of the path, bound to `name` as the remaining segments joined with `/` (decoded segment by segment, so `a%2Fb` gives `a/b`, indistinguishable from two segments). It matches zero or more segments; it should be last (segments written after it are ignored).

A page route matches only when the pattern consumes **all** URL segments. A layout pattern is a prefix: the rest of the URL is matched against its children, in the order they were added; if no child matches, matching continues with the next sibling of the layout.

`Routes::view` builds the table inside `View::lazy`, so the table is built (and the router context provided) when the view is mounted, under the mount's scope. Build the `Routes` inside a component or the root closure, not before the reactive root exists.

### Resolution outcome

For a location the router produces one of: *matched* (a chain of layouts ending in a page), *not found*, *forbidden*, or *redirected*. Exact behaviour:

* No route matches: not-found (the `not_found` page, or the text `Not found`).
* A guard returns `Deny`: forbidden, at the top level (the whole table is replaced by the `forbidden` page, including layouts).
* A guard returns `Redirect(url)`: `url` is parsed and resolved again. If the redirect target does not match a route, the result is still a redirect (the router navigates there and then shows not-found); if after 8 hops the guards still redirect, the result is not-found.
* After a redirect the router calls `replace(&target.url())` inside an effect (untracked), so the history entry becomes the target.
* While the top level is showing not-found, `Router::is_not_found()` is true; it is updated each time the top-level view is built.

### Parameters are reactive, pages are not rebuilt

A page function runs **once** per matched route (untracked). Navigating between URLs of the same route (`/users/1` to `/users/2`) does not rebuild the page: `RouteContext::param` reads a memo, so a view closure that calls it follows the change and the existing nodes are patched. Moving to a different route rebuilds the page. A layout stays mounted while navigation moves between its children.

## Layout

```rust
#[must_use]
pub fn layout(pattern: &str, wrap: impl Fn(&RouteContext, View) -> View + 'static) -> Layout

pub struct Layout { /* private */ }
impl Debug for Layout   // opaque
```

A layout for URLs that start with `pattern`. `wrap` receives the context and the view of whichever child route matched, and returns the page around it. The wrap function runs once while the layout stays selected (untracked); the inner view is a dynamic region that switches children.

All methods are `#[must_use]` and take `self` by value:

| Method | Signature | Behaviour |
|---|---|---|
| `route` | `pub fn route(self, pattern: &str, page: impl Fn(&RouteContext) -> View + 'static) -> Self` | A page inside the layout. The pattern is relative to the layout's. `""` is the index page, shown when the layout's own pattern is matched exactly. |
| `guarded_route` | `pub fn guarded_route(self, pattern: &str, guard: impl Fn(&Visit) -> Guard + 'static, page: impl Fn(&RouteContext) -> View + 'static) -> Self` | A page with a guard. |
| `layout` | `pub fn layout(self, layout: Self) -> Self` | A layout nested in this one; its pattern is relative to this layout's. |
| `guard` | `pub fn guard(self, guard: impl Fn(&Visit) -> Guard + 'static) -> Self` | Checks `guard` before anything inside this layout is shown. Call it before adding the layout to its parent (it consumes the value). |

## RouteContext

```rust
#[derive(Clone)]
pub struct RouteContext { /* private */ }
impl Debug for RouteContext   // opaque
```

What a page or layout function receives. Clone it into view closures (`let ctx = ctx.clone();`).

* `#[must_use] pub fn param(&self, name: &str) -> Option<String>`: the URL parameter `name`. Reads a memo, so reading it inside a view closure follows navigation between URLs of the same route. `None` for a name the matched pattern does not have, and always `None` while the route is not matched (not-found, forbidden).
* `#[must_use] pub fn location(&self) -> Location`: the current location (reactive: reads the router's signal).
* `#[must_use] pub const fn router(&self) -> &Router`.

## History

```rust
pub trait History {
    fn location(&self) -> Location;
    /// Adds an entry.
    fn push(&self, location: &Location);
    /// Replaces the current entry.
    fn replace(&self, location: &Location);
    /// Goes back one entry. False when there is none to go back to.
    fn back(&self) -> bool;
    fn forward(&self) -> bool;
    /// Calls `listener` whenever the location changes other than through `push` and `replace`
    /// (the back and forward buttons).
    fn subscribe(&self, listener: Rc<dyn Fn(Location)>);
}
```

Where URLs come from and go to. All methods take `&self`; implementations use interior mutability. Implementations in the workspace: `MemoryHistory` (below), `quench_ui_web::BrowserHistory`. The `Router` is the only caller of `subscribe`, once, in `Router::new`.

## MemoryHistory

```rust
pub struct MemoryHistory { /* private */ }
impl History for MemoryHistory
impl Debug for MemoryHistory   // opaque
```

A history in memory, for tests and for rendering on the server.

* `#[must_use] pub fn new(url: &str) -> Self`: one entry, `Location::parse(url)`, current.
* `#[must_use] pub fn entries(&self) -> Vec<Location>`: every entry, oldest first (the current one is not marked).
* `push` drops all entries after the current one (forward history), appends and makes the new entry current.
* `replace` overwrites the current entry.
* `back` / `forward` return `false` at the first / last entry; otherwise move the index and call every subscribed listener with the new location, then return `true`.

## Router

```rust
#[derive(Clone)]
pub struct Router(/* Rc */);
impl Debug for Router   // prints `Router(<current url>)`
```

The current location (a signal) and the means to change it.

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `#[must_use] pub fn new(history: Rc<dyn History>) -> Self` | Reads the history's location into a signal and subscribes to back/forward changes (updating with `set_if_changed`). The signal is created in the current scope (inside `create_root` or a component), as with every signal. |
| `provide` | `pub fn provide(&self)` | `provide_context(self.clone())` under the current scope. `Routes::view` does this itself. |
| `location` | `#[must_use] pub fn location(&self) -> Location` | Reactive read. |
| `is_not_found` | `#[must_use] pub fn is_not_found(&self) -> bool` | Whether the last top-level route shown was the not-found page, for a server to answer 404. Plain flag, not reactive. |
| `navigate` | `pub fn navigate(&self, url: &str)` | Parses `url`. If it equals the current location (path, query and hash all) the current entry is **replaced**, otherwise an entry is pushed. Then the location signal is set. Guards are not run here but by the route resolution that follows. |
| `replace` | `pub fn replace(&self, url: &str)` | Replaces the current history entry and sets the location. |
| `back` | `#[must_use] pub fn back(&self) -> bool` | `History::back`. Does not consult `on_back` handlers. |
| `forward` | `#[must_use] pub fn forward(&self) -> bool` | `History::forward`. |
| `on_back` | `pub fn on_back(&self, handler: impl Fn() -> bool + 'static)` | Registers a back handler. It returns true when it used the action up (closing a sheet). Handlers run newest first. Registered under the **current scope**: it is removed by `on_cleanup` when that scope is disposed. |
| `handle_back` | `#[must_use] pub fn handle_back(&self) -> bool` | For the host's back button: runs the handlers newest first and stops at the first that returns true (true is returned); if none does, calls `back()`. `false` means nothing was left to do, so the host should leave the app. |

`on_back` calls `on_cleanup`, so it has an effect (and cleanup) only inside a scope; the handler list is snapshotted before running, so a handler may register or remove handlers.

## use_router

```rust
#[must_use]
pub fn use_router() -> Router
```

The router provided by `Router::provide` or `Routes::view`, found through `use_context::<Router>()`.

**Panics** with `use_router needs a router: call Router::provide first` when no router was provided in the scope or an ancestor.

## link

```rust
#[must_use]
pub fn link(href: &str) -> Element
```

An `<a href="...">` element.

* If `href` starts with `/` and not with `//` (a path inside the app), the link navigates without a page load: it calls `use_router()`, adds `aria-current="page"` while the router's current path equals `Location::parse(href).path` (string equality on the raw paths; query and hash are ignored), calls `.intercept_click()` (a plain left click's default is prevented; ctrl-, shift- and middle-clicks are left to the browser) and an `on_click` that does `router.navigate(href)`.
* Any other `href` (`https://...`, `//host`, `mailto:`, `#top`, `page.html`) gives an ordinary link with only the `href` attribute, and **no router is needed**.

**Panics** (via `use_router`) when `href` is a path starting with `/` and no router was provided.

The doc comment in the source says "A relative `href`"; the code's test is "starts with `/`", as described above.
