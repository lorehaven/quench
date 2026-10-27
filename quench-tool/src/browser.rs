//! `quench test --browser`: the project's own server, a headless Chromium, and `tests/browser.mjs`.

use std::io::Write;

use crate::error::{Result, fail};
use crate::project::Project;
use crate::release::{build_server, build_wasm, step};
use crate::system::{Cmd, System};

/// The Chromium-family programs the script can drive, in the order they are tried.
pub const BROWSERS: &[&str] = &[
    "chromium",
    "chromium-browser",
    "google-chrome",
    "google-chrome-stable",
];

/// How long the server gets to start listening.
const START_MILLIS: u64 = 15_000;

/// Builds the app, serves it on a free port and runs the script against it.
///
/// # Errors
/// A missing tool (with what to install), a build that fails, a server that does not start, or a
/// script that fails.
pub fn run(sys: &dyn System, out: &mut dyn Write, project: &Project) -> Result<()> {
    let script = project.root.join("tests/browser.mjs");
    if !sys.exists(&script) {
        return fail("tests/browser.mjs is missing: a project made by `quench new` has one");
    }
    if !sys.on_path("node") {
        return fail("`node` (22 or later) is needed to drive the browser: install Node.js");
    }
    let Some(browser) = BROWSERS.iter().find(|b| sys.on_path(b)) else {
        return fail("no Chromium found on PATH (tried chromium, google-chrome): install one");
    };
    build_wasm(sys, out, project, false)?;
    build_server(sys, out, project, false, false)?;
    let port = sys.free_port();
    let binary = project.server_path("debug");
    let server = Cmd::new(&binary.to_string_lossy(), &[], &project.root)
        .env("PORT", &port.to_string())
        .env("WASM", &project.wasm_path("wasm-dev").to_string_lossy());
    let mut child = sys
        .spawn(&server)
        .map_err(|e| crate::error::Error(format!("cannot start {}: {e}", binary.display())))?;
    let result = if sys.wait_port(port, START_MILLIS) {
        let url = format!("http://127.0.0.1:{port}");
        step(
            sys,
            out,
            &Cmd::new("node", &["tests/browser.mjs", &url], &project.root).env("CHROMIUM", browser),
        )
    } else {
        fail(format!(
            "the server did not start listening on port {port} within {} s",
            START_MILLIS / 1000
        ))
    };
    child.kill();
    result
}
