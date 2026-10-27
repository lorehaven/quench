//! Everything the tool asks of the machine, behind one trait so the commands can be tested without
//! running cargo.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// A command to run: the program, its arguments, where, and the environment added to ours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

impl Cmd {
    #[must_use]
    pub fn new(program: &str, args: &[&str], cwd: &Path) -> Self {
        Self {
            program: program.to_string(),
            args: args.iter().map(ToString::to_string).collect(),
            cwd: cwd.to_path_buf(),
            env: Vec::new(),
        }
    }

    #[must_use]
    pub fn env(mut self, name: &str, value: &str) -> Self {
        self.env.push((name.to_string(), value.to_string()));
        self
    }

    /// The command as typed, for the progress lines.
    #[must_use]
    pub fn line(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A process the tool started and may stop.
pub trait Process {
    fn kill(&mut self);
    /// The exit code, once the process has ended.
    fn try_wait(&mut self) -> Option<i32>;
}

pub trait System {
    /// Runs `cmd` with the terminal's output and waits; the exit code.
    ///
    /// # Errors
    /// When the program cannot be started.
    fn run(&self, cmd: &Cmd) -> io::Result<i32>;
    /// Starts `cmd` and returns without waiting.
    ///
    /// # Errors
    /// When the program cannot be started.
    fn spawn(&self, cmd: &Cmd) -> io::Result<Box<dyn Process>>;
    /// What `program args` prints (output and error together) when it succeeds.
    fn capture(&self, program: &str, args: &[&str]) -> Option<String>;
    fn var(&self, name: &str) -> Option<String>;
    fn exists(&self, path: &Path) -> bool;
    /// The names inside a directory, sorted.
    fn list_dir(&self, path: &Path) -> Vec<String>;
    /// Whether `program` is found on `PATH`.
    fn on_path(&self, program: &str) -> bool;
    /// A port nothing is listening on.
    fn free_port(&self) -> u16;
    /// Waits until something accepts connections on `port`, for at most `millis`.
    fn wait_port(&self, port: u16, millis: u64) -> bool;
}

/// The real machine.
#[derive(Debug, Default, Clone, Copy)]
pub struct Host;

struct Running(Child);

impl Process for Running {
    fn kill(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }

    fn try_wait(&mut self) -> Option<i32> {
        self.0
            .try_wait()
            .ok()
            .flatten()
            .map(|status| status.code().unwrap_or(-1))
    }
}

fn command(cmd: &Cmd) -> Command {
    let mut c = Command::new(&cmd.program);
    c.args(&cmd.args).current_dir(&cmd.cwd);
    for (name, value) in &cmd.env {
        c.env(name, value);
    }
    c
}

impl System for Host {
    fn run(&self, cmd: &Cmd) -> io::Result<i32> {
        Ok(command(cmd).status()?.code().unwrap_or(-1))
    }

    fn spawn(&self, cmd: &Cmd) -> io::Result<Box<dyn Process>> {
        Ok(Box::new(Running(command(cmd).spawn()?)))
    }

    fn capture(&self, program: &str, args: &[&str]) -> Option<String> {
        let out = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        Some(text)
    }

    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn list_dir(&self, path: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        names.sort();
        names
    }

    fn on_path(&self, program: &str) -> bool {
        std::env::var_os("PATH").is_some_and(|paths| {
            std::env::split_paths(&paths).any(|dir| dir.join(program).is_file())
        })
    }

    fn free_port(&self) -> u16 {
        std::net::TcpListener::bind(("127.0.0.1", 0))
            .and_then(|l| l.local_addr())
            .map_or(3190, |a| a.port())
    }

    fn wait_port(&self, port: u16, millis: u64) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(millis);
        while std::time::Instant::now() < deadline {
            // Connecting again and again to a free port in the ephemeral range can connect the
            // socket to itself (TCP simultaneous open): that is not a server.
            if std::net::TcpStream::connect(("127.0.0.1", port))
                .is_ok_and(|c| c.local_addr().ok() != c.peer_addr().ok())
            {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        false
    }
}
