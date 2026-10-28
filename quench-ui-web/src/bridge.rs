//! Calls to the native plugins of the host app (Android, and later desktop).
//!
//! A host that shows the app in a `WebView` gives the page a message port. A call is a JSON envelope
//! sent through it, and the answer is another, matched by `id`:
//!
//! ```text
//! app → host   {"id":7,"plugin":"echo","method":"say","args":{"text":"hi"}}
//! host → app   {"id":7,"ok":"hi"}                                  success: any JSON value
//! host → app   {"id":7,"err":{"code":"permission_denied","message":"..."}}
//! ```
//!
//! The host can also push events, which need no call: `{"event":"progress","plugin":"files","data":{..}}`
//! reaches every function registered with [`on_event`]. An event sent while no page is connected is
//! lost.
//!
//! Nothing here is new to the module's boundary with `quench.js`: a call is a [`Request`] to the
//! address `quench-bridge:call`, and the events are an [`EventSource`] on `quench-bridge:events`;
//! `quench.js` recognises the addresses and talks to the port instead of the network. In a plain
//! browser there is no host and every call fails at once with `unavailable`. Declare the calls of a
//! plugin with [`plugin`](crate::plugin) and call them like functions:
//!
//! ```ignore
//! #[quench_ui_web::plugin("echo")]
//! pub trait Echo {
//!     async fn say(text: String) -> Result<String, BridgeError>;
//! }
//!
//! match Echo::say("hi".into()).await {
//!     Ok(text) => { /* ... */ }
//!     Err(e) if e.code == "permission_denied" => { /* ask the person to allow it */ }
//!     Err(e) => { /* e.message */ }
//! }
//! ```

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

use quench_serde::{Deserialize, Serialize, Value, json};

use crate::exec::spawn;
use crate::fetch::{FetchError, Request};
use crate::sse::{EventSource, SseEvent};

/// The address of a call, and of the events.
const CALL: &str = "quench-bridge:call";
const EVENTS: &str = "quench-bridge:events";

/// Why a call to the host failed.
///
/// `code` is one of a small set the application can act on: `unavailable` (no host, or it did not
/// connect), `aborted` (the call was dropped), `unknown_plugin`, `unknown_method`, `invalid_args`,
/// `permission_denied`, `internal`, `bad_response` (the answer was not readable), or a code the
/// plugin chose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeError {
    pub code: String,
    pub message: String,
}

impl BridgeError {
    #[must_use]
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
        }
    }
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for BridgeError {}

/// A listener for the host's events: plugin, event name, data.
type Listener = Rc<dyn Fn(&str, &str, Value)>;

thread_local! {
    static NEXT: Cell<u32> = const { Cell::new(0) };
    static LISTENERS: RefCell<Vec<Listener>> = const { RefCell::new(Vec::new()) };
    static PUMPING: Cell<bool> = const { Cell::new(false) };
}

#[cfg(not(target_arch = "wasm32"))]
pub fn reset() {
    NEXT.with(|n| n.set(0));
    PUMPING.with(|p| p.set(false));
    // Dropped after the borrow ends (see `exec::reset`).
    let listeners = LISTENERS.with(|l| std::mem::take(&mut *l.borrow_mut()));
    drop(listeners);
}

/// Calls `method` of `plugin` with `args` (a JSON object by name) and reads the result as `T`.
///
/// What the methods of a [`plugin`](crate::plugin) trait expand to. Dropping the future forgets the
/// call, and a late answer is ignored.
///
/// # Errors
/// [`BridgeError`]: the host's refusal or failure, `unavailable` without a host, or `bad_response`
/// when the answer is not readable or the result is not a `T`.
#[allow(clippy::future_not_send)] // the browser runs one thread
pub async fn call<A: Serialize, T: Deserialize>(
    plugin: &str,
    method: &str,
    args: &A,
) -> Result<T, BridgeError> {
    let id = NEXT.with(|n| {
        let id = n.get();
        n.set(id.wrapping_add(1));
        id
    });
    let envelope =
        json::to_string(&json!({ "id": id, "plugin": plugin, "method": method, "args": args }));
    let response = Request::post(CALL)
        .body(envelope)
        .send()
        .await
        .map_err(|e| match e {
            FetchError::Network(why) => BridgeError::new("unavailable", &why),
            FetchError::Aborted => BridgeError::new("aborted", "the call was dropped"),
        })?;
    let answer: Value = json::from_str(&response.text())
        .map_err(|e| BridgeError::new("bad_response", &format!("unreadable answer: {e}")))?;
    if let Some(err) = answer.get("err") {
        let field = |name: &str, fallback: &str| {
            err.get(name)
                .and_then(Value::as_str)
                .unwrap_or(fallback)
                .to_string()
        };
        return Err(BridgeError {
            code: field("code", "internal"),
            message: field("message", ""),
        });
    }
    let ok = answer
        .get("ok")
        .ok_or_else(|| BridgeError::new("bad_response", "the answer has no result"))?;
    json::from_str(&json::to_string(ok))
        .map_err(|e| BridgeError::new("bad_response", &format!("unreadable result: {e}")))
}

/// Runs `listener(plugin, event, data)` for every event the host pushes, from now on.
pub fn on_event(listener: impl Fn(&str, &str, Value) + 'static) {
    LISTENERS.with(|l| l.borrow_mut().push(Rc::new(listener)));
    if PUMPING.with(|p| p.replace(true)) {
        return;
    }
    spawn(async {
        let source = EventSource::open(EVENTS, &["event"]);
        while let Some(event) = source.next().await {
            if let SseEvent::Message { data, .. } = event {
                deliver(&data);
            }
        }
    });
}

/// One event from the host. Anything that is not an event is ignored.
fn deliver(text: &str) {
    let Ok(value) = json::from_str::<Value>(text) else {
        return;
    };
    let Some(event) = value.get("event").and_then(Value::as_str) else {
        return;
    };
    let plugin = value.get("plugin").and_then(Value::as_str).unwrap_or("");
    let data = value.get("data").cloned().unwrap_or(Value::Null);
    // Cloned out so a listener may add listeners or make calls.
    let listeners: Vec<_> = LISTENERS.with(|l| l.borrow().clone());
    for listener in listeners {
        listener(plugin, event, data.clone());
    }
}
