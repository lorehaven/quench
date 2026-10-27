//! The files of a new project. They are embedded here and rendered with `quench-template`.

use quench_serde::json;
use quench_template::Environment;

use crate::cli::Template;
use crate::error::{Error, Result};
use crate::versions;

/// Which projects get a file.
#[derive(Clone, Copy, PartialEq, Eq)]
enum For {
    All,
    Web,
    Fullstack,
}

struct File {
    path: &'static str,
    body: &'static str,
    only: For,
}

const fn file(path: &'static str, body: &'static str, only: For) -> File {
    File { path, body, only }
}

const FILES: &[File] = &[
    file(
        "quench.toml",
        include_str!("../templates/quench.toml.tpl"),
        For::All,
    ),
    file(
        "Cargo.toml",
        include_str!("../templates/Cargo.toml.tpl"),
        For::All,
    ),
    file(
        ".gitignore",
        include_str!("../templates/gitignore.tpl"),
        For::All,
    ),
    file(
        "README.md",
        include_str!("../templates/README.md.tpl"),
        For::All,
    ),
    file(
        "core/Cargo.toml",
        include_str!("../templates/core/Cargo.toml.tpl"),
        For::All,
    ),
    file(
        "core/src/lib.rs",
        include_str!("../templates/core/src/lib.rs.tpl"),
        For::All,
    ),
    file(
        "core/tests/unit.rs",
        include_str!("../templates/core/tests/unit.rs.tpl"),
        For::All,
    ),
    file(
        "app/Cargo.toml",
        include_str!("../templates/app/Cargo.toml.tpl"),
        For::All,
    ),
    file(
        "app/src/lib.rs",
        include_str!("../templates/app/src/lib.rs.tpl"),
        For::Web,
    ),
    file(
        "app/src/lib.rs",
        include_str!("../templates/app/src/lib_fullstack.rs.tpl"),
        For::Fullstack,
    ),
    file(
        "tests/browser.mjs",
        include_str!("../templates/tests/browser.mjs.tpl"),
        For::All,
    ),
    file(
        "server/Cargo.toml",
        include_str!("../templates/server/Cargo.toml.tpl"),
        For::All,
    ),
    file(
        "server/src/main.rs",
        include_str!("../templates/server/src/main.rs.tpl"),
        For::All,
    ),
    file(
        "shared/Cargo.toml",
        include_str!("../templates/shared/Cargo.toml.tpl"),
        For::Fullstack,
    ),
    file(
        "shared/src/lib.rs",
        include_str!("../templates/shared/src/lib.rs.tpl"),
        For::Fullstack,
    ),
];

const CARGO_CONFIG: &str = include_str!("../templates/cargo-config.toml.tpl");

/// Every file of a project called `name`: its path under the project directory and its text.
/// `quench_path` makes the quench crates come from a checkout instead of the registry.
///
/// # Errors
/// When a template does not render (a bug in this tool, caught by its tests).
pub fn render(
    name: &str,
    template: Template,
    quench_path: Option<&str>,
) -> Result<Vec<(String, String)>> {
    let context = json!({
        "name": name,
        "snake": name.replace('-', "_"),
        "template": template.name(),
        "fullstack": template == Template::Fullstack,
        "deps": versions::dependency_lines(quench_path),
        "registry": versions::REGISTRY,
        "index": versions::REGISTRY_INDEX,
    });
    let mut env = Environment::new();
    let mut out = Vec::new();
    let mut add = |path: &str, body: &str| -> Result<()> {
        env.add_template(path, body)
            .map_err(|e| Error(format!("template {path}: {e}")))?;
        let text = env
            .render(path, &context)
            .map_err(|e| Error(format!("template {path}: {e}")))?;
        out.push((path.to_string(), text));
        Ok(())
    };
    for f in FILES {
        let wanted = match f.only {
            For::All => true,
            For::Web => template == Template::Web,
            For::Fullstack => template == Template::Fullstack,
        };
        if wanted {
            add(f.path, f.body)?;
        }
    }
    if quench_path.is_none() {
        add(".cargo/config.toml", CARGO_CONFIG)?;
    }
    Ok(out)
}
