//! An in-memory stand-in for `quench.js`, used on every target but `wasm32`.
//!
//! It keeps what the real JS side keeps: a handle table with a free list in front of a DOM that
//! outlives the handles (releasing a handle does not detach its node). Tests drive it as the
//! browser would: fire an event, a timer, a fetch completion, a tick.
#![allow(clippy::must_use_candidate)] // the `fire_*` helpers are called for their effect

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
    ticks: usize,
    logs: Vec<(u32, String)>,
    server_content: bool,
    mismatches: u32,
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
            ticks: 0,
            logs: Vec::new(),
            server_content: false,
            mismatches: 0,
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

/// Forgets everything and starts again with just the root. Call at the start of a test.
pub fn reset() {
    with(|s| *s = State::new());
    crate::reset_runtime();
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
    // SAFETY: `(ptr, len)` was just produced by `alloc_copy`, and is passed to one export.
    #[allow(unsafe_code)]
    unsafe {
        ffi::q_dispatch_event(handle, kind, ptr, len, a, b, c, flags);
    }
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
        // SAFETY: both buffers were just produced by `alloc_copy`, each passed to one export.
        #[allow(unsafe_code)]
        unsafe {
            ffi::q_fetch_done(id, status, bp, bl, hp, hl);
        }
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
            // SAFETY: the buffer was just produced by `alloc_copy`; a zero-length headers argument
            // means "no data" and its pointer is ignored.
            #[allow(unsafe_code)]
            unsafe {
                ffi::q_fetch_done(id, 0, bp, bl, std::ptr::null_mut(), 0);
            }
        }
        if ticked {
            ffi::q_tick();
        }
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
        with(|s| {
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
        with(|s| {
            s.calls.push(Call::ClearTimeout(id));
            s.timers.remove(&id);
        });
    }

    pub fn fetch(id: u32, method: &str, url: &str, headers: &str, body: &[u8]) {
        with(|s| {
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
        with(|s| {
            s.calls.push(Call::FetchAbort(id));
            if s.fetches.remove(&id).is_some() {
                s.aborts.push(id);
            }
        });
    }

    pub fn schedule_tick() {
        with(|s| s.ticks += 1);
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
}
