//! A quench project: a workspace whose root holds `.quench.toml`.

use std::path::{Path, PathBuf};

use quench_serde::Deserialize;

use crate::cli::Template;
use crate::error::{Result, fail};

pub const MANIFEST: &str = ".quench.toml";

/// The `[android]` table of `.quench.toml`: what the build needs to know.
#[derive(Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AndroidConfig {
    /// The application id (`quench android init --package` writes it).
    pub package: Option<String>,
    /// What people see. Left out, it is the project's Cargo version (see [`crate::linux::version`]).
    pub version_name: Option<String>,
    /// Must go up with every release. Left out, it is derived from the version: `major * 1_000_000 + minor * 1_000 + patch`
    /// (see [`crate::linux::version_code`]), so one number to bump gives every platform the same release.
    pub version_code: Option<u32>,
    /// The release keystore (a path from the project root). The passwords are never here: they are
    /// read from `QUENCH_KEYSTORE_PASSWORD` and `QUENCH_KEY_PASSWORD`.
    pub keystore: Option<String>,
    /// The key in the keystore.
    pub key_alias: Option<String>,
}

/// The `[linux]` table of `.quench.toml`.
#[derive(Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct LinuxConfig {
    /// The application id the keyring, notifications and the menu entry know the app by.
    pub id: Option<String>,
    /// The window's title (default: the project's name).
    pub title: Option<String>,
    /// The name of the host's binary when it is not the crate's (`[[bin]] name` in `desktop/Cargo.toml`).
    pub bin: Option<String>,
}

#[derive(Deserialize)]
struct Manifest {
    name: String,
    template: String,
    android: Option<AndroidConfig>,
    linux: Option<LinuxConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub root: PathBuf,
    pub name: String,
    pub template: Template,
    pub android: AndroidConfig,
    pub linux: LinuxConfig,
}

/// Whether `name` can be a project name: it becomes a directory and the prefix of four crate names.
///
/// # Errors
/// The reason it cannot.
pub fn check_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return fail("the project needs a name");
    };
    if !first.is_ascii_lowercase() {
        return fail(format!("`{name}` must start with a lowercase letter"));
    }
    if let Some(bad) =
        chars.find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '_'))
    {
        return fail(format!(
            "`{name}` has `{bad}` in it; use lowercase letters, digits, `-` and `_`"
        ));
    }
    if name.len() > 40 {
        return fail("the name is longer than 40 characters");
    }
    if name.starts_with("quench") {
        return fail(
            "the name cannot start with `quench`: it would be taken for one of the framework's crates",
        );
    }
    Ok(())
}

impl Project {
    /// The project that contains `start`, found by looking for `.quench.toml` upwards.
    ///
    /// # Errors
    /// When there is none, or its manifest cannot be read.
    pub fn find(start: &Path) -> Result<Self> {
        let mut dir = Some(start);
        while let Some(d) = dir {
            let file = d.join(MANIFEST);
            if file.is_file() {
                return Self::read(d);
            }
            dir = d.parent();
        }
        fail(format!(
            "no {MANIFEST} in {} or above it: run this inside a project made by `quench new`",
            start.display()
        ))
    }

    fn read(root: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(root.join(MANIFEST))?;
        let manifest: Manifest = quench_serde::toml::from_str(&text)
            .map_err(|e| crate::error::Error(format!("{MANIFEST}: {e}")))?;
        let Some(template) = Template::from_name(&manifest.template) else {
            return fail(format!(
                "{MANIFEST}: unknown template `{}` (web or fullstack)",
                manifest.template
            ));
        };
        check_name(&manifest.name).map_err(|e| crate::error::Error(format!("{MANIFEST}: {e}")))?;
        Ok(Self {
            root: root.to_path_buf(),
            name: manifest.name,
            template,
            android: manifest.android.unwrap_or_default(),
            linux: manifest.linux.unwrap_or_default(),
        })
    }

    /// The name as a Rust identifier.
    #[must_use]
    pub fn snake(&self) -> String {
        self.name.replace('-', "_")
    }

    #[must_use]
    pub fn app_package(&self) -> String {
        format!("{}-app", self.name)
    }

    #[must_use]
    pub fn server_package(&self) -> String {
        format!("{}-server", self.name)
    }

    /// Where cargo puts the module built with `profile`.
    #[must_use]
    pub fn wasm_path(&self, profile: &str) -> PathBuf {
        self.target_dir()
            .join("wasm32-unknown-unknown")
            .join(profile)
            .join(format!("{}_app.wasm", self.snake()))
    }

    /// Where cargo puts the server binary built with `profile` (`debug` or `release`).
    #[must_use]
    pub fn server_path(&self, profile: &str) -> PathBuf {
        self.target_dir().join(profile).join(self.server_package())
    }

    #[must_use]
    pub fn target_dir(&self) -> PathBuf {
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(
            || self.root.join("target"),
            |dir| {
                let dir = PathBuf::from(dir);
                if dir.is_absolute() {
                    dir
                } else {
                    self.root.join(dir)
                }
            },
        )
    }

    /// Where `quench build --release` leaves its output.
    #[must_use]
    pub fn dist(&self) -> PathBuf {
        self.root.join("dist")
    }
}
