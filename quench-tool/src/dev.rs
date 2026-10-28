//! `quench dev`: build, run the server, and start over when a source file changes. The server was
//! started with `QUENCH_DEV`, so the pages reload themselves once it is back.

use std::io::Write;

use crate::error::Result;
use crate::project::Project;
use crate::release::{build_server, build_wasm};
use crate::system::{Cmd, Process, System};
use crate::watch::{Event, Watch};

fn start(
    sys: &dyn System,
    out: &mut dyn Write,
    project: &Project,
    port: u16,
) -> Result<Box<dyn Process>> {
    build_wasm(sys, out, project, false)?;
    build_server(sys, out, project, false, false)?;
    let binary = project.server_path("debug");
    let cmd = Cmd::new(&binary.to_string_lossy(), &[], &project.root)
        .env("PORT", &port.to_string())
        .env("QUENCH_DEV", "1")
        .env("WASM", &project.wasm_path("wasm-dev").to_string_lossy());
    sys.spawn(&cmd)
        .map_err(|e| crate::error::Error(format!("cannot start {}: {e}", binary.display())))
}

/// What `quench dev` was asked.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub port: u16,
    pub open: bool,
    /// Also start the app on the attached device.
    pub android: bool,
    /// Also open the desktop window on the server.
    pub linux: bool,
}

/// Runs until `watch` says to stop (the real one never does: the person interrupts the process).
///
/// # Errors
/// Only output that cannot be written; a failed build is reported and waited out.
pub fn dev(
    sys: &dyn System,
    out: &mut dyn Write,
    project: &Project,
    options: Options,
    watch: &mut dyn Watch,
) -> Result<()> {
    let Options {
        port,
        open,
        android,
        linux,
    } = options;
    // The browser is opened once: later restarts reload the page it already shows.
    let mut opened = false;
    let mut launched = false;
    let mut window: Option<Box<dyn crate::system::Process>> = None;
    loop {
        let mut server = match start(sys, out, project, port) {
            Ok(child) => {
                writeln!(
                    out,
                    "serving http://127.0.0.1:{port} - edit a file, the page reloads"
                )?;
                if android && !launched {
                    launched = true;
                    crate::android::launch_dev(sys, out, project, port)?;
                }
                if linux && window.is_none() {
                    window = crate::linux::launch_dev(sys, out, project, port)?;
                }
                if open && !opened {
                    opened = true;
                    open_browser(sys, out, &format!("http://127.0.0.1:{port}"))?;
                }
                Some(child)
            }
            Err(e) => {
                writeln!(out, "error: {e}\nwaiting for a change")?;
                None
            }
        };
        let mut reported = false;
        let stopped = loop {
            match watch.poll() {
                Event::Changed => break false,
                Event::Stop => break true,
                Event::Quiet => {}
            }
            if let Some(child) = server.as_mut()
                && !reported
                && let Some(code) = child.try_wait()
            {
                writeln!(
                    out,
                    "the server stopped (exit {code}); waiting for a change"
                )?;
                reported = true;
            }
        };
        if let Some(mut child) = server {
            child.kill();
        }
        if stopped {
            if let Some(mut window) = window {
                window.kill();
            }
            return Ok(());
        }
        writeln!(out, "change detected, rebuilding")?;
    }
}

/// Asks the desktop to open `url`: `xdg-open` on Linux, `open` on macOS. Failing to is not an error.
fn open_browser(sys: &dyn System, out: &mut dyn Write, url: &str) -> Result<()> {
    let Some(program) = ["xdg-open", "open"].into_iter().find(|p| sys.on_path(p)) else {
        writeln!(out, "no xdg-open or open found: open {url} yourself")?;
        return Ok(());
    };
    if sys
        .run(&Cmd::new(program, &[url], std::path::Path::new(".")))
        .map_or(true, |code| code != 0)
    {
        writeln!(out, "{program} failed: open {url} yourself")?;
    }
    Ok(())
}
