//! The command line: what `quench --help` lists.

use quench_args::{Parser, Subcommand, ValueEnum};

/// The project shapes `quench new` knows.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Template {
    /// A quench-ui app and a server that serves it.
    Web,
    /// As `web`, plus a `shared` crate whose `#[server]` functions the app calls.
    Fullstack,
}

impl Template {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Fullstack => "fullstack",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "web" => Some(Self::Web),
            "fullstack" => Some(Self::Fullstack),
            _ => None,
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "quench",
    version,
    about = "Build quench-ui apps: new projects, a dev loop with live reload, release builds"
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Create a project: `app/` (UI), `core/` (logic), `server/` and, for fullstack, `shared/`
    New {
        /// The project's name: lowercase letters, digits, `-` and `_`
        name: String,
        #[arg(long, value_enum, default_value_t = Template::Fullstack)]
        template: Template,
        /// Where to create it (default: a directory called after the project)
        #[arg(long, value_name = "DIR")]
        dir: Option<String>,
        /// Depend on a checkout of the quench repository instead of the registry
        #[arg(long, value_name = "DIR")]
        quench_path: Option<String>,
    },
    /// Build, serve and reload the browser when a source file changes
    Dev {
        /// The port the server listens on
        #[arg(long, default_value_t = 3000)]
        port: u16,
        /// Open the page in the default browser once the server is up
        #[arg(long)]
        open: bool,
        /// Also start the app on the attached Android device, loading this server
        #[arg(long)]
        android: bool,
        /// Also open the desktop window (`desktop/`) on this server
        #[arg(long)]
        linux: bool,
    },
    /// Build the app and the server
    Build {
        /// Size-optimised wasm and an optimised server with the wasm built in
        #[arg(long)]
        release: bool,
        /// With `--release`: keep the wasm as a file next to the server instead of building it in
        #[arg(long)]
        no_embed: bool,
        /// Instead: write the app as a static bundle in `dist/www`, for the Android host to serve
        #[arg(long)]
        bundle: bool,
        /// Build a package instead: `apk` (debug, or signed with --release) or `aab` for Android, `linux`
        /// for the desktop binary, `deb`, `rpm` and `arch` for its packages, `source` for a tarball of the sources
        #[arg(value_enum)]
        target: Option<BuildTarget>,
        /// With `apk` or `aab`: only this ABI's native library (`arm64-v8a` or `x86_64`)
        #[arg(long, value_name = "ABI")]
        abi: Option<String>,
    },
    /// Build the app, install it on the attached device and start it
    Run {
        #[arg(value_enum)]
        target: RunTarget,
        /// The native library's ABI (default: the device's)
        #[arg(long, value_name = "ABI")]
        abi: Option<String>,
    },
    /// The Linux desktop host of the project
    Linux {
        #[command(subcommand)]
        cmd: LinuxCmd,
    },
    /// The Android host of the project
    Android {
        #[command(subcommand)]
        cmd: AndroidCmd,
    },
    /// Native capabilities for the Android host
    Plugin {
        #[command(subcommand)]
        cmd: PluginCmd,
    },
    /// Check this machine for what building and running needs
    Doctor,
    /// Run the project's tests, and check that the app builds for the browser
    Test {
        /// Also run `tests/browser.mjs` against the project's server in headless Chromium
        #[arg(long)]
        browser: bool,
        /// Passed to `cargo test` (a name filter, for instance)
        args: Vec<String>,
    },
}

/// The activities the launcher can start.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum LauncherChoice {
    Web,
    Surface,
}

/// What `quench build` can make besides the web app.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildTarget {
    /// An Android APK
    Apk,
    /// An Android App Bundle, for a store that wants one
    Aab,
    /// The Linux desktop binary, in `dist/`
    Linux,
    /// A Debian package of the Linux desktop binary, in `dist/`
    Deb,
    /// An RPM package of the same
    Rpm,
    /// An Arch Linux package (`.pkg.tar.zst`) of the same
    Arch,
    /// The project's sources as a `.tar.gz` to build from
    Source,
}

/// Where `quench run` runs the app.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunTarget {
    Android,
    Linux,
}

#[derive(Subcommand, Debug)]
pub enum LinuxCmd {
    /// Create `desktop/`: the crate whose binary shows this project's bundle in a window
    Init {
        /// The application id (default: com.example.<name>)
        #[arg(long, value_name = "ID")]
        id: Option<String>,
        /// Take the quench crates from a checkout of the repository (default: the registry)
        #[arg(long, value_name = "DIR")]
        quench_path: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum AndroidCmd {
    /// Make a release key with `keytool` (passwords from `QUENCH_KEYSTORE_PASSWORD` and `QUENCH_KEY_PASSWORD`)
    Keystore {
        /// Where to write it, from the project root
        #[arg(long, value_name = "FILE", default_value = "release.keystore")]
        out: String,
        /// The key's name inside it
        #[arg(long, default_value = "release")]
        alias: String,
    },
    /// Create `android/`: the Gradle project of the host app that serves this project's bundle
    Init {
        /// The application id (default: com.example.<name>)
        #[arg(long, value_name = "ID")]
        package: Option<String>,
    },
    /// Choose which activity the launcher starts: the web app, or the native renderer's `surface`
    Launcher {
        /// `web` (the `WebView` app, `.MainActivity`) or `surface` (the native renderer, `.SurfaceActivity`)
        #[arg(value_enum)]
        which: LauncherChoice,
    },
    /// Build the project's native library (`native/`) for the Android ABIs
    BuildNative {
        /// An optimised build
        #[arg(long)]
        release: bool,
        /// Only this ABI: `arm64-v8a` or `x86_64` (default: both)
        #[arg(long, value_name = "ABI")]
        abi: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum PluginCmd {
    /// Add a plugin (and what it needs) to the Android host
    Add {
        /// Its name; `quench plugin list` has them
        name: String,
    },
    /// Write the host code of the project's plugins again from this tool's templates (after upgrading the crates)
    Update {
        /// One plugin (default: all the project has)
        name: Option<String>,
    },
    /// The plugins there are, and which this project has
    List,
}
