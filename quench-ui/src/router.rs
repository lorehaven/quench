//! Client-side routing: URL patterns with parameters, nested layouts, guards, and a history the
//! browser, a test or the server provides.
//!
//! ```
//! use std::rc::Rc;
//! use quench_ui::prelude::*;
//! use quench_ui::router::{MemoryHistory, Router, Routes};
//!
//! let (view, _scope) = create_root(|| {
//!     let router = Router::new(Rc::new(MemoryHistory::new("/users/7")));
//!     Routes::new()
//!         .route("/", |_| p().text("home").into_view())
//!         .route("/users/:id", |ctx| {
//!             let ctx = ctx.clone();
//!             p().text_fn(move || format!("user {}", ctx.param("id").unwrap_or_default())).into_view()
//!         })
//!         .view(&router)
//! });
//! # drop(view);
//! ```

use crate::reactive::{Memo, Signal, create_effect, create_memo, create_signal, on_cleanup};
use crate::reactive::{provide_context, untrack, use_context};
use crate::view::{Element, View, a};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The most redirects one navigation may follow before the router gives up on it.
const MAX_REDIRECTS: usize = 8;

/// A URL split into the parts a router works with. The query and hash are kept as written, without
/// their `?` and `#`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Location {
    pub path: String,
    pub query: String,
    pub hash: String,
}

impl Location {
    /// Splits `url` (a path with an optional query and hash). A missing path is `/`.
    #[must_use]
    pub fn parse(url: &str) -> Self {
        let (rest, hash) = url.split_once('#').unwrap_or((url, ""));
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        Self {
            path: if path.is_empty() {
                "/".into()
            } else {
                path.into()
            },
            query: query.into(),
            hash: hash.into(),
        }
    }

    /// The URL again: path, then `?query` and `#hash` when they are not empty.
    #[must_use]
    pub fn url(&self) -> String {
        let mut url = self.path.clone();
        if !self.query.is_empty() {
            url.push('?');
            url.push_str(&self.query);
        }
        if !self.hash.is_empty() {
            url.push('#');
            url.push_str(&self.hash);
        }
        url
    }

    /// The query's `name=value` pairs, decoded.
    #[must_use]
    pub fn query_pairs(&self) -> Vec<(String, String)> {
        self.query
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| {
                let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
                (decode(name, true), decode(value, true))
            })
            .collect()
    }

    /// The first value of the query parameter `name`.
    #[must_use]
    pub fn query_get(&self, name: &str) -> Option<String> {
        self.query_pairs()
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
    }

    fn segments(&self) -> Vec<String> {
        self.path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| decode(s, false))
            .collect()
    }
}

/// Percent-decodes `text` (and `+` as a space, in a query). Bytes that are not valid UTF-8 after
/// decoding become U+FFFD; a `%` not followed by two hex digits stays as it is.
#[must_use]
pub fn percent_decode(text: &str) -> String {
    decode(text, false)
}

fn decode(text: &str, plus_is_space: bool) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let pair = (b == b'%').then(|| bytes.get(i + 1..i + 3)).flatten();
        let value = pair.and_then(|p| Some(hex(p[0])? << 4 | hex(p[1])?));
        if let Some(v) = value {
            out.push(v);
            i += 3;
        } else {
            out.push(if plus_is_space && b == b'+' { b' ' } else { b });
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

const fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// The values a route's `:name` and `*name` segments matched, decoded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Params(Vec<(String, String)>);

impl Params {
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Segment {
    Static(String),
    Param(String),
    /// Everything that is left, joined with `/`.
    Rest(String),
}

#[derive(Clone, Debug)]
struct Pattern(Vec<Segment>);

impl Pattern {
    fn parse(pattern: &str) -> Self {
        Self(
            pattern
                .split('/')
                .filter(|s| !s.is_empty())
                .map(|s| match s.as_bytes()[0] {
                    b':' => Segment::Param(s[1..].into()),
                    b'*' => Segment::Rest(s[1..].into()),
                    _ => Segment::Static(s.into()),
                })
                .collect(),
        )
    }

    /// How many leading `segments` this pattern consumes, and what its parameters matched.
    fn matches(&self, segments: &[String]) -> Option<(usize, Vec<(String, String)>)> {
        let mut params = Vec::new();
        for (i, part) in self.0.iter().enumerate() {
            match part {
                Segment::Static(s) => {
                    if segments.get(i)? != s {
                        return None;
                    }
                }
                Segment::Param(name) => params.push((name.clone(), segments.get(i)?.clone())),
                Segment::Rest(name) => {
                    params.push((name.clone(), segments.get(i..)?.join("/")));
                    return Some((segments.len(), params));
                }
            }
        }
        Some((self.0.len(), params))
    }
}

/// Where a navigation is allowed to go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Guard {
    Allow,
    /// Go to this URL instead.
    Redirect(String),
    /// Show the router's forbidden view.
    Deny,
}

/// What a guard sees of the navigation it decides on.
#[derive(Clone, Debug)]
pub struct Visit {
    pub location: Location,
    pub params: Params,
}

type Matched = (Vec<usize>, Vec<(String, String)>);
type Listener = Rc<dyn Fn(Location)>;
type GuardFn = Rc<dyn Fn(&Visit) -> Guard>;
type PageFn = Rc<dyn Fn(&RouteContext) -> View>;
type LayoutFn = Rc<dyn Fn(&RouteContext, View) -> View>;

enum Kind {
    Page(PageFn),
    Layout(LayoutFn, Vec<usize>),
}

struct Node {
    pattern: Pattern,
    kind: Kind,
    guard: Option<GuardFn>,
}

/// A route being declared: [`Routes::route`] and [`Routes::layout`] build the top level, and
/// [`Layout`] holds the routes inside a layout.
pub struct Routes {
    nodes: Vec<Node>,
    top: Vec<usize>,
    not_found: Option<PageFn>,
    forbidden: Option<PageFn>,
}

impl Default for Routes {
    fn default() -> Self {
        Self::new()
    }
}

/// A layout route under construction: a pattern, the function that wraps what its child routes
/// show, and the child routes.
pub struct Layout {
    pattern: Pattern,
    wrap: LayoutFn,
    guard: Option<GuardFn>,
    children: Vec<ChildRoute>,
}

struct ChildRoute {
    pattern: Pattern,
    page: Option<PageFn>,
    layout: Option<Layout>,
    guard: Option<GuardFn>,
}

/// A layout for URLs starting with `pattern`; add its child routes with [`Layout::route`].
///
/// `wrap` receives the view of whichever child route matched and returns the page around it. The
/// layout stays mounted while navigation moves between its children.
#[must_use]
pub fn layout(pattern: &str, wrap: impl Fn(&RouteContext, View) -> View + 'static) -> Layout {
    Layout {
        pattern: Pattern::parse(pattern),
        wrap: Rc::new(wrap),
        guard: None,
        children: Vec::new(),
    }
}

impl Layout {
    /// A page inside this layout. The pattern is relative to the layout's; `""` is the index page,
    /// shown when the layout's own pattern is matched exactly.
    #[must_use]
    pub fn route(mut self, pattern: &str, page: impl Fn(&RouteContext) -> View + 'static) -> Self {
        self.children.push(ChildRoute {
            pattern: Pattern::parse(pattern),
            page: Some(Rc::new(page)),
            layout: None,
            guard: None,
        });
        self
    }

    /// A page with a guard.
    #[must_use]
    pub fn guarded_route(
        mut self,
        pattern: &str,
        guard: impl Fn(&Visit) -> Guard + 'static,
        page: impl Fn(&RouteContext) -> View + 'static,
    ) -> Self {
        self.children.push(ChildRoute {
            pattern: Pattern::parse(pattern),
            page: Some(Rc::new(page)),
            layout: None,
            guard: Some(Rc::new(guard)),
        });
        self
    }

    /// A layout nested in this one.
    #[must_use]
    pub fn layout(mut self, layout: Self) -> Self {
        self.children.push(ChildRoute {
            pattern: layout.pattern.clone(),
            page: None,
            guard: layout.guard.clone(),
            layout: Some(layout),
        });
        self
    }

    /// Checks `guard` before anything inside this layout is shown.
    #[must_use]
    pub fn guard(mut self, guard: impl Fn(&Visit) -> Guard + 'static) -> Self {
        self.guard = Some(Rc::new(guard));
        self
    }
}

impl Routes {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            nodes: Vec::new(),
            top: Vec::new(),
            not_found: None,
            forbidden: None,
        }
    }

    /// A page for URLs matching `pattern`: segments are literal, `:name` (one segment) or
    /// `*name` (the rest of the path, last). Earlier routes win.
    #[must_use]
    pub fn route(mut self, pattern: &str, page: impl Fn(&RouteContext) -> View + 'static) -> Self {
        let id = self.push(Pattern::parse(pattern), Kind::Page(Rc::new(page)), None);
        self.top.push(id);
        self
    }

    /// A page with a guard.
    #[must_use]
    pub fn guarded_route(
        mut self,
        pattern: &str,
        guard: impl Fn(&Visit) -> Guard + 'static,
        page: impl Fn(&RouteContext) -> View + 'static,
    ) -> Self {
        let id = self.push(
            Pattern::parse(pattern),
            Kind::Page(Rc::new(page)),
            Some(Rc::new(guard)),
        );
        self.top.push(id);
        self
    }

    /// A layout, from [`layout`].
    #[must_use]
    pub fn layout(mut self, layout: Layout) -> Self {
        let id = self.add_layout(layout);
        self.top.push(id);
        self
    }

    /// What to show when no route matches.
    #[must_use]
    pub fn not_found(mut self, page: impl Fn(&RouteContext) -> View + 'static) -> Self {
        self.not_found = Some(Rc::new(page));
        self
    }

    /// What to show when a guard denies the navigation.
    #[must_use]
    pub fn forbidden(mut self, page: impl Fn(&RouteContext) -> View + 'static) -> Self {
        self.forbidden = Some(Rc::new(page));
        self
    }

    fn push(&mut self, pattern: Pattern, kind: Kind, guard: Option<GuardFn>) -> usize {
        self.nodes.push(Node {
            pattern,
            kind,
            guard,
        });
        self.nodes.len() - 1
    }

    fn add_layout(&mut self, layout: Layout) -> usize {
        let children = layout
            .children
            .into_iter()
            .map(|c| match (c.page, c.layout) {
                (Some(page), _) => self.push(c.pattern, Kind::Page(page), c.guard),
                (None, Some(inner)) => self.add_layout(inner),
                (None, None) => unreachable!("a child route has a page or a layout"),
            })
            .collect();
        self.push(
            layout.pattern,
            Kind::Layout(layout.wrap, children),
            layout.guard,
        )
    }

    /// The view that shows the matching route, rebuilt as `router`'s location changes. Also makes
    /// `router` available to [`use_router`] and [`link`] for everything inside.
    #[must_use]
    pub fn view(self, router: &Router) -> View {
        let router = router.clone();
        // Built inside a region of its own so it has a scope to hold the router's context.
        View::lazy(move || self.build(&router))
    }

    fn build(self, router: &Router) -> View {
        router.provide();
        let shared = Rc::new(Shared {
            nodes: self.nodes,
            top: self.top,
            not_found: self.not_found,
            forbidden: self.forbidden,
        });
        let resolution = {
            let (shared, router) = (shared.clone(), router.clone());
            create_memo(move || shared.resolve(&router.location()))
        };
        let params = {
            let resolution = resolution.clone();
            create_memo(move || match resolution.get() {
                Resolution::Matched { params, .. } => params,
                _ => Params::default(),
            })
        };
        let ctx = RouteContext {
            router: router.clone(),
            params,
        };
        let redirects = router.clone();
        {
            let resolution = resolution.clone();
            create_effect(move || {
                if let Resolution::Redirected(to) = resolution.get() {
                    untrack(|| redirects.replace(&to.url()));
                }
            });
        }
        level(&shared, &resolution, &ctx, 0)
    }
}

struct Shared {
    nodes: Vec<Node>,
    top: Vec<usize>,
    not_found: Option<PageFn>,
    forbidden: Option<PageFn>,
}

#[derive(Clone, Debug, PartialEq)]
enum Resolution {
    Matched {
        ids: Vec<usize>,
        params: Params,
    },
    NotFound,
    Forbidden,
    /// A guard sent the navigation elsewhere; the router replaces the location with this one.
    Redirected(Location),
}

impl Shared {
    fn resolve(&self, location: &Location) -> Resolution {
        let mut location = location.clone();
        let mut redirected = false;
        for _ in 0..=MAX_REDIRECTS {
            let segments = location.segments();
            let Some((ids, params)) = self.find(&self.top, &segments) else {
                return if redirected {
                    Resolution::Redirected(location)
                } else {
                    Resolution::NotFound
                };
            };
            let params = Params(params);
            let visit = Visit {
                location: location.clone(),
                params: params.clone(),
            };
            let verdict = ids
                .iter()
                .filter_map(|id| self.nodes[*id].guard.as_ref())
                .map(|guard| guard(&visit))
                .find(|v| *v != Guard::Allow)
                .unwrap_or(Guard::Allow);
            match verdict {
                Guard::Allow if redirected => return Resolution::Redirected(location),
                Guard::Allow => return Resolution::Matched { ids, params },
                Guard::Deny => return Resolution::Forbidden,
                Guard::Redirect(url) => {
                    location = Location::parse(&url);
                    redirected = true;
                }
            }
        }
        Resolution::NotFound
    }

    fn find(&self, ids: &[usize], segments: &[String]) -> Option<Matched> {
        for &id in ids {
            let node = &self.nodes[id];
            let Some((used, mut params)) = node.pattern.matches(segments) else {
                continue;
            };
            let rest = &segments[used..];
            match &node.kind {
                Kind::Page(_) if rest.is_empty() => return Some((vec![id], params)),
                Kind::Page(_) => {}
                Kind::Layout(_, children) => {
                    if let Some((mut chain, inner)) = self.find(children, rest) {
                        chain.insert(0, id);
                        params.extend(inner);
                        return Some((chain, params));
                    }
                }
            }
        }
        None
    }
}

/// What a route's view can ask about the navigation that led to it.
#[derive(Clone)]
pub struct RouteContext {
    router: Router,
    params: Memo<Params>,
}

impl RouteContext {
    /// The URL parameter `name` (`:name` or `*name` in the pattern). Reading it inside a view
    /// closure makes that closure follow navigation between URLs of the same route.
    #[must_use]
    pub fn param(&self, name: &str) -> Option<String> {
        self.params.with(|p| p.get(name).map(str::to_string))
    }

    /// The current location.
    #[must_use]
    pub fn location(&self) -> Location {
        self.router.location()
    }

    #[must_use]
    pub const fn router(&self) -> &Router {
        &self.router
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Slot {
    Empty,
    Route(usize),
    NotFound,
    Forbidden,
}

fn level(
    shared: &Rc<Shared>,
    resolution: &Memo<Resolution>,
    ctx: &RouteContext,
    depth: usize,
) -> View {
    let slot = {
        let resolution = resolution.clone();
        create_memo(move || match resolution.get() {
            Resolution::Matched { ids, .. } => {
                ids.get(depth).map_or(Slot::Empty, |id| Slot::Route(*id))
            }
            Resolution::NotFound if depth == 0 => Slot::NotFound,
            Resolution::Forbidden if depth == 0 => Slot::Forbidden,
            _ => Slot::Empty,
        })
    };
    let (shared, resolution, ctx) = (shared.clone(), resolution.clone(), ctx.clone());
    View::dynamic(move || {
        if depth == 0 {
            ctx.router
                .0
                .not_found
                .set(slot.get_untracked() == Slot::NotFound);
        }
        let page = |f: &Option<PageFn>, fallback: &str| {
            f.as_ref()
                .map_or_else(|| View::text(fallback), |f| untrack(|| f(&ctx)))
        };
        match slot.get() {
            Slot::Empty => View::Empty,
            Slot::NotFound => page(&shared.not_found, "Not found"),
            Slot::Forbidden => page(&shared.forbidden, "Forbidden"),
            Slot::Route(id) => match &shared.nodes[id].kind {
                Kind::Page(f) => untrack(|| f(&ctx)),
                Kind::Layout(wrap, _) => {
                    let inner = level(&shared, &resolution, &ctx, depth + 1);
                    untrack(|| wrap(&ctx, inner))
                }
            },
        }
    })
}

/// Where URLs come from and go to: the browser's history, an in-memory one for tests and for
/// rendering on the server.
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
    fn subscribe(&self, listener: Listener);
}

/// A history that lives in memory.
pub struct MemoryHistory {
    entries: RefCell<Vec<Location>>,
    index: Cell<usize>,
    listeners: RefCell<Vec<Listener>>,
}

impl MemoryHistory {
    #[must_use]
    pub fn new(url: &str) -> Self {
        Self {
            entries: RefCell::new(vec![Location::parse(url)]),
            index: Cell::new(0),
            listeners: RefCell::new(Vec::new()),
        }
    }

    /// Every entry, oldest first.
    #[must_use]
    pub fn entries(&self) -> Vec<Location> {
        self.entries.borrow().clone()
    }

    fn go(&self, index: usize) {
        self.index.set(index);
        let location = self.location();
        let listeners = self.listeners.borrow().clone();
        for listener in listeners {
            listener(location.clone());
        }
    }
}

impl History for MemoryHistory {
    fn location(&self) -> Location {
        self.entries.borrow()[self.index.get()].clone()
    }

    fn push(&self, location: &Location) {
        let mut entries = self.entries.borrow_mut();
        entries.truncate(self.index.get() + 1);
        entries.push(location.clone());
        self.index.set(entries.len() - 1);
    }

    fn replace(&self, location: &Location) {
        self.entries.borrow_mut()[self.index.get()] = location.clone();
    }

    fn back(&self) -> bool {
        let index = self.index.get();
        if index == 0 {
            return false;
        }
        self.go(index - 1);
        true
    }

    fn forward(&self) -> bool {
        let index = self.index.get();
        if index + 1 >= self.entries.borrow().len() {
            return false;
        }
        self.go(index + 1);
        true
    }

    fn subscribe(&self, listener: Listener) {
        self.listeners.borrow_mut().push(listener);
    }
}

type BackHandler = Rc<dyn Fn() -> bool>;

struct RouterInner {
    history: Rc<dyn History>,
    location: Signal<Location>,
    back_handlers: RefCell<Vec<(u64, BackHandler)>>,
    next_handler: Cell<u64>,
    not_found: Cell<bool>,
}

/// The current location and the means to change it. Cheap to clone.
#[derive(Clone)]
pub struct Router(Rc<RouterInner>);

impl std::fmt::Debug for Router {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Router({})", self.0.location.get_untracked().url())
    }
}

impl Router {
    #[must_use]
    pub fn new(history: Rc<dyn History>) -> Self {
        let location = create_signal(history.location());
        history.subscribe({
            let location = location.clone();
            Rc::new(move |l| location.set_if_changed(l))
        });
        Self(Rc::new(RouterInner {
            history,
            location,
            back_handlers: RefCell::new(Vec::new()),
            next_handler: Cell::new(0),
            not_found: Cell::new(false),
        }))
    }

    /// Makes this router available to [`use_router`] and [`link`] under the current scope.
    pub fn provide(&self) {
        provide_context(self.clone());
    }

    /// The current location; reading it in a computation follows navigation.
    #[must_use]
    pub fn location(&self) -> Location {
        self.0.location.get()
    }

    /// Whether the last route shown was the not-found page, for a server to answer with a 404.
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        self.0.not_found.get()
    }

    /// Goes to `url`, adding a history entry (or replacing the current one when it is the same
    /// URL, so a repeated click does not pile up entries).
    pub fn navigate(&self, url: &str) {
        let location = Location::parse(url);
        if location == self.0.location.get_untracked() {
            self.0.history.replace(&location);
        } else {
            self.0.history.push(&location);
        }
        self.0.location.set_if_changed(location);
    }

    /// Goes to `url` in place of the current history entry.
    pub fn replace(&self, url: &str) {
        let location = Location::parse(url);
        self.0.history.replace(&location);
        self.0.location.set_if_changed(location);
    }

    #[must_use]
    pub fn back(&self) -> bool {
        self.0.history.back()
    }

    #[must_use]
    pub fn forward(&self) -> bool {
        self.0.history.forward()
    }

    /// Registers a handler for the back action, for hosts with a back button (an Android
    /// activity). It returns true when it used the action up, such as closing a sheet; handlers
    /// run newest first. It is removed when the current scope is disposed.
    pub fn on_back(&self, handler: impl Fn() -> bool + 'static) {
        let id = self.0.next_handler.get();
        self.0.next_handler.set(id + 1);
        self.0
            .back_handlers
            .borrow_mut()
            .push((id, Rc::new(handler)));
        let router = self.clone();
        on_cleanup(move || {
            router
                .0
                .back_handlers
                .borrow_mut()
                .retain(|(h, _)| *h != id);
        });
    }

    /// The host's back button was pressed: lets the `on_back` handlers use it, then goes back in
    /// the history. False when nothing was left to do, so the host should leave the app.
    #[must_use]
    pub fn handle_back(&self) -> bool {
        let handlers: Vec<BackHandler> = self
            .0
            .back_handlers
            .borrow()
            .iter()
            .rev()
            .map(|(_, h)| h.clone())
            .collect();
        handlers.iter().any(|h| h()) || self.back()
    }
}

/// The router provided by [`Router::provide`] or [`Routes::view`].
///
/// # Panics
/// When no router was provided.
#[must_use]
pub fn use_router() -> Router {
    use_context::<Router>().expect("use_router needs a router: call Router::provide first")
}

/// A link to `href`.
///
/// A relative `href` is followed without a page load (a plain click; ctrl-click and the like still
/// open a tab), and the link carries `aria-current="page"` while it points at the current path.
/// Anything else is an ordinary link.
///
/// # Panics
/// When no router was provided and `href` is relative.
#[must_use]
pub fn link(href: &str) -> Element {
    let element = a().attr("href", href);
    if !href.starts_with('/') || href.starts_with("//") {
        return element;
    }
    let router = use_router();
    let target = Location::parse(href).path;
    let current = router.clone();
    element
        .attr_opt("aria-current", move || {
            (current.location().path == target).then(|| "page".to_string())
        })
        .intercept_click()
        .on_click({
            let href = href.to_string();
            move || router.navigate(&href)
        })
}

macro_rules! opaque_debug {
    ($($t:ident),*) => {$(
        impl std::fmt::Debug for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($t)).finish_non_exhaustive()
            }
        }
    )*};
}

opaque_debug!(Routes, Layout, RouteContext, MemoryHistory);
