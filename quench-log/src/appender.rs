//! A rolling log file, written from a background thread so that logging
//! never waits on the disk.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use quench_core::time::Time;

use crate::config::Rotation;

/// Lines buffered between the logging threads and the writer before new ones
/// are dropped (and counted).
const QUEUE: usize = 128_000;

/// Which file a moment belongs to: `""`, `2026-10-23`, or `2026-10-23-14`.
#[must_use]
pub fn period(rotation: Rotation, time: Time) -> String {
    let c = time.civil();
    match rotation {
        Rotation::Never => String::new(),
        Rotation::Daily => format!("{:04}-{:02}-{:02}", c.year, c.month, c.day),
        Rotation::Hourly => format!("{:04}-{:02}-{:02}-{:02}", c.year, c.month, c.day, c.hour),
    }
}

/// `prefix.2026-10-23.log`, or `prefix.log` when not rotating.
#[must_use]
pub fn file_name(prefix: &str, suffix: &str, period: &str) -> String {
    if period.is_empty() {
        format!("{prefix}.{suffix}")
    } else {
        format!("{prefix}.{period}.{suffix}")
    }
}

/// An `io::Write` that appends to the file for the current period and keeps at
/// most `max_files` of them.
#[derive(Debug)]
pub struct RollingWriter {
    dir: PathBuf,
    prefix: String,
    suffix: String,
    rotation: Rotation,
    max_files: usize,
    current: Option<(String, File)>,
}

impl RollingWriter {
    pub fn new(
        dir: &Path,
        prefix: &str,
        suffix: &str,
        rotation: Rotation,
        max_files: usize,
    ) -> std::io::Result<Self> {
        let mut writer = Self {
            dir: dir.to_path_buf(),
            prefix: prefix.to_string(),
            suffix: suffix.to_string(),
            rotation,
            max_files,
            current: None,
        };
        writer.roll(&period(rotation, Time::now()))?;
        Ok(writer)
    }

    fn roll(&mut self, period: &str) -> std::io::Result<()> {
        let path = self.dir.join(file_name(&self.prefix, &self.suffix, period));
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        self.current = Some((period.to_string(), file));
        self.prune();
        Ok(())
    }

    /// Deletes the oldest files beyond `max_files` (names sort by date).
    fn prune(&self) {
        if self.max_files == 0 || self.rotation == Rotation::Never {
            return;
        }
        let start = format!("{}.", self.prefix);
        let end = format!(".{}", self.suffix);
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| {
                n.len() > start.len() + end.len() && n.starts_with(&start) && n.ends_with(&end)
            })
            .collect();
        names.sort();
        let excess = names.len().saturating_sub(self.max_files);
        for name in names.into_iter().take(excess) {
            let _ = fs::remove_file(self.dir.join(name));
        }
    }
}

impl Write for RollingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let wanted = period(self.rotation, Time::now());
        if self
            .current
            .as_ref()
            .is_none_or(|(held, _)| *held != wanted)
        {
            self.roll(&wanted)?;
        }
        match &mut self.current {
            Some((_, file)) => file.write(buf),
            None => Ok(0),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.current
            .as_mut()
            .map_or(Ok(()), |(_, file)| file.flush())
    }
}

enum Message {
    Line(Vec<u8>),
    Shutdown,
}

/// Hands lines to a writer thread.
#[derive(Debug, Clone)]
pub struct NonBlocking {
    sender: SyncSender<Message>,
    dropped: Arc<Mutex<u64>>,
}

impl NonBlocking {
    /// Queues a line; if the queue is full the line is dropped and counted.
    pub fn send(&self, line: Vec<u8>) {
        if let Err(TrySendError::Full(_)) = self.sender.try_send(Message::Line(line)) {
            *self.dropped.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        }
    }

    /// Lines lost because the queue was full.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        *self.dropped.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Keep this alive as long as logging should reach the file: dropping it
/// writes out what is queued and stops the writer thread.
#[derive(Debug)]
pub struct Guard {
    sender: SyncSender<Message>,
    worker: Option<JoinHandle<()>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        // `send` blocks if the queue is full, which is right: the shutdown must get in.
        let _ = self.sender.send(Message::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Starts the writer thread for `writer`.
pub fn non_blocking<W: Write + Send + 'static>(mut writer: W) -> (NonBlocking, Guard) {
    let (sender, receiver) = mpsc::sync_channel::<Message>(QUEUE);
    let worker = std::thread::Builder::new()
        .name("quench-log-writer".to_string())
        .spawn(move || {
            while let Ok(Message::Line(line)) = receiver.recv() {
                let _ = writer.write_all(&line);
            }
            let _ = writer.flush();
        })
        .ok();
    let handle = NonBlocking {
        sender: sender.clone(),
        dropped: Arc::new(Mutex::new(0)),
    };
    (handle, Guard { sender, worker })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_core::tmp::TempDir;

    #[test]
    fn periods_and_names() {
        let t = Time::from_ymd_hms(2026, 10, 3, 14, 5, 6).unwrap();
        assert_eq!(period(Rotation::Never, t), "");
        assert_eq!(period(Rotation::Daily, t), "2026-10-03");
        assert_eq!(period(Rotation::Hourly, t), "2026-10-03-14");
        assert_eq!(file_name("svc", "log", ""), "svc.log");
        assert_eq!(file_name("svc", "log", "2026-10-03"), "svc.2026-10-03.log");
    }

    #[test]
    fn writes_append_to_the_current_file() {
        let dir = TempDir::new().unwrap();
        let mut writer = RollingWriter::new(dir.path(), "app", "log", Rotation::Never, 0).unwrap();
        writer.write_all(b"one\n").unwrap();
        writer.write_all(b"two\n").unwrap();
        writer.flush().unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("app.log")).unwrap(),
            "one\ntwo\n"
        );
        // reopening appends rather than truncating
        let mut again = RollingWriter::new(dir.path(), "app", "log", Rotation::Never, 0).unwrap();
        again.write_all(b"three\n").unwrap();
        again.flush().unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("app.log")).unwrap(),
            "one\ntwo\nthree\n"
        );
    }

    #[test]
    fn daily_files_are_named_for_today() {
        let dir = TempDir::new().unwrap();
        let mut writer = RollingWriter::new(dir.path(), "svc", "log", Rotation::Daily, 0).unwrap();
        writer.write_all(b"x\n").unwrap();
        let expected = file_name("svc", "log", &period(Rotation::Daily, Time::now()));
        assert!(dir.path().join(&expected).is_file(), "{expected}");
    }

    #[test]
    fn rolling_to_a_new_period_starts_a_new_file_and_prunes() {
        let dir = TempDir::new().unwrap();
        for day in ["2026-10-01", "2026-10-02", "2026-10-03", "2026-10-04"] {
            fs::write(dir.path().join(format!("svc.{day}.log")), day).unwrap();
        }
        fs::write(dir.path().join("other.2026-09-01.log"), "x").unwrap();
        fs::write(dir.path().join("svc.notes.txt"), "x").unwrap();
        let mut writer = RollingWriter::new(dir.path(), "svc", "log", Rotation::Daily, 3).unwrap();
        // creating today's file left today's plus the two newest old ones
        let mut names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .collect();
        names.sort();
        assert!(
            !names.contains(&"svc.2026-10-01.log".to_string()),
            "{names:?}"
        );
        assert!(
            names.contains(&"other.2026-09-01.log".to_string())
                && names.contains(&"svc.notes.txt".to_string()),
            "{names:?}"
        );
        assert_eq!(
            names
                .iter()
                .filter(|n| n.starts_with("svc.2")
                    && std::path::Path::new(n)
                        .extension()
                        .is_some_and(|e| e == "log"))
                .count(),
            3,
            "{names:?}"
        );
        // forcing a different period rolls over
        writer.roll("2999-01-01").unwrap();
        assert!(dir.path().join("svc.2999-01-01.log").is_file());
    }

    #[test]
    fn the_writer_thread_flushes_on_guard_drop() {
        let dir = TempDir::new().unwrap();
        let writer = RollingWriter::new(dir.path(), "bg", "log", Rotation::Never, 0).unwrap();
        let (queue, guard) = non_blocking(writer);
        for i in 0..200 {
            queue.send(format!("line {i}\n").into_bytes());
        }
        assert_eq!(queue.dropped(), 0);
        drop(guard);
        let text = fs::read_to_string(dir.path().join("bg.log")).unwrap();
        assert_eq!(text.lines().count(), 200);
        assert!(text.starts_with("line 0\n") && text.ends_with("line 199\n"));
    }

    #[test]
    fn sending_after_shutdown_is_harmless() {
        let dir = TempDir::new().unwrap();
        let (queue, guard) = non_blocking(
            RollingWriter::new(dir.path(), "late", "log", Rotation::Never, 0).unwrap(),
        );
        drop(guard);
        queue.send(b"too late\n".to_vec());
        assert_eq!(fs::read_to_string(dir.path().join("late.log")).unwrap(), "");
    }

    #[test]
    fn opening_in_a_missing_directory_fails() {
        let dir = TempDir::new().unwrap();
        assert!(
            RollingWriter::new(&dir.path().join("nope"), "x", "log", Rotation::Never, 0).is_err()
        );
    }
}
