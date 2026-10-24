//! Finding programs on `PATH` (`which`'s role).

use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

/// No executable by that name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotFound(pub String);

impl fmt::Display for NotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot find binary path: {}", self.0)
    }
}

impl std::error::Error for NotFound {}

/// The executable `name` resolves to. A name with a path separator is checked as it is; a bare name is looked up
/// in each `PATH` directory in order (on Windows also with each `PATHEXT` extension).
///
/// # Errors
///
/// [`NotFound`] when nothing executable matches.
pub fn which(name: impl AsRef<str>) -> Result<PathBuf, NotFound> {
    let name = name.as_ref();
    if name.is_empty() {
        return Err(NotFound(String::new()));
    }
    let found = if name.contains(std::path::MAIN_SEPARATOR) || name.contains('/') {
        candidates(Path::new(name)).find(|path| is_executable(path))
    } else {
        env::var_os("PATH").and_then(|path| {
            env::split_paths(&path)
                .filter(|dir| !dir.as_os_str().is_empty())
                .flat_map(|dir| candidates(&dir.join(name)))
                .find(|path| is_executable(path))
        })
    };
    found.ok_or_else(|| NotFound(name.to_string()))
}

#[cfg(windows)]
fn candidates(base: &Path) -> impl Iterator<Item = PathBuf> + use<> {
    let extensions: Vec<String> = env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
        .split(';')
        .filter(|ext| !ext.is_empty())
        .map(str::to_string)
        .collect();
    let base = base.to_path_buf();
    std::iter::once(base.clone()).chain(extensions.into_iter().map(move |ext| {
        let mut with = base.clone().into_os_string();
        with.push(ext);
        PathBuf::from(with)
    }))
}

#[cfg(not(windows))]
fn candidates(base: &Path) -> impl Iterator<Item = PathBuf> + use<> {
    std::iter::once(base.to_path_buf())
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.metadata().is_ok_and(|meta| meta.is_file())
}

#[cfg(all(test, unix))]
#[allow(clippy::wildcard_imports)]
mod tests {
    use super::*;
    use crate::tmp::TempDir;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn make(dir: &Path, name: &str, mode: u32) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    #[quench_test::test]
    fn finds_programs_by_path_and_by_name() {
        let dir = TempDir::new().unwrap();
        let tool = make(dir.path(), "quench-tool", 0o755);
        make(dir.path(), "not-executable", 0o644);
        fs::create_dir(dir.path().join("a-dir")).unwrap();

        assert_eq!(which(tool.to_str().unwrap()), Ok(tool.clone()));
        assert!(which(dir.path().join("not-executable").to_str().unwrap()).is_err());
        assert!(which(dir.path().join("a-dir").to_str().unwrap()).is_err());
        assert!(which(dir.path().join("missing").to_str().unwrap()).is_err());
        assert!(which("").is_err());

        // `sh` is on every PATH the tests run with.
        assert!(which("sh").unwrap().is_absolute());
        assert_eq!(
            which("definitely-not-a-program-xyz")
                .unwrap_err()
                .to_string(),
            "cannot find binary path: definitely-not-a-program-xyz"
        );
    }
}
