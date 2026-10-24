//! Running a program and asserting on how it ended.
//!
//! ```no_run
//! use quench_test::cmd::Command;
//! use quench_test::predicate::prelude::*;
//!
//! Command::cargo_bin("mytool")
//!     .unwrap()
//!     .args(["--version"])
//!     .assert()
//!     .success()
//!     .stdout(predicate::str::contains("mytool"));
//! ```

use crate::predicate::Predicate;
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// The program to test could not be found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoBinError(String);

impl std::fmt::Display for CargoBinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CargoBinError {}

/// A program to run, with its arguments, environment and input.
#[derive(Debug, Clone)]
pub struct Command {
    program: OsString,
    args: Vec<OsString>,
    env: Vec<(OsString, Option<OsString>)>,
    cwd: Option<PathBuf>,
    stdin: Option<Vec<u8>>,
    timeout: Option<Duration>,
}

/// What a finished program produced.
#[derive(Debug, Clone)]
pub struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Command {
    #[must_use]
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_os_string(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            stdin: None,
            timeout: None,
        }
    }

    /// The binary target `name` of the package under test: Cargo's `CARGO_BIN_EXE_<name>` when set, else
    /// the executable next to the test binary's `deps` directory.
    ///
    /// # Errors
    ///
    /// [`CargoBinError`] when neither points at an existing file.
    pub fn cargo_bin(name: &str) -> Result<Self, CargoBinError> {
        if let Some(path) = std::env::var_os(format!("CARGO_BIN_EXE_{name}"))
            .map(PathBuf::from)
            .filter(|p| p.is_file())
        {
            return Ok(Self::new(path));
        }
        let exe = std::env::current_exe().map_err(|e| CargoBinError(e.to_string()))?;
        let mut dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
        if dir.file_name().is_some_and(|n| n == "deps") {
            dir.pop();
        }
        let candidate = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        if candidate.is_file() {
            Ok(Self::new(candidate))
        } else {
            Err(CargoBinError(format!(
                "no binary `{name}` next to {}",
                exe.display()
            )))
        }
    }

    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|a| a.as_ref().to_os_string()));
        self
    }

    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.env.push((
            key.as_ref().to_os_string(),
            Some(value.as_ref().to_os_string()),
        ));
        self
    }

    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        for (k, v) in vars {
            self.env(k, v);
        }
        self
    }

    pub fn env_remove(&mut self, key: impl AsRef<OsStr>) -> &mut Self {
        self.env.push((key.as_ref().to_os_string(), None));
        self
    }

    pub fn current_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    /// What the program reads on its standard input.
    pub fn write_stdin(&mut self, data: impl Into<Vec<u8>>) -> &mut Self {
        self.stdin = Some(data.into());
        self
    }

    /// Kill the program (and fail) if it runs longer than this.
    pub const fn timeout(&mut self, timeout: Duration) -> &mut Self {
        self.timeout = Some(timeout);
        self
    }

    fn describe(&self) -> String {
        let mut parts = vec![self.program.to_string_lossy().into_owned()];
        parts.extend(self.args.iter().map(|a| a.to_string_lossy().into_owned()));
        parts.join(" ")
    }

    /// Runs the program to completion.
    ///
    /// # Errors
    ///
    /// When it cannot be started, its pipes fail, or it outlives the timeout (it is killed first).
    pub fn output(&mut self) -> std::io::Result<Output> {
        let mut command = std::process::Command::new(&self.program);
        command.args(&self.args);
        for (key, value) in &self.env {
            match value {
                Some(v) => command.env(key, v),
                None => command.env_remove(key),
            };
        }
        if let Some(dir) = &self.cwd {
            command.current_dir(dir);
        }
        command
            .stdin(if self.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;

        let input = self.stdin.clone();
        let writer = child.stdin.take().map(|mut pipe| {
            std::thread::spawn(move || {
                if let Some(data) = input {
                    let _ = pipe.write_all(&data);
                }
            })
        });
        let reader = |pipe: Option<Box<dyn Read + Send>>| {
            std::thread::spawn(move || {
                let mut data = Vec::new();
                if let Some(mut pipe) = pipe {
                    let _ = pipe.read_to_end(&mut data);
                }
                data
            })
        };
        let out = reader(
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        );
        let err = reader(
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        );

        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if self.timeout.is_some_and(|t| started.elapsed() > t) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "`{}` did not finish within {:?}",
                        self.describe(),
                        self.timeout.unwrap_or_default()
                    ),
                ));
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        if let Some(writer) = writer {
            let _ = writer.join();
        }
        Ok(Output {
            status,
            stdout: out.join().unwrap_or_default(),
            stderr: err.join().unwrap_or_default(),
        })
    }

    /// Runs the program and returns its output for assertions.
    ///
    /// # Panics
    ///
    /// When the program cannot be run.
    pub fn assert(&mut self) -> Assert {
        let description = self.describe();
        match self.output() {
            Ok(output) => Assert {
                output,
                description,
            },
            Err(e) => panic!("could not run `{description}`: {e}"),
        }
    }
}

/// What can be asserted about a program's output: text predicates, or an exact text or bytes.
pub trait OutputPredicate {
    fn check(&self, bytes: &[u8]) -> bool;
    fn describe(&self) -> String;
}

impl<P: Predicate<str>> OutputPredicate for P {
    fn check(&self, bytes: &[u8]) -> bool {
        self.eval(&String::from_utf8_lossy(bytes))
    }

    fn describe(&self) -> String {
        Predicate::describe(self)
    }
}

/// An exact byte string.
#[derive(Debug, Clone)]
pub struct Exactly(pub Vec<u8>);

impl OutputPredicate for Exactly {
    fn check(&self, bytes: &[u8]) -> bool {
        self.0 == bytes
    }

    fn describe(&self) -> String {
        format!("exactly {:?}", String::from_utf8_lossy(&self.0))
    }
}

/// A finished program, ready for chained assertions that panic with the full output when they fail.
#[derive(Debug, Clone)]
pub struct Assert {
    output: Output,
    description: String,
}

impl Assert {
    #[must_use]
    pub const fn get_output(&self) -> &Output {
        &self.output
    }

    fn fail(&self, expectation: &str) -> ! {
        panic!(
            "assertion failed for `{}`: {expectation}\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            self.description,
            self.output.status,
            String::from_utf8_lossy(&self.output.stdout),
            String::from_utf8_lossy(&self.output.stderr)
        )
    }

    /// The program exited with status 0.
    pub fn success(self) -> Self {
        if !self.output.status.success() {
            self.fail("expected success");
        }
        self
    }

    /// The program exited with a non-zero status (or was killed).
    pub fn failure(self) -> Self {
        if self.output.status.success() {
            self.fail("expected failure");
        }
        self
    }

    /// The program exited with exactly this status code.
    pub fn code(self, expected: i32) -> Self {
        if self.output.status.code() != Some(expected) {
            self.fail(&format!("expected exit code {expected}"));
        }
        self
    }

    /// The standard output satisfies the predicate.
    pub fn stdout(self, predicate: impl OutputPredicate) -> Self {
        if !predicate.check(&self.output.stdout) {
            self.fail(&format!("expected stdout to be {}", predicate.describe()));
        }
        self
    }

    /// The standard error satisfies the predicate.
    pub fn stderr(self, predicate: impl OutputPredicate) -> Self {
        if !predicate.check(&self.output.stderr) {
            self.fail(&format!("expected stderr to be {}", predicate.describe()));
        }
        self
    }
}

impl OutputPredicate for &str {
    fn check(&self, bytes: &[u8]) -> bool {
        self.as_bytes() == bytes
    }

    fn describe(&self) -> String {
        format!("exactly {self:?}")
    }
}

impl OutputPredicate for String {
    fn check(&self, bytes: &[u8]) -> bool {
        self.as_bytes() == bytes
    }

    fn describe(&self) -> String {
        format!("exactly {self:?}")
    }
}
