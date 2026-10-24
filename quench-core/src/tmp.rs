//! Temporary directories for tests and tooling (`tempfile::TempDir`'s role).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{hex, random};

/// A fresh directory under the system temp dir, removed (with everything in
/// it) when dropped.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates `<tmp>/quench-<random>`.
    pub fn new() -> io::Result<Self> {
        Self::new_in(std::env::temp_dir())
    }

    /// Creates `<parent>/quench-<random>`.
    pub fn new_in(parent: impl AsRef<Path>) -> io::Result<Self> {
        let suffix: [u8; 8] = random::bytes();
        let path = parent
            .as_ref()
            .join(format!("quench-{}", hex::encode(suffix)));
        // `create_dir` (not `_all`) so a name collision fails instead of sharing a dir.
        fs::create_dir(&path)?;
        Ok(Self { path })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Keeps the directory on disk and hands back its path.
    #[must_use]
    pub fn into_path(self) -> PathBuf {
        let path = self.path.clone();
        std::mem::forget(self);
        path
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[quench_test::test]
    fn creates_and_removes() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        assert!(path.is_dir());
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("quench-")
        );
        fs::write(path.join("file.txt"), "x").unwrap();
        fs::create_dir(path.join("nested")).unwrap();
        fs::write(path.join("nested/inner"), "y").unwrap();
        assert_eq!(dir.as_ref(), path);
        drop(dir);
        assert!(!path.exists());
    }

    #[quench_test::test]
    fn directories_are_distinct() {
        let (a, b) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        assert_ne!(a.path(), b.path());
    }

    #[quench_test::test]
    fn into_path_keeps_the_directory() {
        let kept = TempDir::new().unwrap().into_path();
        assert!(kept.is_dir());
        fs::remove_dir_all(&kept).unwrap();
    }

    #[quench_test::test]
    fn new_in_uses_the_parent_and_fails_when_missing() {
        let parent = TempDir::new().unwrap();
        let child = TempDir::new_in(parent.path()).unwrap();
        assert!(child.path().starts_with(parent.path()));
        assert!(TempDir::new_in(parent.path().join("missing/deeper")).is_err());
    }
}
