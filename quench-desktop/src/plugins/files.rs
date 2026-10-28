use crate::bridge::{Call, Plugin, string_arg};
use quench_serde::{Value, json};
use std::fmt::Write as _;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::UNIX_EPOCH;

/// The largest chunk `read` returns.
pub const CHUNK: usize = 256 * 1024;

/// Asks the user for files or a folder (the system's file chooser).
pub trait Chooser: Send + Sync {
    /// Shows the chooser and calls `done` with what was picked: `None` when it was cancelled.
    fn choose(
        &self,
        folder: bool,
        multiple: bool,
        done: Box<dyn FnOnce(Option<Vec<PathBuf>>) + Send>,
    );
}

/// Files the user picked, and nothing else.
///
/// The page can name any path in a `file://` URI, so the plugin only reads what the user handed over through the
/// chooser in this run: a picked file, or a picked folder and what is under it (symbolic links that leave it
/// are refused). Anything else answers `permission_denied`, as an Android folder that was never granted does.
/// Uploads, the media library and grants that outlive the run are not offered (`unsupported`).
pub struct Files {
    chooser: Arc<dyn Chooser>,
    granted: Arc<Mutex<Vec<PathBuf>>>,
}

impl std::fmt::Debug for Files {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Files").finish_non_exhaustive()
    }
}

type Failure = (&'static str, String);

impl Files {
    #[must_use]
    pub fn new(chooser: Arc<dyn Chooser>) -> Self {
        Self {
            chooser,
            granted: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn grants(&self) -> std::sync::MutexGuard<'_, Vec<PathBuf>> {
        self.granted.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether `real` (a canonical path) is a picked file, or inside a picked folder.
    fn granted(&self, real: &Path) -> bool {
        self.grants()
            .iter()
            .any(|root| real == root || real.starts_with(root))
    }

    /// Opens a granted `uri`, and the real path of what was opened.
    ///
    /// The check is made on the **descriptor that was opened** (its path is read back from `/proc/self/fd`), not
    /// on a path that is looked up again, so a link swapped in between the check and the read cannot redirect
    /// the read. Without `/proc` nothing can be opened: it fails closed. The file is opened without blocking, so
    /// a named pipe in a picked folder cannot hang the call.
    fn open_granted(&self, uri: &str) -> Result<(fs::File, PathBuf), Failure> {
        use std::os::unix::fs::OpenOptionsExt;
        const O_NONBLOCK: i32 = 0o4000; // Linux
        let path = path_of(uri)
            .ok_or_else(|| ("invalid_args", format!("`{uri}` is not a file address")))?;
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(O_NONBLOCK)
            .open(&path)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => {
                    ("not_found", format!("no such file: {}", path.display()))
                }
                _ => ("permission_denied", e.to_string()),
            })?;
        let real = fs::canonicalize(format!("/proc/self/fd/{}", file.as_raw_fd()))
            .map_err(|e| ("permission_denied", e.to_string()))?;
        if self.granted(&real) {
            Ok((file, real))
        } else {
            Err((
                "permission_denied",
                "the user has not picked this file or folder".to_string(),
            ))
        }
    }

    fn pick(&self, folder: bool, multiple: bool, call: Call) {
        let granted = Arc::clone(&self.granted);
        self.chooser.choose(
            folder,
            multiple,
            Box::new(move |picked| {
                let Some(paths) = picked.filter(|p| !p.is_empty()) else {
                    return call.reject("cancelled", "nothing was picked");
                };
                let mut uris = Vec::new();
                for path in paths {
                    let Ok(real) = fs::canonicalize(&path) else {
                        continue;
                    };
                    granted
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(real);
                    uris.push(uri_of(&path));
                }
                call.resolve(&json!({ "uris": uris }));
            }),
        );
    }

    fn list(&self, args: &Value) -> Result<Value, Failure> {
        let (held, dir) =
            self.open_granted(string_arg(args, "uri").map_err(|m| ("invalid_args", m))?)?;
        let limit = args
            .get("limit")
            .and_then(Value::as_i64)
            .map_or(500, |n| usize::try_from(n).unwrap_or(0).max(1));
        let mut entries: Vec<(bool, String, Value)> = Vec::new();
        // Read the folder that was opened and checked, not whatever the path names by now.
        for entry in fs::read_dir(format!("/proc/self/fd/{}", held.as_raw_fd()))
            .map_err(|e| ("not_found", e.to_string()))?
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().into_owned();
            // `metadata` follows links; one that leaves the granted folder is not listed.
            let Ok(real) = fs::canonicalize(entry.path()) else {
                continue;
            };
            if !self.granted(&real) {
                continue;
            }
            let Ok(meta) = fs::metadata(&real) else {
                continue;
            };
            let is_dir = meta.is_dir();
            entries.push((
                !is_dir,
                name.to_lowercase(),
                json!({
                    "uri": uri_of(&dir.join(&name)), "name": name, "mime": mime_of(&name, is_dir),
                    "is_dir": is_dir, "size": size_of(&meta), "modified_ms": modified_ms(&meta),
                }),
            ));
        }
        // Folders first, then by name, as a file manager does.
        entries.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        Ok(json!(
            entries
                .into_iter()
                .take(limit)
                .map(|e| e.2)
                .collect::<Vec<_>>()
        ))
    }

    fn stat(&self, args: &Value) -> Result<Value, Failure> {
        let (file, path) =
            self.open_granted(string_arg(args, "uri").map_err(|m| ("invalid_args", m))?)?;
        let meta = file.metadata().map_err(|e| ("not_found", e.to_string()))?;
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let mime = (!meta.is_dir()).then(|| mime_of(&name, false));
        Ok(json!({ "name": name, "size": size_of(&meta), "mime": mime }))
    }

    fn read(&self, args: &Value) -> Result<Value, Failure> {
        let (mut file, _) =
            self.open_granted(string_arg(args, "uri").map_err(|m| ("invalid_args", m))?)?;
        let offset = u64::try_from(args.get("offset").and_then(Value::as_i64).unwrap_or(0))
            .map_err(|_| ("invalid_args", "`offset` cannot be negative".to_string()))?;
        let length = args
            .get("length")
            .and_then(Value::as_i64)
            .map_or(CHUNK, |n| usize::try_from(n).unwrap_or(1).clamp(1, CHUNK));
        let size = file
            .metadata()
            .map_err(|e| ("internal", e.to_string()))?
            .len();
        if file.metadata().is_ok_and(|m| m.is_dir()) {
            return Err(("invalid_args", "a folder cannot be read".to_string()));
        }
        // A pipe cannot seek: only a read that starts later than the beginning needs to.
        if offset > 0 {
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| ("internal", e.to_string()))?;
        }
        let mut buffer = vec![0; length];
        let mut got = 0;
        while got < length {
            let n = file
                .read(&mut buffer[got..])
                .map_err(|e| ("internal", e.to_string()))?;
            if n == 0 {
                break;
            }
            got += n;
        }
        buffer.truncate(got);
        let end = offset + got as u64;
        Ok(
            json!({ "data": quench_core::base64::STANDARD.encode(&buffer), "size": size, "eof": end >= size }),
        )
    }
}

impl Plugin for Files {
    fn name(&self) -> &'static str {
        "files"
    }

    fn call(&self, method: &str, args: &Value, call: Call) {
        let result = match method {
            "pick_folder" => return self.pick(true, false, call),
            "pick_files" => {
                let multiple = args
                    .get("multiple")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                return self.pick(false, multiple, call);
            }
            "list_folder" => self.list(args),
            "stat" => self.stat(args),
            "read" => self.read(args),
            "media" | "upload" | "cancel_upload" => Err((
                "unsupported",
                format!("files.{method} is not available on the desktop host"),
            )),
            _ => Err(("unknown_method", format!("files has no method `{method}`"))),
        };
        match result {
            Ok(value) => call.resolve(&value),
            Err((code, message)) => call.reject(code, &message),
        }
    }
}

fn size_of(meta: &fs::Metadata) -> i64 {
    if meta.is_dir() {
        0
    } else {
        i64::try_from(meta.len()).unwrap_or(i64::MAX)
    }
}

fn modified_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// The content type of a file by its name (the few kinds an app tells apart).
fn mime_of(name: &str, is_dir: bool) -> String {
    if is_dir {
        return "inode/directory".to_string();
    }
    let extension = name
        .rsplit_once('.')
        .map_or("", |(_, e)| e)
        .to_ascii_lowercase();
    match extension.as_str() {
        "txt" | "md" | "log" => "text/plain",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// `file:///a%20b` for `/a b`.
#[must_use]
pub fn uri_of(path: &Path) -> String {
    let mut out = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~/".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// The path of a `file://` URI; `None` for anything else or a path that is not absolute.
#[must_use]
pub fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = rest.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let path = PathBuf::from(String::from_utf8(out).ok()?);
    (path.is_absolute() && !path.to_string_lossy().contains('\0')).then_some(path)
}
