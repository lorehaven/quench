//! An in-memory stand-in for `quench.js`, used on every target but `wasm32`.
//!
//! It keeps what the real JS side keeps: a handle table with a free list in front of a DOM that
//! outlives the handles (releasing a handle does not detach its node). Tests drive it as the
//! browser would: fire an event, a timer, a fetch completion, a tick.
#![allow(clippy::must_use_candidate)] // the `fire_*` helpers are called for their effect

pub use crate::ffi::alloc_copy;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;

use quench_ui::html::{escape_attr, escape_text, is_void};

use crate::ffi;

/// A call the Rust side made into the host, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    CreateElement(u32, String),
    CreateText(u32, String),
    CreateAnchor(u32),
    SetText(u32, String),
    SetAttr(u32, String, String),
    RemoveAttr(u32, String),
    Insert {
        parent: u32,
        child: u32,
        before: u32,
    },
    Remove {
        parent: u32,
        child: u32,
    },
    Listen(u32, u32),
    Release(u32),
    SetTimeout(u32, u32),
    ClearTimeout(u32),
    Fetch(u32),
    FetchAbort(u32),
    HydrateBegin,
    HydrateEnd,
    SseOpen(u32),
    SseClose(u32),
    Prevent(u32, u32, u32),
    SetStyle(u32, String, String),
    Focus(u32),
    TrapFocus(u32),
    History(u32, String),
}

/// A fetch the Rust side started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchCall {
    pub id: u32,
    pub method: String,
    pub url: String,
    pub headers: String,
    pub body: Vec<u8>,
}

enum Kind {
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
    },
    Text(String),
    Anchor,
}

struct DomNode {
    kind: Kind,
    parent: Option<usize>,
    children: Vec<usize>,
}

struct State {
    dom: Vec<DomNode>,
    table: Vec<Option<usize>>,
    free: Vec<u32>,
    listeners: BTreeMap<u32, Vec<u32>>,
    calls: Vec<Call>,
    timers: BTreeMap<u32, u32>,
    fetches: BTreeMap<u32, FetchCall>,
    aborts: Vec<u32>,
    sse: BTreeMap<u32, (String, Vec<String>)>,
    ticks: usize,
    logs: Vec<(u32, String)>,
    server_content: bool,
    mismatches: u32,
    /// The browser's session history for `history()`: URLs and the current index.
    entries: Vec<String>,
    entry: usize,
    styles: BTreeMap<(usize, String), String>,
    now: f64,
    wall_ms: i64,
    /// What `q_storage_set` was called with, newest last (an empty value forgets the key).
    storage_sets: Vec<(String, String)>,
    reduced_motion: bool,
    bridge_sent: Vec<String>,
}

impl State {
    fn new() -> Self {
        Self {
            dom: vec![DomNode {
                kind: Kind::Element {
                    tag: "#app".to_string(),
                    attrs: Vec::new(),
                },
                parent: None,
                children: Vec::new(),
            }],
            table: vec![None, Some(0)],
            free: Vec::new(),
            listeners: BTreeMap::new(),
            calls: Vec::new(),
            timers: BTreeMap::new(),
            fetches: BTreeMap::new(),
            aborts: Vec::new(),
            sse: BTreeMap::new(),
            ticks: 0,
            logs: Vec::new(),
            server_content: false,
            mismatches: 0,
            entries: vec!["/".to_string()],
            entry: 0,
            styles: BTreeMap::new(),
            now: 0.0,
            wall_ms: 0,
            storage_sets: Vec::new(),
            reduced_motion: false,
            bridge_sent: Vec::new(),
        }
    }

    fn node(&self, handle: u32) -> Option<usize> {
        self.table.get(handle as usize).copied().flatten()
    }

    fn create(&mut self, kind: Kind) -> u32 {
        self.dom.push(DomNode {
            kind,
            parent: None,
            children: Vec::new(),
        });
        let index = self.dom.len() - 1;
        if let Some(h) = self.free.pop() {
            self.table[h as usize] = Some(index);
            h
        } else {
            self.table.push(Some(index));
            u32::try_from(self.table.len() - 1).expect("handle table fits u32")
        }
    }

    fn detach(&mut self, child: usize) {
        if let Some(parent) = self.dom[child].parent.take() {
            self.dom[parent].children.retain(|c| *c != child);
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::new());
}

fn with<T>(f: impl FnOnce(&mut State) -> T) -> T {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// For calls made by `Drop`, which can run while the thread is being torn down.
fn try_with(f: impl FnOnce(&mut State)) {
    let _ = STATE.try_with(|s| f(&mut s.borrow_mut()));
}

/// Forgets everything and starts again with just the root. Call at the start of a test.
pub fn reset() {
    // The runtime first: dropping what the last test left (an open event source) calls the host, and
    // that must not land in the state the next test starts with.
    crate::reset_runtime();
    crate::browser::reset();
    with(|s| *s = State::new());
}

/// The calls the Rust side made, oldest first.
#[must_use]
pub fn calls() -> Vec<Call> {
    with(|s| s.calls.clone())
}

pub fn clear_calls() {
    with(|s| s.calls.clear());
}

/// Handles in use, not counting the root. After a view is unmounted this is back to 0.
#[must_use]
pub fn live_handles() -> usize {
    with(|s| s.table.iter().skip(2).filter(|e| e.is_some()).count())
}

/// Listeners the host currently has registered.
#[must_use]
pub fn live_listeners() -> usize {
    with(|s| s.listeners.values().map(Vec::len).sum())
}

/// Handles of the elements with this tag that are still in the table, ascending.
#[must_use]
pub fn handles_by_tag(tag: &str) -> Vec<u32> {
    with(|s| {
        (2..s.table.len())
            .filter(|h| {
                s.table[*h].is_some_and(
                    |i| matches!(&s.dom[i].kind, Kind::Element { tag: t, .. } if t == tag),
                )
            })
            .map(|h| u32::try_from(h).expect("handle fits u32"))
            .collect()
    })
}

/// The attribute of the element behind a handle.
#[must_use]
pub fn attr(handle: u32, name: &str) -> Option<String> {
    with(|s| {
        let index = s.node(handle)?;
        match &s.dom[index].kind {
            Kind::Element { attrs, .. } => attrs
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.clone()),
            _ => None,
        }
    })
}

fn write_html(s: &State, index: usize, out: &mut String) {
    match &s.dom[index].kind {
        Kind::Text(t) => out.push_str(&escape_text(t)),
        Kind::Anchor => out.push_str("<!---->"),
        Kind::Element { tag, attrs } => {
            out.push('<');
            out.push_str(tag);
            for (n, v) in attrs {
                let _ = write!(out, " {n}=\"{}\"", escape_attr(v));
            }
            out.push('>');
            if !is_void(tag) {
                for c in &s.dom[index].children {
                    write_html(s, *c, out);
                }
                let _ = write!(out, "</{tag}>");
            }
        }
    }
}

/// The content of the root as HTML; dynamic-region anchors appear as `<!---->`.
#[must_use]
pub fn html() -> String {
    with(|s| {
        let mut out = String::new();
        for c in &s.dom[0].children {
            write_html(s, *c, &mut out);
        }
        out
    })
}

/// All text under the root.
#[must_use]
pub fn text() -> String {
    fn walk(s: &State, index: usize, out: &mut String) {
        if let Kind::Text(t) = &s.dom[index].kind {
            out.push_str(t);
        }
        for c in &s.dom[index].children {
            walk(s, *c, out);
        }
    }
    with(|s| {
        let mut out = String::new();
        walk(s, 0, &mut out);
        out
    })
}

/// Whether the host has a listener of this kind on the handle.
#[must_use]
pub fn has_listener(handle: u32, kind: u32) -> bool {
    with(|s| s.listeners.get(&handle).is_some_and(|k| k.contains(&kind)))
}

/// Delivers an event as `quench.js` would, if a listener for it is registered. Returns whether
/// one was.
pub fn fire(handle: u32, kind: u32, payload: &str, a: f64, b: f64, c: i32, flags: u32) -> bool {
    if !has_listener(handle, kind) {
        return false;
    }
    let (ptr, len) = ffi::alloc_copy(payload.as_bytes());
    ffi::q_dispatch_event(handle, kind, ptr, len, a, b, c, flags);
    true
}

pub fn fire_click(handle: u32) -> bool {
    fire(handle, 0, "", 0.0, 0.0, 0, 0)
}

pub fn fire_input(handle: u32, value: &str) -> bool {
    fire(handle, 2, value, 0.0, 0.0, 0, 0)
}

/// Timers waiting to fire: `(id, ms)`.
#[must_use]
pub fn timers() -> Vec<(u32, u32)> {
    with(|s| s.timers.iter().map(|(i, m)| (*i, *m)).collect())
}

/// Fires a timer as the browser would, then runs the tick it asks for.
pub fn fire_timer(id: u32) -> bool {
    let known = with(|s| s.timers.remove(&id).is_some());
    if known {
        ffi::q_timer_fire(id);
        run_ticks();
    }
    known
}

/// Fetches started and not yet completed or aborted, oldest id first.
#[must_use]
pub fn fetches() -> Vec<FetchCall> {
    with(|s| s.fetches.values().cloned().collect())
}

/// Completes a fetch as the browser would, then runs the tick it asks for.
pub fn complete_fetch(id: u32, status: u32, body: &[u8], headers: &str) -> bool {
    let known = with(|s| s.fetches.remove(&id).is_some());
    if known {
        let (bp, bl) = ffi::alloc_copy(body);
        let (hp, hl) = ffi::alloc_copy(headers.as_bytes());
        ffi::q_fetch_done(id, status, bp, bl, hp, hl);
        run_ticks();
    }
    known
}

/// Does what the browser does after a call returns to it.
///
/// Delivers the completions of aborted fetches (status 0) and runs the tick the Rust side asked
/// for with `q_schedule_tick`, until nothing is left. Returns whether anything ran.
pub fn run_ticks() -> bool {
    let mut ran = false;
    loop {
        let aborts = with(|s| std::mem::take(&mut s.aborts));
        let ticked = with(|s| std::mem::take(&mut s.ticks) > 0);
        if aborts.is_empty() && !ticked {
            return ran;
        }
        ran = true;
        for id in aborts {
            let (bp, bl) = ffi::alloc_copy(b"aborted");
            ffi::q_fetch_done(id, 0, bp, bl, 0, 0);
        }
        if ticked {
            ffi::q_tick();
        }
    }
}

/// The envelopes the app sent to the host, oldest first. A bridge call is a fetch of
/// `quench-bridge:call`, so it is also in [`fetches`] until it is answered.
#[must_use]
pub fn bridge_sent() -> Vec<String> {
    with(|s| s.bridge_sent.clone())
}

/// The id of the envelope in `body`.
fn envelope_id(body: &[u8]) -> Option<u64> {
    let value: quench_serde::Value =
        quench_serde::json::from_str(std::str::from_utf8(body).ok()?).ok()?;
    value.get("id").and_then(quench_serde::Value::as_u64)
}

/// Plays the host: delivers one message as the message port would.
///
/// An answer completes the fetch of
/// the call with that id; an event goes to the app's event source on the bridge. A message that is
/// neither, or answers nothing, is ignored.
pub fn bridge_deliver(text: &str) {
    run_ticks(); // the app may have just asked for its event source
    let Ok(value) = quench_serde::json::from_str::<quench_serde::Value>(text) else {
        return;
    };
    if value.get("event").is_some() {
        if let Some((id, _, _)) = sse_sources()
            .into_iter()
            .find(|(_, url, _)| url == "quench-bridge:events")
        {
            sse_emit(id, 1, "event", text);
        }
        return;
    }
    let Some(id) = value.get("id").and_then(quench_serde::Value::as_u64) else {
        return;
    };
    let call = fetches()
        .into_iter()
        .find(|f| f.url == "quench-bridge:call" && envelope_id(&f.body) == Some(id));
    if let Some(call) = call {
        complete_fetch(call.id, 200, text.as_bytes(), "");
    }
}

/// Plays a page that has no host: every call waiting for one fails, as `quench.js` fails it.
pub fn bridge_unavailable() {
    for call in fetches()
        .into_iter()
        .filter(|f| f.url == "quench-bridge:call")
    {
        complete_fetch(call.id, 0, b"this page is not shown by a host app", "");
    }
}

/// Pretends the page arrived with server-rendered content in the root (or not).
///
/// `mismatches` is what hydration will report. The claiming itself is done by `quench.js`; the real
/// thing is exercised by the browser test.
pub fn set_server_content(present: bool, mismatches: u32) {
    with(|s| {
        s.server_content = present;
        s.mismatches = mismatches;
    });
}

/// The event sources the Rust side has open: `(id, url, event names)`.
#[must_use]
pub fn sse_sources() -> Vec<(u32, String, Vec<String>)> {
    with(|s| {
        s.sse
            .iter()
            .map(|(i, (u, n))| (*i, u.clone(), n.clone()))
            .collect()
    })
}

/// What the browser reports about an event source (see the contract).
///
/// `kind` is 0 opened, 1 an event, 2 reconnecting, 3 failed for good. An event is delivered only if
/// the source is open and listens for that name (`message` always). Returns whether anything was
/// delivered.
pub fn sse_emit(id: u32, kind: u32, name: &str, data: &str) -> bool {
    let listening = with(|s| {
        s.sse.get(&id).is_some_and(|(_, names)| {
            kind != 1 || name == "message" || names.iter().any(|n| n == name)
        })
    });
    if !listening {
        return false;
    }
    let (np, nl) = ffi::alloc_copy(name.as_bytes());
    let (dp, dl) = ffi::alloc_copy(data.as_bytes());
    ffi::q_sse_event(id, kind, np, nl, dp, dl);
    run_ticks();
    true
}

/// The CSS property set on the handle's node through the style object.
#[must_use]
pub fn style(handle: u32, property: &str) -> Option<String> {
    with(|s| {
        let i = s.node(handle)?;
        s.styles.get(&(i, property.to_string())).cloned()
    })
}

/// Sets the clock `q_now` reads, in milliseconds.
pub fn set_now(ms: f64) {
    with(|s| s.now = ms);
}

/// Sets the wall clock `q_wall_ms` reads, in milliseconds since 1970.
pub fn set_wall_ms(ms: i64) {
    with(|s| s.wall_ms = ms);
}

/// What the module asked the page to keep, in order: `(key, value)`, an empty value meaning forget.
#[must_use]
pub fn storage_sets() -> Vec<(String, String)> {
    with(|s| s.storage_sets.clone())
}

/// Sets what `prefers-reduced-motion` reports.
pub fn set_reduced_motion(on: bool) {
    with(|s| s.reduced_motion = on);
}

/// The URL the page was loaded at, as `quench.js` reports it before `q_main`.
pub fn set_location(url: &str) {
    let (ptr, len) = ffi::alloc_copy(url.as_bytes());
    ffi::q_set_location(ptr, len);
}

/// The browser went to `url` through its history (the back or forward button).
pub fn popstate(url: &str) {
    let (ptr, len) = ffi::alloc_copy(url.as_bytes());
    ffi::q_popstate(ptr, len);
}

/// The browser's session history: `(entries, index of the current one)`.
#[must_use]
pub fn history_entries() -> (Vec<String>, usize) {
    with(|s| (s.entries.clone(), s.entry))
}

/// What `q_log` received: `(level, message)`.
#[must_use]
pub fn logs() -> Vec<(u32, String)> {
    with(|s| s.logs.clone())
}

/// The host calls themselves, as the rest of the crate sees them.
pub(crate) mod imports {
    use super::{Call, FetchCall, Kind, with};

    pub fn create_element(tag: &str) -> u32 {
        with(|s| {
            let h = s.create(Kind::Element {
                tag: tag.to_string(),
                attrs: Vec::new(),
            });
            s.calls.push(Call::CreateElement(h, tag.to_string()));
            h
        })
    }

    pub fn create_text(text: &str) -> u32 {
        with(|s| {
            let h = s.create(Kind::Text(text.to_string()));
            s.calls.push(Call::CreateText(h, text.to_string()));
            h
        })
    }

    pub fn create_anchor() -> u32 {
        with(|s| {
            let h = s.create(Kind::Anchor);
            s.calls.push(Call::CreateAnchor(h));
            h
        })
    }

    pub fn set_text(h: u32, text: &str) {
        with(|s| {
            s.calls.push(Call::SetText(h, text.to_string()));
            if let Some(i) = s.node(h)
                && let Kind::Text(t) = &mut s.dom[i].kind
            {
                *t = text.to_string();
            }
        });
    }

    pub fn set_attr(h: u32, name: &str, value: &str) {
        with(|s| {
            s.calls
                .push(Call::SetAttr(h, name.to_string(), value.to_string()));
            if let Some(i) = s.node(h)
                && let Kind::Element { attrs, .. } = &mut s.dom[i].kind
            {
                if let Some(slot) = attrs.iter_mut().find(|(n, _)| n == name) {
                    slot.1 = value.to_string();
                } else {
                    attrs.push((name.to_string(), value.to_string()));
                }
            }
        });
    }

    pub fn remove_attr(h: u32, name: &str) {
        with(|s| {
            s.calls.push(Call::RemoveAttr(h, name.to_string()));
            if let Some(i) = s.node(h)
                && let Kind::Element { attrs, .. } = &mut s.dom[i].kind
            {
                attrs.retain(|(n, _)| n != name);
            }
        });
    }

    pub fn insert(parent: u32, child: u32, before: u32) {
        with(|s| {
            s.calls.push(Call::Insert {
                parent,
                child,
                before,
            });
            let (Some(p), Some(c)) = (s.node(parent), s.node(child)) else {
                return;
            };
            if p == c {
                return;
            }
            let b = if before == 0 { None } else { s.node(before) };
            s.detach(c);
            let at = b
                .and_then(|b| s.dom[p].children.iter().position(|x| *x == b))
                .unwrap_or(s.dom[p].children.len());
            s.dom[p].children.insert(at, c);
            s.dom[c].parent = Some(p);
        });
    }

    pub fn remove(parent: u32, child: u32) {
        with(|s| {
            s.calls.push(Call::Remove { parent, child });
            if let (Some(p), Some(c)) = (s.node(parent), s.node(child))
                && s.dom[c].parent == Some(p)
            {
                s.detach(c);
            }
        });
    }

    pub fn listen(h: u32, kind: u32) {
        with(|s| {
            s.calls.push(Call::Listen(h, kind));
            if s.node(h).is_some() {
                let kinds = s.listeners.entry(h).or_default();
                if !kinds.contains(&kind) {
                    kinds.push(kind);
                }
            }
        });
    }

    pub fn release(h: u32) {
        super::try_with(|s| {
            s.calls.push(Call::Release(h));
            if h >= 2 && s.node(h).is_some() {
                s.table[h as usize] = None;
                s.free.push(h);
                s.listeners.remove(&h);
            }
        });
    }

    pub fn log(level: u32, message: &str) {
        with(|s| s.logs.push((level, message.to_string())));
    }

    pub fn set_timeout(id: u32, ms: u32) {
        with(|s| {
            s.calls.push(Call::SetTimeout(id, ms));
            s.timers.insert(id, ms);
        });
    }

    pub fn clear_timeout(id: u32) {
        super::try_with(|s| {
            s.calls.push(Call::ClearTimeout(id));
            s.timers.remove(&id);
        });
    }

    pub fn fetch(id: u32, method: &str, url: &str, headers: &str, body: &[u8]) {
        with(|s| {
            if url == "quench-bridge:call" {
                s.bridge_sent
                    .push(String::from_utf8_lossy(body).into_owned());
            }
            s.calls.push(Call::Fetch(id));
            s.fetches.insert(
                id,
                FetchCall {
                    id,
                    method: method.to_string(),
                    url: url.to_string(),
                    headers: headers.to_string(),
                    body: body.to_vec(),
                },
            );
        });
    }

    /// Aborting a fetch makes the browser complete it with status 0, a little later: see
    /// [`super::run_ticks`].
    pub fn fetch_abort(id: u32) {
        super::try_with(|s| {
            s.calls.push(Call::FetchAbort(id));
            if s.fetches.remove(&id).is_some() {
                s.aborts.push(id);
            }
        });
    }

    pub fn schedule_tick() {
        with(|s| s.ticks += 1);
    }

    pub fn sse_open(id: u32, url: &str, names: &str) {
        super::try_with(|s| {
            s.calls.push(Call::SseOpen(id));
            let names = names
                .split('\n')
                .filter(|n| !n.is_empty())
                .map(str::to_string)
                .collect();
            s.sse.insert(id, (url.to_string(), names));
        });
    }

    pub fn sse_close(id: u32) {
        super::try_with(|s| {
            s.calls.push(Call::SseClose(id));
            s.sse.remove(&id);
        });
    }

    pub fn root_has_content() -> bool {
        with(|s| s.server_content)
    }

    pub fn hydrate_begin() {
        with(|s| s.calls.push(Call::HydrateBegin));
    }

    pub fn hydrate_end() -> u32 {
        with(|s| {
            s.calls.push(Call::HydrateEnd);
            s.mismatches
        })
    }

    pub fn prevent(h: u32, kind: u32, mode: u32) {
        with(|s| s.calls.push(Call::Prevent(h, kind, mode)));
    }

    pub fn set_style(h: u32, property: &str, value: &str) {
        with(|s| {
            s.calls
                .push(Call::SetStyle(h, property.to_string(), value.to_string()));
            if let Some(i) = s.node(h) {
                s.styles.retain(|(n, p), _| !(*n == i && p == property));
                if !value.is_empty() {
                    s.styles
                        .insert((i, property.to_string()), value.to_string());
                }
            }
        });
    }

    pub fn now() -> f64 {
        with(|s| s.now)
    }

    pub fn wall_ms() -> i64 {
        with(|s| s.wall_ms)
    }

    pub fn storage_set(key: &str, value: &str) {
        with(|s| s.storage_sets.push((key.to_string(), value.to_string())));
    }

    pub fn prefers_reduced_motion() -> bool {
        with(|s| s.reduced_motion)
    }

    pub fn focus(h: u32) {
        with(|s| s.calls.push(Call::Focus(h)));
    }

    pub fn trap_focus(h: u32) {
        with(|s| s.calls.push(Call::TrapFocus(h)));
    }

    /// The browser's session history: push and replace edit it; back and forward move in it and
    /// report the entry they land on, as a `popstate` would.
    pub fn history(op: u32, url: &str) {
        let landed = with(|s| {
            s.calls.push(Call::History(op, url.to_string()));
            match op {
                0 => {
                    s.entries.truncate(s.entry + 1);
                    s.entries.push(url.to_string());
                    s.entry += 1;
                    None
                }
                1 => {
                    let at = s.entry;
                    s.entries[at] = url.to_string();
                    None
                }
                2 if s.entry > 0 => {
                    s.entry -= 1;
                    Some(s.entries[s.entry].clone())
                }
                3 if s.entry + 1 < s.entries.len() => {
                    s.entry += 1;
                    Some(s.entries[s.entry].clone())
                }
                _ => None,
            }
        });
        if let Some(url) = landed {
            super::popstate(&url);
        }
    }
}
