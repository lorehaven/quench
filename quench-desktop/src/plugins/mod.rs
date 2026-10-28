//! The plugins the desktop host offers, under the names and with the methods of the Android ones, so the typed
//! clients in `quench_ui_web::plugins` work on both.
//!
//! What a desktop cannot do answers `unsupported`.
//!
//! Where a plugin uses a program of the system (`secret-tool`, `notify-send`, `xdg-open`), it runs it through a
//! [`Runner`], so tests drive the plugin with a stand-in and see exactly what would have been run.

mod device;
mod echo;
mod files;
mod notifications;
mod share;
mod storage;

pub use device::DeviceState;
pub use echo::Echo;
pub use files::{CHUNK, Chooser, Files, path_of, uri_of};
pub use notifications::Notifications;
pub use share::Share;
pub use storage::SecureStorage;

use std::io::{self, Write};
use std::process::{Command, Stdio};

/// What a program printed and how it ended.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Output {
    pub success: bool,
    pub stdout: String,
    /// What it wrote to its error output (`secret-tool search` prints the attributes of the items there).
    pub stderr: String,
}

/// Runs the system's programs.
pub trait Runner: Send + Sync {
    /// Runs `program` with `args`, writing `stdin` to it when given, and waits for it.
    ///
    /// # Errors
    /// When the program cannot be started (usually: it is not installed).
    fn run(&self, program: &str, args: &[&str], stdin: Option<&[u8]>) -> io::Result<Output>;

    /// Starts `program` and does not wait for it (it outlives the call: a viewer, a mail client).
    ///
    /// # Errors
    /// When the program cannot be started.
    fn spawn(&self, program: &str, args: &[&str]) -> io::Result<()>;
}

/// The real thing.
#[derive(Debug, Clone, Copy, Default)]
pub struct System;

impl Runner for System {
    fn run(&self, program: &str, args: &[&str], stdin: Option<&[u8]>) -> io::Result<Output> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let (Some(bytes), Some(mut pipe)) = (stdin, child.stdin.take()) {
            // The program may exit without reading it all; that is its answer, not our error.
            let _ = pipe.write_all(bytes);
        }
        let out = child.wait_with_output()?;
        Ok(Output {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    fn spawn(&self, program: &str, args: &[&str]) -> io::Result<()> {
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
    }
}
