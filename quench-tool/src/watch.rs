//! Noticing that a source file changed, by looking at modification times: no file-system
//! notification API is needed, and a project's sources are few enough to look at every 300 ms.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::{Duration, SystemTime};

/// What the dev loop learns each time it looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Quiet,
    Changed,
    /// Only a test double ends the loop; the real watcher runs until the process is interrupted.
    Stop,
}

pub trait Watch {
    fn poll(&mut self) -> Event;
}

/// The files whose change means a rebuild.
const EXTENSIONS: &[&str] = &["rs", "toml", "ftl", "css", "js", "html"];

/// Directories never looked into.
const SKIPPED: &[&str] = &["target", "dist", "node_modules"];

/// One number that changes when a source file is added, removed, edited or replaced.
#[must_use]
pub fn fingerprint(root: &Path) -> u64 {
    let mut found: Vec<(String, SystemTime, u64)> = Vec::new();
    collect(root, root, &mut found);
    found.sort();
    let mut hasher = DefaultHasher::new();
    found.hash(&mut hasher);
    hasher.finish()
}

fn collect(root: &Path, dir: &Path, found: &mut Vec<(String, SystemTime, u64)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            if !name.starts_with('.') && !SKIPPED.contains(&name.as_str()) {
                collect(root, &path, found);
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| EXTENSIONS.contains(&e))
        {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            found.push((
                rel.to_string_lossy().into_owned(),
                meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                meta.len(),
            ));
        }
    }
}

/// Polls the file system.
#[derive(Debug)]
pub struct Poller {
    root: std::path::PathBuf,
    last: u64,
    interval: Duration,
}

impl Poller {
    #[must_use]
    pub fn new(root: &Path, interval: Duration) -> Self {
        Self {
            root: root.to_path_buf(),
            last: fingerprint(root),
            interval,
        }
    }
}

impl Watch for Poller {
    fn poll(&mut self) -> Event {
        std::thread::sleep(self.interval);
        let now = fingerprint(&self.root);
        if now == self.last {
            return Event::Quiet;
        }
        // An editor writes a file in several steps: wait until it has settled.
        let mut settled = now;
        loop {
            std::thread::sleep(self.interval / 2);
            let again = fingerprint(&self.root);
            if again == settled {
                break;
            }
            settled = again;
        }
        self.last = settled;
        Event::Changed
    }
}
