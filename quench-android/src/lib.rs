//! Native code for the Android host, in a process of its own.
//!
//! The app's Rust code is a small program (`native/`, built to `libquench_native.so` and started from
//! the app's native library directory). The host (Kotlin) starts it, sends one request and reads one
//! answer over its standard input and output, so there is no JNI, no loaded library, and no `unsafe`:
//! the program's pipes are the whole interface, and a crash in native code ends that process, not the
//! app. A request is an operation name and a byte string; operations are plain Rust functions
//! registered by name.
//!
//! A frame is a 4-byte big-endian length and that many bytes. A request is two frames (the operation,
//! then the input); an answer is one, whose first byte is `0` for success (the rest is the result) or
//! `1` for failure (the rest is the message). The program keeps answering until its input ends.
//!
//! ```ignore
//! fn register_ops() {
//!     quench_android::register("heartbeat", |input| Ok(input.to_vec()));
//! }
//! fn main() -> std::process::ExitCode {
//!     quench_android::run(register_ops)
//! }
//! ```
//!
//! Standard output belongs to the protocol: operations log to standard error.

pub mod ops;
mod serve;

use std::collections::HashMap;
use std::sync::{Mutex, Once, PoisonError};

pub use serve::{run, serve};

/// An operation: bytes in, bytes or a message out.
pub type Handler = fn(&[u8]) -> Result<Vec<u8>, String>;

fn registry() -> &'static Mutex<HashMap<&'static str, Handler>> {
    static REGISTRY: std::sync::OnceLock<Mutex<HashMap<&'static str, Handler>>> =
        std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Makes `handler` answer calls to `op`. Registering a name again replaces the earlier handler.
pub fn register(op: &'static str, handler: Handler) {
    registry()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(op, handler);
}

/// Registers the operations that come with this crate, once.
fn builtins() {
    static ONCE: Once = Once::new();
    ONCE.call_once(ops::register_all);
}

/// Runs `op`. An unknown operation and a panic in a handler are errors, never a crash.
///
/// # Errors
/// The handler's message, or why there is no answer.
pub fn call(op: &str, input: &[u8]) -> Result<Vec<u8>, String> {
    builtins();
    let handler = registry()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(op)
        .copied()
        .ok_or_else(|| format!("no native operation `{op}`"))?;
    std::panic::catch_unwind(|| handler(input)).unwrap_or_else(|panic| {
        let message = panic
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".to_string());
        Err(format!("native operation `{op}` panicked: {message}"))
    })
}

/// The answer's bytes: `0` and the result, or `1` and the message.
#[must_use]
pub fn encode(result: Result<Vec<u8>, String>) -> Vec<u8> {
    match result {
        Ok(mut bytes) => {
            bytes.insert(0, 0);
            bytes
        }
        Err(message) => {
            let mut bytes = vec![1];
            bytes.extend_from_slice(message.as_bytes());
            bytes
        }
    }
}
