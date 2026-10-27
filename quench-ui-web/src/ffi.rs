//! The functions `quench.js` calls, and the buffer rules behind them.
#![allow(unsafe_code)] // raw buffers cross the wasm boundary as (pointer, length) pairs

use crate::{dom, events, exec, fetch, state};

/// Room for `len` bytes that JS fills in and hands to one export. The export takes ownership.
#[must_use]
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn q_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::NonNull::dangling().as_ptr();
    }
    Box::into_raw(vec![0_u8; len].into_boxed_slice()).cast::<u8>()
}

/// Frees a buffer from [`q_alloc`] that was not passed to an export.
///
/// # Safety
/// `ptr` and `len` must be exactly what `q_alloc(len)` returned and `len`, and not be used again.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub unsafe extern "C" fn q_dealloc(ptr: *mut u8, len: usize) {
    drop(unsafe { take(ptr, len) });
}

/// Takes ownership of a buffer from [`q_alloc`]. A zero `len` means "no data".
///
/// # Safety
/// As for [`q_dealloc`].
unsafe fn take(ptr: *mut u8, len: usize) -> Vec<u8> {
    if len == 0 || ptr.is_null() {
        return Vec::new();
    }
    let slice = std::ptr::slice_from_raw_parts_mut(ptr, len);
    unsafe { Box::from_raw(slice) }.into_vec()
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A listener fired on the node `handle`. `ptr`/`len` carry the text payload (the value of an
/// input, the key of a key event).
///
/// # Safety
/// `ptr` and `len` must be a buffer from `q_alloc`, or `len` must be 0.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
#[allow(clippy::too_many_arguments)] // the shape fixed by the contract
pub unsafe extern "C" fn q_dispatch_event(
    handle: u32,
    kind: u32,
    ptr: *mut u8,
    len: usize,
    a: f64,
    b: f64,
    c: i32,
    flags: u32,
) {
    let payload = lossy(&unsafe { take(ptr, len) });
    if let Some(event) = events::decode(kind, payload, a, b, c, flags) {
        dom::dispatch(handle, &event);
    }
}

/// The JSON the server embedded in the page for the app, read before `q_main`.
///
/// # Safety
/// `ptr` and `len` must be a buffer from `q_alloc`, or `len` must be 0.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub unsafe extern "C" fn q_set_state(ptr: *mut u8, len: usize) {
    state::set(lossy(&unsafe { take(ptr, len) }));
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
///
/// # Safety
/// Each `(ptr, len)` pair must be a buffer from `q_alloc`, or have `len` 0.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub unsafe extern "C" fn q_fetch_done(
    id: u32,
    status: u32,
    body_ptr: *mut u8,
    body_len: usize,
    headers_ptr: *mut u8,
    headers_len: usize,
) {
    let body = unsafe { take(body_ptr, body_len) };
    let headers = lossy(&unsafe { take(headers_ptr, headers_len) });
    fetch::completed(id, status, body, &headers);
}

/// A copy of `bytes` in a fresh `q_alloc` buffer, as `quench.js` would make.
#[cfg(not(target_arch = "wasm32"))]
pub fn alloc_copy(bytes: &[u8]) -> (*mut u8, usize) {
    let ptr = q_alloc(bytes.len());
    if !bytes.is_empty() {
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len()) };
    }
    (ptr, bytes.len())
}
