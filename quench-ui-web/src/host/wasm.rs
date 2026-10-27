//! The real imports: functions `quench.js` provides in the `"env"` module.
#![allow(unsafe_code)] // calling imports and passing pointers is what a wasm host boundary is

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn q_create_el(tag_ptr: *const u8, tag_len: usize) -> u32;
    fn q_create_text(ptr: *const u8, len: usize) -> u32;
    fn q_create_anchor() -> u32;
    fn q_set_text(h: u32, ptr: *const u8, len: usize);
    fn q_set_attr(h: u32, name_ptr: *const u8, name_len: usize, val_ptr: *const u8, val_len: usize);
    fn q_remove_attr(h: u32, name_ptr: *const u8, name_len: usize);
    fn q_insert(parent: u32, child: u32, before: u32);
    fn q_remove(parent: u32, child: u32);
    fn q_listen(h: u32, kind: u32);
    fn q_release(h: u32);
    fn q_log(level: u32, ptr: *const u8, len: usize);
    fn q_set_timeout(id: u32, ms: u32);
    fn q_clear_timeout(id: u32);
    fn q_fetch(
        id: u32,
        method_ptr: *const u8,
        method_len: usize,
        url_ptr: *const u8,
        url_len: usize,
        headers_ptr: *const u8,
        headers_len: usize,
        body_ptr: *const u8,
        body_len: usize,
    );
    fn q_fetch_abort(id: u32);
    fn q_schedule_tick();
}

// SAFETY (all wrappers): the pointers come from live `&str`/`&[u8]` borrows that outlast the call,
// and the contract (docs/quench-ui-web.md) has JS copy the bytes before the import returns.

pub fn create_element(tag: &str) -> u32 {
    unsafe { q_create_el(tag.as_ptr(), tag.len()) }
}

pub fn create_text(text: &str) -> u32 {
    unsafe { q_create_text(text.as_ptr(), text.len()) }
}

pub fn create_anchor() -> u32 {
    unsafe { q_create_anchor() }
}

pub fn set_text(h: u32, text: &str) {
    unsafe { q_set_text(h, text.as_ptr(), text.len()) }
}

pub fn set_attr(h: u32, name: &str, value: &str) {
    unsafe { q_set_attr(h, name.as_ptr(), name.len(), value.as_ptr(), value.len()) }
}

pub fn remove_attr(h: u32, name: &str) {
    unsafe { q_remove_attr(h, name.as_ptr(), name.len()) }
}

pub fn insert(parent: u32, child: u32, before: u32) {
    unsafe { q_insert(parent, child, before) }
}

pub fn remove(parent: u32, child: u32) {
    unsafe { q_remove(parent, child) }
}

pub fn listen(h: u32, kind: u32) {
    unsafe { q_listen(h, kind) }
}

pub fn release(h: u32) {
    unsafe { q_release(h) }
}

pub fn log(level: u32, message: &str) {
    unsafe { q_log(level, message.as_ptr(), message.len()) }
}

pub fn set_timeout(id: u32, ms: u32) {
    unsafe { q_set_timeout(id, ms) }
}

pub fn clear_timeout(id: u32) {
    unsafe { q_clear_timeout(id) }
}

pub fn fetch(id: u32, method: &str, url: &str, headers: &str, body: &[u8]) {
    unsafe {
        q_fetch(
            id,
            method.as_ptr(),
            method.len(),
            url.as_ptr(),
            url.len(),
            headers.as_ptr(),
            headers.len(),
            body.as_ptr(),
            body.len(),
        );
    }
}

pub fn fetch_abort(id: u32) {
    unsafe { q_fetch_abort(id) }
}

pub fn schedule_tick() {
    unsafe { q_schedule_tick() }
}
