//! A control socket for tests of an app in its window (debug builds only).
//!
//! With `QUENCH_DESKTOP_CONTROL=<path>` a debug build listens on that Unix socket: each line a client sends is a
//! script to run in the page, and the answer is one line, `{"ok":<the value as JSON>}` or `{"error":"..."}`.
//! A release build ignores the variable. It is what `scripts/linux-e2e.fish` drives the gallery with, the way
//! `scripts/webview-eval.mjs` drives the Android one.

use quench_serde::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;

/// Runs a script in the page and waits for its value as JSON.
pub type Ask = Arc<dyn Fn(String) -> Result<String, String> + Send + Sync>;

/// The answer line for the result of a script.
#[must_use]
pub fn answer(result: &Result<String, String>) -> String {
    match result {
        Ok(json) => format!("{{\"ok\":{json}}}"),
        Err(message) => format!("{{\"error\":{}}}", Value::String(message.clone())),
    }
}

fn serve_client(stream: &UnixStream, ask: &Ask) {
    let Ok(read) = stream.try_clone() else { return };
    let mut write = stream;
    for line in BufReader::new(read).lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let reply = answer(&ask(line));
        if writeln!(write, "{reply}").is_err() {
            return;
        }
    }
}

/// Listens on `path` (replacing a stale socket) in a thread of its own.
///
/// # Errors
/// When the socket cannot be made.
pub fn listen(path: &Path, ask: Ask) -> std::io::Result<()> {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    // Only the person running the app may drive it.
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let ask = Arc::clone(&ask);
            std::thread::spawn(move || serve_client(&stream, &ask));
        }
    });
    Ok(())
}
