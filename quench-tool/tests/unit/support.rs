//! A pretend machine: it records what it was asked to run and answers from tables.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use quench_tool::system::{Cmd, Http, Process, System};

#[derive(Default)]
pub struct Fake {
    pub ran: RefCell<Vec<Cmd>>,
    pub spawned: RefCell<Vec<(Cmd, Rc<Cell<bool>>)>>,
    /// A command whose line contains this fails with exit code 101.
    pub fail_on: RefCell<Option<String>>,
    /// Exit code of a spawned process, once `exit_spawned` is set.
    pub exit_spawned: Cell<Option<i32>>,
    pub captures: HashMap<String, String>,
    pub vars: HashMap<String, String>,
    pub existing: HashSet<PathBuf>,
    pub dirs: HashMap<PathBuf, Vec<String>>,
    pub programs: HashSet<String>,
    /// Whether something ever listens on the port `wait_port` is asked about.
    pub listening: Cell<bool>,
    /// Every HTTP request, and the answers to give (a 201 with no body when the queue is empty).
    pub requests: RefCell<Vec<Http>>,
    pub replies: RefCell<Vec<(u16, String)>>,
}

pub struct FakeProcess {
    killed: Rc<Cell<bool>>,
    exit: Option<i32>,
}

impl Process for FakeProcess {
    fn kill(&mut self) {
        self.killed.set(true);
    }

    fn try_wait(&mut self) -> Option<i32> {
        self.exit
    }
}

impl Fake {
    pub fn lines(&self) -> Vec<String> {
        self.ran.borrow().iter().map(Cmd::line).collect()
    }
}

impl System for Fake {
    fn run(&self, cmd: &Cmd) -> io::Result<i32> {
        self.ran.borrow_mut().push(cmd.clone());
        if cmd.program == "missing-program" {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no such program"));
        }
        let failing = self.fail_on.borrow().clone();
        Ok(if failing.is_some_and(|f| cmd.line().contains(&f)) {
            101
        } else {
            0
        })
    }

    fn spawn(&self, cmd: &Cmd) -> io::Result<Box<dyn Process>> {
        if cmd.program.contains("unstartable") {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no such file"));
        }
        let killed = Rc::new(Cell::new(false));
        self.spawned
            .borrow_mut()
            .push((cmd.clone(), killed.clone()));
        Ok(Box::new(FakeProcess {
            killed,
            exit: self.exit_spawned.get(),
        }))
    }

    fn capture(&self, program: &str, args: &[&str]) -> Option<String> {
        let key = std::iter::once(program)
            .chain(args.iter().copied())
            .collect::<Vec<_>>()
            .join(" ");
        self.captures.get(&key).cloned()
    }

    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
    }

    fn exists(&self, path: &Path) -> bool {
        self.existing.contains(path)
    }

    fn list_dir(&self, path: &Path) -> Vec<String> {
        self.dirs.get(path).cloned().unwrap_or_default()
    }

    fn on_path(&self, program: &str) -> bool {
        self.programs.contains(program)
    }

    fn http(&self, request: &Http) -> io::Result<(u16, Vec<u8>)> {
        self.requests.borrow_mut().push(request.clone());
        if request.url.contains("unreachable") {
            return Err(io::Error::other("connection refused"));
        }
        let mut replies = self.replies.borrow_mut();
        let (status, body) = if replies.is_empty() {
            (201, String::new())
        } else {
            replies.remove(0)
        };
        Ok((status, body.into_bytes()))
    }

    fn free_port(&self) -> u16 {
        4242
    }

    fn wait_port(&self, _port: u16, _millis: u64) -> bool {
        self.listening.get()
    }
}

/// A fresh empty directory under the system's temporary one.
pub fn tmp(label: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "quench-tool-test-{}-{label}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn text(out: Vec<u8>) -> String {
    String::from_utf8(out).unwrap()
}

/// A project directory with a manifest, and the project read from it.
pub fn project(label: &str, template: &str) -> quench_tool::project::Project {
    project_named(label, "demo", template)
}

pub fn project_named(label: &str, name: &str, template: &str) -> quench_tool::project::Project {
    let dir = tmp(label);
    std::fs::write(
        dir.join(".quench.toml"),
        format!("name = \"{name}\"\ntemplate = \"{template}\"\n"),
    )
    .unwrap();
    quench_tool::project::Project::find(&dir).unwrap()
}
