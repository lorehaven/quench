//! The real imports: functions `quench.js` provides in the `"env"` module.
//!
//! Text crosses as `(address, length)`: the address of a live borrow, as a number. The imports are safe
//! to call because nothing is dereferenced on this side, and the contract (docs/quench-ui-web.md) has JS
//! copy the bytes before the import returns, while the borrow is still alive. The `unsafe extern` block
//! is the one thing the compiler asks to be called unsafe.
#![allow(unsafe_code)] // `unsafe extern`: how a wasm import is declared

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    safe fn q_create_el(tag_ptr: usize, tag_len: usize) -> u32;
    safe fn q_create_text(ptr: usize, len: usize) -> u32;
    safe fn q_create_anchor() -> u32;
    safe fn q_set_text(h: u32, ptr: usize, len: usize);
    safe fn q_set_attr(h: u32, name_ptr: usize, name_len: usize, val_ptr: usize, val_len: usize);
    safe fn q_remove_attr(h: u32, name_ptr: usize, name_len: usize);
    safe fn q_insert(parent: u32, child: u32, before: u32);
    safe fn q_remove(parent: u32, child: u32);
    safe fn q_listen(h: u32, kind: u32);
    safe fn q_release(h: u32);
    safe fn q_log(level: u32, ptr: usize, len: usize);
    safe fn q_set_timeout(id: u32, ms: u32);
    safe fn q_clear_timeout(id: u32);
    safe fn q_fetch(
        id: u32,
        method_ptr: usize,
        method_len: usize,
        url_ptr: usize,
        url_len: usize,
        headers_ptr: usize,
        headers_len: usize,
        body_ptr: usize,
        body_len: usize,
    );
    safe fn q_fetch_abort(id: u32);
    safe fn q_schedule_tick();
    safe fn q_sse_open(id: u32, url_ptr: usize, url_len: usize, names_ptr: usize, names_len: usize);
    safe fn q_sse_close(id: u32);
    safe fn q_root_has_content() -> u32;
    safe fn q_hydrate_begin();
    safe fn q_hydrate_end() -> u32;
    safe fn q_prevent(h: u32, kind: u32, mode: u32);
    safe fn q_set_style(h: u32, name_ptr: usize, name_len: usize, val_ptr: usize, val_len: usize);
    safe fn q_now() -> f64;
    safe fn q_wall_ms() -> f64;
    safe fn q_storage_set(key_ptr: usize, key_len: usize, val_ptr: usize, val_len: usize);
    safe fn q_reduced_motion() -> u32;
    safe fn q_focus(h: u32);
    safe fn q_trap_focus(h: u32);
    safe fn q_history(op: u32, ptr: usize, len: usize);
}

pub fn create_element(tag: &str) -> u32 {
    q_create_el(tag.as_ptr().addr(), tag.len())
}

pub fn create_text(text: &str) -> u32 {
    q_create_text(text.as_ptr().addr(), text.len())
}

pub fn create_anchor() -> u32 {
    q_create_anchor()
}

pub fn set_text(h: u32, text: &str) {
    q_set_text(h, text.as_ptr().addr(), text.len());
}

pub fn set_attr(h: u32, name: &str, value: &str) {
    q_set_attr(
        h,
        name.as_ptr().addr(),
        name.len(),
        value.as_ptr().addr(),
        value.len(),
    );
}

pub fn remove_attr(h: u32, name: &str) {
    q_remove_attr(h, name.as_ptr().addr(), name.len());
}

pub fn insert(parent: u32, child: u32, before: u32) {
    q_insert(parent, child, before);
}

pub fn remove(parent: u32, child: u32) {
    q_remove(parent, child);
}

pub fn listen(h: u32, kind: u32) {
    q_listen(h, kind);
}

pub fn release(h: u32) {
    q_release(h);
}

pub fn log(level: u32, message: &str) {
    q_log(level, message.as_ptr().addr(), message.len());
}

pub fn set_timeout(id: u32, ms: u32) {
    q_set_timeout(id, ms);
}

pub fn clear_timeout(id: u32) {
    q_clear_timeout(id);
}

pub fn fetch(id: u32, method: &str, url: &str, headers: &str, body: &[u8]) {
    q_fetch(
        id,
        method.as_ptr().addr(),
        method.len(),
        url.as_ptr().addr(),
        url.len(),
        headers.as_ptr().addr(),
        headers.len(),
        body.as_ptr().addr(),
        body.len(),
    );
}

pub fn fetch_abort(id: u32) {
    q_fetch_abort(id);
}

pub fn schedule_tick() {
    q_schedule_tick();
}

pub fn root_has_content() -> bool {
    q_root_has_content() != 0
}

pub fn hydrate_begin() {
    q_hydrate_begin();
}

/// Ends hydration and returns how many server nodes did not match what the app built.
pub fn hydrate_end() -> u32 {
    q_hydrate_end()
}

pub fn sse_open(id: u32, url: &str, names: &str) {
    q_sse_open(
        id,
        url.as_ptr().addr(),
        url.len(),
        names.as_ptr().addr(),
        names.len(),
    );
}

pub fn sse_close(id: u32) {
    q_sse_close(id);
}

pub fn prevent(h: u32, kind: u32, mode: u32) {
    q_prevent(h, kind, mode);
}

pub fn focus(h: u32) {
    q_focus(h);
}

pub fn trap_focus(h: u32) {
    q_trap_focus(h);
}

/// `op`: 0 push, 1 replace, 2 back, 3 forward. `url` is used by push and replace.
pub fn history(op: u32, url: &str) {
    q_history(op, url.as_ptr().addr(), url.len());
}

pub fn set_style(h: u32, name: &str, value: &str) {
    q_set_style(
        h,
        name.as_ptr().addr(),
        name.len(),
        value.as_ptr().addr(),
        value.len(),
    );
}

pub fn now() -> f64 {
    q_now()
}

#[allow(clippy::cast_possible_truncation)] // milliseconds since 1970 fit an `i64` for the next 292 million years
pub fn wall_ms() -> i64 {
    q_wall_ms() as i64
}

pub fn storage_set(key: &str, value: &str) {
    q_storage_set(
        key.as_ptr().addr(),
        key.len(),
        value.as_ptr().addr(),
        value.len(),
    );
}

pub fn prefers_reduced_motion() -> bool {
    q_reduced_motion() != 0
}
