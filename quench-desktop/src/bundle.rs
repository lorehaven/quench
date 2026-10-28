//! The app's assets, served from the host's own origin.
//!
//! A bundle is what `quench build --bundle` writes (`index.html` and `_quench/*`), embedded in the binary or
//! read from a directory. The rules are the Android host's: a path with no extension is a route of the app and
//! gets `index.html`; a path with an extension that is not in the bundle is a `404`; a path that climbs
//! (`..`) or hides a separator is a `404`.

use crate::answer::Answer;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io;
use std::path::Path;

/// The scheme and host the app is served from: `quench://app/`.
pub const SCHEME: &str = "quench";
/// The host of the app's origin.
pub const HOST: &str = "app";

/// The files of an app, by path (`index.html`, `_quench/app.<hash>.wasm`).
#[derive(Debug, Default, Clone)]
pub struct Bundle {
    files: BTreeMap<String, Cow<'static, [u8]>>,
}

impl Bundle {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file; `path` has no leading slash. Embedded bytes (`include_bytes!`) are not copied.
    #[must_use]
    pub fn with(mut self, path: &str, bytes: impl Into<Cow<'static, [u8]>>) -> Self {
        self.files
            .insert(path.trim_start_matches('/').to_string(), bytes.into());
        self
    }

    /// Reads every file under `dir`.
    ///
    /// # Errors
    /// When a directory or file cannot be read.
    pub fn from_dir(dir: &Path) -> io::Result<Self> {
        fn walk(dir: &Path, prefix: &str, into: &mut Bundle) -> io::Result<()> {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                };
                if entry.file_type()?.is_dir() {
                    walk(&entry.path(), &path, into)?;
                } else {
                    into.files
                        .insert(path, Cow::Owned(std::fs::read(entry.path())?));
                }
            }
            Ok(())
        }
        let mut bundle = Self::new();
        walk(dir, "", &mut bundle)?;
        Ok(bundle)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The file at `path`, if the bundle has it.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(AsRef::as_ref)
    }

    /// The answer to a request for `uri`.
    #[must_use]
    pub fn answer(&self, uri: &str) -> Answer {
        let Some(path) = app_path(uri) else {
            return Answer::not_found();
        };
        let file = if path.is_empty() || !has_extension(path) {
            "index.html"
        } else {
            path
        };
        self.files
            .get(file)
            .map_or_else(Answer::not_found, |bytes| {
                Answer::ok(content_type(file), bytes.to_vec())
            })
    }
}

/// The path of a `quench://app/...` URI without its query and fragment or leading slash; `None` for another
/// host, scheme, or a path that climbs or carries a backslash or an encoded separator.
fn app_path(uri: &str) -> Option<&str> {
    let rest = uri.strip_prefix(SCHEME)?.strip_prefix("://")?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    if host != HOST {
        return None;
    }
    let path = path.split(['?', '#']).next().unwrap_or("");
    let lower = path.to_ascii_lowercase();
    let hostile = path.contains('\\')
        || lower.contains("%2f")
        || lower.contains("%5c")
        || path.split('/').any(|part| part == ".." || part == ".");
    (!hostile).then_some(path)
}

fn has_extension(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| name.contains('.'))
}

/// The content type of a file by its extension.
#[must_use]
pub fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
