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
    },
    /// Build the app and the server
    Build {
        /// Size-optimised wasm and an optimised server with the wasm built in
        #[arg(long)]
        release: bool,
        /// With `--release`: keep the wasm as a file next to the server instead of building it in
        #[arg(long)]
        no_embed: bool,
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
