//! `quench new`.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::cli::Template;
use crate::error::{Result, fail};
use crate::{project, templates};

/// Writes the project and returns the files it made.
///
/// # Errors
/// A bad name, a directory that is not empty, or a file that cannot be written.
pub fn create(
    out: &mut dyn Write,
    cwd: &Path,
    name: &str,
    template: Template,
    dir: Option<&str>,
    quench_path: Option<&str>,
) -> Result<Vec<PathBuf>> {
    project::check_name(name)?;
    let root = dir.map_or_else(|| cwd.join(name), |d| cwd.join(d));
    if root.exists() && std::fs::read_dir(&root)?.next().is_some() {
        return fail(format!("{} exists and is not empty", root.display()));
    }
    let quench_path = match quench_path {
        Some(p) => Some(
            cwd.join(p)
                .canonicalize()
                .map_err(|e| crate::error::Error(format!("--quench-path {p}: {e}")))?
                .to_string_lossy()
                .into_owned(),
        ),
        None => None,
    };
    let mut written = Vec::new();
    for (path, text) in templates::render(name, template, quench_path.as_deref())? {
        let target = root.join(&path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, text)?;
        written.push(target);
    }
    writeln!(
        out,
        "created {name} ({}) in {}",
        template.name(),
        root.display()
    )?;
    writeln!(out, "next:  cd {} && quench dev", root.display())?;
    Ok(written)
}
