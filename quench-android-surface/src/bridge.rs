//! Calls to the Kotlin host's plugins (`device`, `files`, `notifications`, `packages`, ...).
//!
//! The way a web page calls them through `quench.js`: a plugin, a method and JSON arguments go out; the answer
//! comes back later and runs the closure that was given.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use quench_serde::{Map, Value, json};

/// Why a call failed: the plugin's error code and message (`unknown_plugin`, `invalid_args`, `denied`, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

type Answer = Box<dyn FnOnce(Result<Value, CallError>)>;
type Listener = Box<dyn FnMut(&Value)>;

#[derive(Default)]
struct State {
    next: u64,
    outbox: Vec<String>,
    pending: HashMap<u64, Answer>,
    listeners: HashMap<(String, String), Vec<Listener>>,
}

/// The way to the host's plugins. Clones share one state; it belongs to the program's single thread.
#[derive(Clone, Default)]
pub struct Bridge {
    state: Rc<RefCell<State>>,
}

impl std::fmt::Debug for Bridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bridge").finish_non_exhaustive()
    }
}

impl Bridge {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Calls `plugin.method(args)`; `then` runs with the answer when it arrives.
    pub fn call(
        &self,
        plugin: &str,
        method: &str,
        args: Value,
        then: impl FnOnce(Result<Value, CallError>) + 'static,
    ) {
        let mut state = self.state.borrow_mut();
        state.next += 1;
        let id = state.next;
        let mut envelope = Map::new();
        envelope.insert("id", Value::UInt(id));
        envelope.insert("plugin", Value::String(plugin.into()));
        envelope.insert("method", Value::String(method.into()));
        envelope.insert("args", args);
        state.outbox.push(json::to_string(&Value::Object(envelope)));
        state.pending.insert(id, Box::new(then));
    }

    /// Runs `handler` for each event the plugin pushes (progress, a new intent).
    pub fn on(&self, plugin: &str, event: &str, handler: impl FnMut(&Value) + 'static) {
        self.state
            .borrow_mut()
            .listeners
            .entry((plugin.into(), event.into()))
            .or_default()
            .push(Box::new(handler));
    }

    /// The calls waiting to go to the host.
    pub(crate) fn take_outbox(&self) -> Vec<String> {
        std::mem::take(&mut self.state.borrow_mut().outbox)
    }

    /// An answer or an event from the host.
    pub(crate) fn deliver(&self, envelope: &str) {
        let Ok(message) = json::parse(envelope) else {
            return;
        };
        if let Some(id) = message.get("id").and_then(Value::as_u64) {
            // Taken out first: the closure may call again.
            let answer = self.state.borrow_mut().pending.remove(&id);
            let Some(answer) = answer else { return };
            answer(message.get("err").map_or_else(
                || Ok(message.get("ok").cloned().unwrap_or_default()),
                |err| {
                    let text = |key, otherwise: &str| {
                        err.get(key)
                            .and_then(Value::as_str)
                            .unwrap_or(otherwise)
                            .to_string()
                    };
                    Err(CallError {
                        code: text("code", "internal"),
                        message: text("message", ""),
                    })
                },
            ));
        } else if let (Some(event), Some(plugin)) = (
            message.get("event").and_then(Value::as_str),
            message.get("plugin").and_then(Value::as_str),
        ) {
            let key = (plugin.to_string(), event.to_string());
            let data = message.get("data").cloned().unwrap_or_default();
            let taken = self.state.borrow_mut().listeners.remove(&key);
            if let Some(mut handlers) = taken {
                for handler in &mut handlers {
                    handler(&data);
                }
                // Handlers added while these ran come after.
                let mut state = self.state.borrow_mut();
                let added = state.listeners.remove(&key).unwrap_or_default();
                handlers.extend(added);
                state.listeners.insert(key, handlers);
            }
        }
    }
}
