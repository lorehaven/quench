//! What `init()` builds a subscriber from, and how it's read from the
//! environment.
//!
//! Split from `lib.rs` so the parsing itself - format, rotation, the file
//! prefix fallback - is testable as plain functions, with no env var to set
//! and restore around each case.

use std::fmt;
use std::path::PathBuf;

/// A file log line's encoding. Console output is always plain text - only
/// the file sink's format is a choice, and it defaults to `Json` because a
/// file is for a log shipper or `jq`, not a human staring at a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Json,
    Plain,
}

impl Format {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Plain => "plain",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "plain" | "text" => Some(Self::Plain),
            _ => None,
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How often the file sink starts a new file. Paired with `Config::max_files`
/// for retention - `Daily` and `max_files: 7` is a week of history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rotation {
    #[default]
    Daily,
    Hourly,
    /// One file, never rotated. `max_files` has nothing to prune in this
    /// mode - there is only ever the one file.
    Never,
}

impl Rotation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Hourly => "hourly",
            Self::Never => "never",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "daily" => Some(Self::Daily),
            "hourly" => Some(Self::Hourly),
            "never" => Some(Self::Never),
            _ => None,
        }
    }

    pub(crate) const fn to_tracing(self) -> tracing_appender::rolling::Rotation {
        match self {
            Self::Daily => tracing_appender::rolling::Rotation::DAILY,
            Self::Hourly => tracing_appender::rolling::Rotation::HOURLY,
            Self::Never => tracing_appender::rolling::Rotation::NEVER,
        }
    }
}

impl fmt::Display for Rotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What `init()` builds a subscriber from.
///
/// `Default` and `from_env()` agree on every value `from_env()` finds no
/// variable for, so the two are never a source of drift from each other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Plain-text logging to stderr. On by default - a service run
    /// interactively, or under something that captures stdout/stderr (every
    /// container runtime), still wants to see its own logs.
    pub console_enabled: bool,
    /// Rolling JSON (by default) file logging. On by default, alongside the
    /// console rather than instead of it - see the crate's own docs for why
    /// a deployment with no writable `file_dir` should set this to `false`.
    pub file_enabled: bool,
    /// An `EnvFilter` directive string - `RUST_LOG`'s own format
    /// (`info`, `my_crate=debug,tower=warn`, ...). Applies to both sinks;
    /// there is deliberately no way to give the file a different level than
    /// the console, since a level chosen for a human reading a terminal and
    /// one chosen for what gets kept on disk are rarely actually different.
    pub level: String,
    /// Directory the rolling file(s) are written into. Created if missing.
    pub file_dir: PathBuf,
    /// Prepended to each file's name, so several services sharing one
    /// `file_dir` don't overwrite each other's logs.
    pub file_prefix: String,
    pub file_format: Format,
    pub rotation: Rotation,
    /// How many rotated files to keep before the oldest is deleted. `0`
    /// means unlimited - nothing is ever pruned.
    pub max_files: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            console_enabled: true,
            file_enabled: true,
            level: "info".to_string(),
            file_dir: PathBuf::from("logs"),
            file_prefix: "app".to_string(),
            file_format: Format::default(),
            rotation: Rotation::default(),
            max_files: 7,
        }
    }
}

impl Config {
    /// Reads every value from its own environment variable, falling back to
    /// `Config::default()` for whatever is unset or unparseable:
    ///
    /// - `LOG_CONSOLE_ENABLED` / `LOG_FILE_ENABLED` (`true`/`false` and the
    ///   usual spellings - see `parse_bool`)
    /// - `RUST_LOG` - the same variable every other `tracing` setup in the
    ///   estate already reads, so turning this crate on changes no habits
    /// - `LOG_DIR`, `LOG_FILE_PREFIX` (falls back to `CARGO_BIN_NAME`, which
    ///   Cargo sets for the binary a workspace member built - see
    ///   `resolve_file_prefix`)
    /// - `LOG_FILE_FORMAT` (`json`/`plain`), `LOG_ROTATION`
    ///   (`daily`/`hourly`/`never`), `LOG_MAX_FILES`
    pub fn from_env() -> Self {
        let default = Self::default();
        Self {
            console_enabled: env_bool("LOG_CONSOLE_ENABLED", default.console_enabled),
            file_enabled: env_bool("LOG_FILE_ENABLED", default.file_enabled),
            level: non_empty_env("RUST_LOG").unwrap_or(default.level),
            file_dir: non_empty_env("LOG_DIR")
                .map(PathBuf::from)
                .unwrap_or(default.file_dir),
            file_prefix: resolve_file_prefix(
                non_empty_env("LOG_FILE_PREFIX").as_deref(),
                non_empty_env("CARGO_BIN_NAME").as_deref(),
            ),
            file_format: non_empty_env("LOG_FILE_FORMAT")
                .as_deref()
                .and_then(Format::parse)
                .unwrap_or(default.file_format),
            rotation: non_empty_env("LOG_ROTATION")
                .as_deref()
                .and_then(Rotation::parse)
                .unwrap_or(default.rotation),
            max_files: non_empty_env("LOG_MAX_FILES")
                .and_then(|v| v.parse().ok())
                .unwrap_or(default.max_files),
        }
    }
}

/// The file prefix `from_env` falls back through: an explicit
/// `LOG_FILE_PREFIX`, then Cargo's own `CARGO_BIN_NAME` (set for whichever
/// binary of a multi-binary crate is actually running), then a plain
/// `"app"` so a caller that sets neither still gets a valid file name
/// instead of an empty one.
pub fn resolve_file_prefix(explicit: Option<&str>, cargo_bin_name: Option<&str>) -> String {
    for name in [explicit, cargo_bin_name].into_iter().flatten() {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "app".to_string()
}

/// `true`/`false` and the handful of spellings every other boolean env var
/// in the estate already accepts.
pub fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn env_bool(key: &str, default: bool) -> bool {
    non_empty_env(key)
        .as_deref()
        .and_then(parse_bool)
        .unwrap_or(default)
}

/// `std::env::var`, treating a set-but-blank value the same as unset - a
/// container that sets `RUST_LOG=""` to "clear" it should fall back to the
/// default, not hand an empty directive string to `EnvFilter`.
fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}
