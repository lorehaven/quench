//! The page's way to the plugins on the desktop: the same envelopes as the Android host.
//!
//! A request is `{"id","plugin","method","args"}`; it is answered by `{"id","ok"}` or
//! `{"id","err":{"code","message"}}`, and a plugin can push `{"event","plugin","data"}`. See the contract in
//! `docs/quench-ui-web.md`. Here the plugins are Rust types ([`Plugin`]); a call answers once, now or later,
//! from any thread, through its [`Call`].

use quench_serde::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The largest message taken; the rest of the app is not meant to move data this way.
pub const MAX_MESSAGE: usize = 1 << 20;

/// Delivers a JSON text to the page.
pub type Deliver = Arc<dyn Fn(String) + std::marker::Send + Sync>;

/// The answer to one call: [`resolve`](Self::resolve) or [`reject`](Self::reject), once (both consume it).
pub struct Call {
    id: Value,
    send: Deliver,
}

impl std::fmt::Debug for Call {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Call")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl Call {
    /// Answers with `value`.
    pub fn resolve(self, value: &Value) {
        (self.send)(json!({ "id": self.id, "ok": value }).to_string());
    }

    /// Answers with an error the app handles by its `code`.
    pub fn reject(self, code: &str, message: &str) {
        (self.send)(
            json!({ "id": self.id, "err": { "code": code, "message": message } }).to_string(),
        );
    }
}

/// Pushes events to the page.
#[derive(Clone)]
pub struct Emitter {
    send: Deliver,
}

impl std::fmt::Debug for Emitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Emitter").finish_non_exhaustive()
    }
}

impl Emitter {
    /// An event named `event` of `plugin` with `data`.
    pub fn emit(&self, plugin: &str, event: &str, data: &Value) {
        (self.send)(json!({ "event": event, "plugin": plugin, "data": data }).to_string());
    }
}

/// A native capability the web app calls by name.
pub trait Plugin: std::marker::Send + Sync {
    /// The name the app calls it by.
    fn name(&self) -> &'static str;

    /// Handles `method` with the named `args`. Answer through `call`, now or later, from any thread.
    fn call(&self, method: &str, args: &Value, call: Call);

    /// Called once, with the way to push events.
    fn attach(&self, _emitter: Emitter) {}
}

/// The plugins of an app and the way to the page.
pub struct Bridge {
    plugins: BTreeMap<&'static str, Arc<dyn Plugin>>,
    send: Deliver,
}

impl std::fmt::Debug for Bridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bridge")
            .field("plugins", &self.plugins.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl Bridge {
    /// A bridge to `plugins`; `send` delivers a JSON text to the page.
    pub fn new(plugins: Vec<Arc<dyn Plugin>>, send: Deliver) -> Self {
        let bridge = Self {
            plugins: plugins.into_iter().map(|p| (p.name(), p)).collect(),
            send,
        };
        for plugin in bridge.plugins.values() {
            plugin.attach(bridge.emitter());
        }
        bridge
    }

    #[must_use]
    pub fn emitter(&self) -> Emitter {
        Emitter {
            send: Arc::clone(&self.send),
        }
    }

    /// Handles one message from the page. A message that is not an envelope with an `id` is dropped (there
    /// is nobody to answer); anything else is answered.
    pub fn handle(&self, text: &str) {
        if text.len() > MAX_MESSAGE {
            return;
        }
        let Ok(message) = quench_serde::json::from_str::<Value>(text) else {
            return;
        };
        let Some(id) = message.get("id").filter(|id| !id.is_null()) else {
            return;
        };
        let call = Call {
            id: id.clone(),
            send: Arc::clone(&self.send),
        };
        let name = message.get("plugin").and_then(Value::as_str).unwrap_or("");
        let Some(plugin) = self.plugins.get(name) else {
            return call.reject("unknown_plugin", &format!("no plugin `{name}`"));
        };
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let empty = Value::Object(quench_serde::Map::new());
        let args = message
            .get("args")
            .filter(|a| a.is_object())
            .unwrap_or(&empty);
        // A plugin that panics must not take the window down, and the page is still owed an answer.
        let guard = Call {
            id: id.clone(),
            send: Arc::clone(&self.send),
        };
        let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            plugin.call(method, args, call);
        }));
        if ran.is_err() {
            eprintln!("quench-desktop: {name}.{method} panicked");
            guard.reject("internal", &format!("{name}.{method} failed"));
        }
    }
}

/// A plugin's helper: an argument that must be there.
///
/// # Errors
/// An `invalid_args` rejection text when `name` is missing or not a string.
pub fn string_arg<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("`{name}` is needed, as text"))
}
