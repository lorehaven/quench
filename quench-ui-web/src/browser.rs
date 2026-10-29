//! What a page knows that the module cannot ask the DOM for: the wall clock, the time zone, and a small store that
//! survives a reload (the page's `localStorage`, under its own `q:` prefix).

use std::cell::RefCell;
use std::collections::HashMap;

use quench_serde::Value;

use crate::host;

#[derive(Default)]
struct Env {
    time_zone: Option<String>,
    storage: HashMap<String, String>,
}

thread_local! {
    static ENV: RefCell<Env> = RefCell::new(Env::default());
}

/// Takes what `quench.js` read at start (see `q_set_env`).
pub fn set_env(json: &str) {
    let Ok(value) = quench_serde::json::from_str::<Value>(json) else {
        return;
    };
    let storage = value
        .get("storage")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let time_zone = value
        .get("time_zone")
        .and_then(Value::as_str)
        .map(str::to_string);
    ENV.with(|e| *e.borrow_mut() = Env { time_zone, storage });
}

/// Milliseconds since 1970-01-01T00:00:00Z by the browser's clock (`Date.now()`).
#[must_use]
pub fn wall_ms() -> i64 {
    host::wall_ms()
}

/// The browser's IANA time zone (`Europe/Warsaw`); `UTC` when it did not say.
#[must_use]
pub fn time_zone() -> String {
    ENV.with(|e| e.borrow().time_zone.clone())
        .unwrap_or_else(|| "UTC".to_string())
}

/// What was kept under `key`.
#[must_use]
pub fn storage_get(key: &str) -> Option<String> {
    ENV.with(|e| e.borrow().storage.get(key).cloned())
}

/// Keeps `value` under `key`, and in `localStorage` for the next load; `None`, or an empty text, forgets it.
///
/// The browser may refuse to keep it (blocked storage, a private window): then it is kept until the page is closed.
pub fn storage_set(key: &str, value: Option<&str>) {
    let value = value.filter(|v| !v.is_empty());
    ENV.with(|e| match value {
        Some(v) => {
            e.borrow_mut()
                .storage
                .insert(key.to_string(), v.to_string());
        }
        None => {
            e.borrow_mut().storage.remove(key);
        }
    });
    host::storage_set(key, value.unwrap_or(""));
}

#[cfg(not(target_arch = "wasm32"))]
pub fn reset() {
    ENV.with(|e| *e.borrow_mut() = Env::default());
}
