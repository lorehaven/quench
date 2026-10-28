//! The native code of {{ name }}: operations the Android host can run, from the app or from a
//! background job with no page open. Register an operation by name; the host starts this program,
//! calls the operation with bytes and gets bytes back (see `quench-android`). Standard output is the
//! protocol's: an operation that wants to log writes to standard error.

use std::io::Write;

use quench_core::time::Time;
use quench_serde::{Value, json};

/// A background job that proves the app's native code runs on its own: it appends a line with the
/// time to `heartbeat.log` in the app's files directory and answers how many lines there are.
/// The input is `{"dir": "<files directory>"}`.
fn heartbeat(input: &[u8]) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(input).map_err(|e| e.to_string())?;
    let request: Value = json::from_str(text).map_err(|e| e.to_string())?;
    let dir = request
        .get("dir")
        .and_then(Value::as_str)
        .ok_or("the job needs the files directory")?;
    let path = std::path::Path::new(dir).join("heartbeat.log");
    let before = std::fs::read_to_string(&path).map_or(0, |t| t.lines().count());
    let now = Time::now().to_rfc3339();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    writeln!(file, "{now} heartbeat {}", before + 1).map_err(|e| e.to_string())?;
    Ok(json::to_string(&json!({ "lines": before + 1, "at": now })).into_bytes())
}

fn register_ops() {
    quench_android::register("job.heartbeat", heartbeat);
}

fn main() -> std::process::ExitCode {
    quench_android::run(register_ops)
}
