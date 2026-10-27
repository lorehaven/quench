//! The `quench` command.
//!
//! ```text
//! quench new demo            # app/, core/, server/, shared/ and a quench.toml
//! cd demo && quench dev      # build, serve, reload the browser when a file changes
//! quench build --release     # dist/: one server binary with the app inside
//! ```

pub mod browser;
pub mod cli;
pub mod dev;
pub mod doctor;
pub mod error;
pub mod project;
pub mod release;
pub mod scaffold;
pub mod system;
pub mod templates;
pub mod versions;
pub mod watch;

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use quench_args::{ErrorKind, Parser};

use crate::cli::{Cli, Cmd};
use crate::error::Result;
use crate::project::Project;
use crate::system::{Cmd as Command, System};
use crate::watch::Watch;

/// Runs the command line `args` (without the program name) and returns the exit code.
pub fn run<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let mut out = std::io::stdout();
    execute(args, &system::Host, &mut out, &cwd, &mut |root| {
        Box::new(watch::Poller::new(root, Duration::from_millis(300)))
    })
}

/// As [`run`], on a given system, output, directory and file watcher.
pub fn execute<I, T>(
    args: I,
    sys: &dyn System,
    out: &mut dyn Write,
    cwd: &Path,
    watcher: &mut dyn FnMut(&Path) -> Box<dyn Watch>,
) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let cli = match Cli::try_parse_from(
        std::iter::once(OsString::from("quench")).chain(args.into_iter().map(Into::into)),
    ) {
        Ok(cli) => cli,
        Err(e) => {
            let shown = matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion);
            if shown {
                let _ = writeln!(out, "{}", e.to_string().trim_end());
            } else {
                eprintln!("{}", e.to_string().trim_end());
            }
            return if shown { 0 } else { 2 };
        }
    };
    match command(cli.cmd, sys, out, cwd, watcher) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn command(
    cmd: Cmd,
    sys: &dyn System,
    out: &mut dyn Write,
    cwd: &Path,
    watcher: &mut dyn FnMut(&Path) -> Box<dyn Watch>,
) -> Result<i32> {
    match cmd {
        Cmd::New {
            name,
            template,
            dir,
            quench_path,
        } => {
            scaffold::create(
                out,
                cwd,
                &name,
                template,
                dir.as_deref(),
                quench_path.as_deref(),
            )?;
            Ok(0)
        }
        Cmd::Doctor => {
            writeln!(out, "quench doctor")?;
            Ok(i32::from(!doctor::report(out, &doctor::checks(sys))?))
        }
        Cmd::Dev { port, open } => {
            let project = Project::find(cwd)?;
            let mut watch = watcher(&project.root);
            dev::dev(sys, out, &project, port, open, &mut *watch)?;
            Ok(0)
        }
        Cmd::Build { release, no_embed } => {
            let project = Project::find(cwd)?;
            release::build(sys, out, &project, release, !no_embed)?;
            Ok(0)
        }
        Cmd::Test { browser, args } => {
            let project = Project::find(cwd)?;
            let mut cargo = vec!["test".to_string(), "--workspace".to_string()];
            cargo.extend(args);
            let refs: Vec<&str> = cargo.iter().map(String::as_str).collect();
            release::step(sys, out, &Command::new("cargo", &refs, &project.root))?;
            let app = project.app_package();
            release::step(
                sys,
                out,
                &Command::new(
                    "cargo",
                    &["check", "--target", "wasm32-unknown-unknown", "-p", &app],
                    &project.root,
                ),
            )?;
            if browser {
                browser::run(sys, out, &project)?;
            }
            Ok(0)
        }
    }
}
