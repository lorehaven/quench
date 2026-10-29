//! The functions `quench.js` calls, and the buffer rules behind them.
//!
//! Text and events cross the boundary as `(address, length)` pairs in linear memory. The buffers are
//! ordinary `Vec`s kept here by address: [`q_alloc`] makes one and tells JS where it is, JS fills it, and
//! the export it is handed to takes it back by that address. An address is only a number to this
//! module, so nothing in it reads or writes through a raw pointer. The one thing the compiler asks to be
//! called unsafe is the `no_mangle` that makes each function an export.
#![allow(unsafe_code)] // `#[unsafe(no_mangle)]`: the export attribute is the only unsafe left here

use crate::browser;
use crate::{dom, events, exec, fetch, history, sse, state};
use std::cell::RefCell;
use std::collections::BTreeMap;

thread_local! {
    /// The buffers JS has been given and not yet handed back, by address.
    static BUFFERS: RefCell<BTreeMap<usize, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
}

/// Room for `len` bytes that JS fills in and hands to one export. Returns where it is (0 for none).
/// The export takes ownership.
#[must_use]
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn q_alloc(len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let buffer = vec![0_u8; len];
    let address = buffer.as_ptr().addr();
    BUFFERS.with(|b| b.borrow_mut().insert(address, buffer));
    address
}

/// Frees a buffer from [`q_alloc`] that was not handed to an export. An address that is not one is
/// ignored.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn q_dealloc(address: usize, len: usize) {
    drop(take(address, len));
}

/// Takes the buffer at `address` back, cut to `len` bytes. A zero `len`, or an address that is not
/// a buffer, means "no data".
fn take(address: usize, len: usize) -> Vec<u8> {
    if len == 0 {
        BUFFERS.with(|b| b.borrow_mut().remove(&address));
        return Vec::new();
    }
    let mut bytes = BUFFERS
        .with(|b| b.borrow_mut().remove(&address))
        .unwrap_or_default();
    bytes.truncate(len);
    bytes
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A DOM event happened on node `handle`. `kind` selects the event (see `events::decode`). Returns 1
/// when a handler asked for `preventDefault`.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
#[allow(clippy::too_many_arguments, clippy::must_use_candidate)] // the shape fixed by the contract
pub extern "C" fn q_dispatch_event(
    handle: u32,
    kind: u32,
    address: usize,
    len: usize,
    a: f64,
    b: f64,
    c: i32,
    flags: u32,
) -> u32 {
    let payload = lossy(&take(address, len));
    let _ = quench_ui::take_prevent_request();
    if let Some(event) = events::decode(kind, payload, a, b, c, flags) {
        dom::dispatch(handle, &event);
    }
    u32::from(quench_ui::take_prevent_request())
}

/// The state the server embedded in the page, read before `q_main`.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn q_set_state(address: usize, len: usize) {
    state::set(lossy(&take(address, len)));
}

/// What the page knows of its browser (`{"time_zone": "...", "storage": {key: value}}`), read before `q_main`.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn q_set_env(address: usize, len: usize) {
    browser::set_env(&lossy(&take(address, len)));
}

/// The URL (path, query and hash) the page was loaded at, read before `q_main`.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn q_set_location(address: usize, len: usize) {
    history::set_initial(&lossy(&take(address, len)));
}

/// The browser moved through its history (back, forward): the URL it is at now.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn q_popstate(address: usize, len: usize) {
    history::popstate(&lossy(&take(address, len)));
}

/// Something happened to an event source opened with `q_sse_open`. Both buffers are consumed.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn q_sse_event(
    id: u32,
    kind: u32,
    name_address: usize,
    name_len: usize,
    data_address: usize,
    data_len: usize,
) {
    let name = lossy(&take(name_address, name_len));
    let data = lossy(&take(data_address, data_len));
    sse::event(id, kind, name, data);
}

/// Polls every task that is ready.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn q_tick() {
    exec::tick();
}

/// A timer set with `q_set_timeout` fired.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn q_timer_fire(id: u32) {
    exec::timer_fired(id);
}

/// A fetch started with `q_fetch` finished (`status` 0: it failed or was aborted, and the body is
/// a message).
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn q_fetch_done(
    id: u32,
    status: u32,
    body_address: usize,
    body_len: usize,
    headers_address: usize,
    headers_len: usize,
) {
    let body = take(body_address, body_len);
    let headers = lossy(&take(headers_address, headers_len));
    fetch::completed(id, status, body, &headers);
}

/// A copy of `bytes` in a fresh `q_alloc` buffer, as `quench.js` would make: `(address, length)`.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn alloc_copy(bytes: &[u8]) -> (usize, usize) {
    let address = q_alloc(bytes.len());
    BUFFERS.with(|b| {
        if let Some(buffer) = b.borrow_mut().get_mut(&address) {
            buffer.copy_from_slice(bytes);
        }
    });
    (address, bytes.len())
}
