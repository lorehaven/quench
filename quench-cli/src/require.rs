//! A single up-front check for the external binaries a CLI is about to shell
//! out to, so a missing tool fails with one clear line instead of a raw OS
//! error however many steps later.

use std::fmt;

pub struct MissingBinary {
    name: String,
    hint: String,
}

impl fmt::Display for MissingBinary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` not found on PATH. {}", self.name, self.hint)
    }
}

// `main`'s default error path prints `{:?}`, not `{}` — this keeps that
// output the same one-line message rather than a derived struct dump.
impl fmt::Debug for MissingBinary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for MissingBinary {}

/// Fails with a clear, named error unless `name` resolves on `PATH`. `hint`
/// should say what needs it and, where there is one, how to install it.
pub fn require_binary(name: &str, hint: &str) -> Result<(), MissingBinary> {
    if on_path(name) {
        Ok(())
    } else {
        Err(MissingBinary {
            name: name.to_string(),
            hint: hint.to_string(),
        })
    }
}

/// Whether `name` is an executable file in one of the `PATH` directories (or,
/// when it contains a path separator, at that path directly).
fn on_path(name: &str) -> bool {
    use std::path::{Path, PathBuf};

    fn is_executable(path: &Path) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            path.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        }
        #[cfg(not(unix))]
        {
            path.is_file()
        }
    }

    if name.contains(std::path::MAIN_SEPARATOR) {
        return is_executable(Path::new(name));
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        let candidate: PathBuf = dir.join(name);
        is_executable(&candidate)
            || (cfg!(windows) && is_executable(&candidate.with_extension("exe")))
    })
}
