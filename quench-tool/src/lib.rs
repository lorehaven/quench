//! The `quench` command.
//!
//! ```text
//! quench new demo            # app/, core/, server/, shared/ and a quench.toml
//! cd demo && quench dev      # build, serve, reload the browser when a file changes
//! quench build --release     # dist/: one server binary with the app inside
//! ```

pub mod android;
pub mod browser;
pub mod cli;
pub mod dev;
pub mod doctor;
pub mod error;
pub mod linux;
pub mod plugins;
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

use crate::cli::{AndroidCmd, BuildTarget, Cli, Cmd, LinuxCmd, PluginCmd, RunTarget};
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

// One arm per command: it reads best as the list it is.
#[allow(clippy::too_many_lines)]
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
        Cmd::Android { cmd } => {
            let project = Project::find(cwd)?;
            match cmd {
                AndroidCmd::Init { package } => android::init(out, &project, package.as_deref())?,
                AndroidCmd::Keystore { out: path, alias } => {
                    android::keystore(sys, out, &project, &path, &alias)?;
                }
                AndroidCmd::Launcher { which } => android::launcher(out, &project, which)?,
                AndroidCmd::BuildNative { release, abi } => {
                    android::build_native(sys, out, &project, release, abi.as_deref())?;
                }
            }
            Ok(0)
        }
        Cmd::Run { target, abi } => {
            let project = Project::find(cwd)?;
            match target {
                RunTarget::Android => android::run(sys, out, &project, abi.as_deref())?,
                RunTarget::Linux => linux::run(sys, out, &project)?,
            }
            Ok(0)
        }
        Cmd::Linux { cmd } => {
            let project = Project::find(cwd)?;
            match cmd {
                LinuxCmd::Init { id, quench_path } => {
                    linux::init(out, &project, id.as_deref(), quench_path.as_deref())?;
                }
            }
            Ok(0)
        }
        Cmd::Plugin { cmd } => {
            let project = Project::find(cwd)?;
            match cmd {
                PluginCmd::Add { name } => plugins::add(out, &project, &name)?,
                PluginCmd::Update { name } => plugins::update(out, &project, name.as_deref())?,
                PluginCmd::List => plugins::list(out, &project)?,
            }
            Ok(0)
        }
        Cmd::Doctor => {
            writeln!(out, "quench doctor")?;
            Ok(i32::from(!doctor::report(out, &doctor::checks(sys))?))
        }
        Cmd::Dev {
            port,
            open,
            android,
            linux,
        } => {
            let project = Project::find(cwd)?;
            let mut watch = watcher(&project.root);
            dev::dev(
                sys,
                out,
                &project,
                dev::Options {
                    port,
                    open,
                    android,
                    linux,
                },
                &mut *watch,
            )?;
            Ok(0)
        }
        Cmd::Build {
            release,
            no_embed,
            bundle,
            target,
            abi,
        } => {
            let project = Project::find(cwd)?;
            if target == Some(BuildTarget::Linux) {
                linux::build(sys, out, &project, release)?;
            } else if target == Some(BuildTarget::Deb) {
                linux::deb(sys, out, &project)?;
            } else if let Some(target) = target {
                let aab = target == BuildTarget::Aab;
                android::build_apk(sys, out, &project, release || aab, aab, abi.as_deref())?;
            } else if bundle {
                release::bundle(sys, out, &project)?;
            } else {
                release::build(sys, out, &project, release, !no_embed)?;
            }
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
