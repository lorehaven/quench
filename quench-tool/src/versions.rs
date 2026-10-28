//! The quench crates a new project depends on, and the oldest version of each that works with this
//! tool's templates. A test keeps the table in step with the workspace (same minor version).

use std::fmt::Write as _;

pub const CRATES: &[(&str, &str)] = &[
    ("quench-android", "0.1.0"),
    ("quench-core", "0.1.6"),
    ("quench-desktop", "0.1.0"),
    ("quench-fullstack", "0.1.3"),
    ("quench-http", "0.1.14"),
    ("quench-serde", "0.1.6"),
    ("quench-test", "0.1.2"),
    ("quench-ui", "0.1.1"),
    ("quench-ui-web", "0.1.3"),
];

/// The registry the quench crates are published to.
pub const REGISTRY: &str = "ennor";
pub const REGISTRY_INDEX: &str = "sparse+https://ennor.ddns.net/warehouse/index/";

/// The `[workspace.dependencies]` lines for the quench crates: from the registry, or from a
/// checkout of the repository at `path`.
#[must_use]
pub fn dependency_lines(path: Option<&str>) -> String {
    let mut out = String::new();
    for (name, version) in CRATES {
        let _ = match path {
            Some(root) => writeln!(
                out,
                "{name} = {{ path = \"{}/{name}\" }}",
                root.trim_end_matches('/')
            ),
            None => writeln!(
                out,
                "{name} = {{ registry = \"{REGISTRY}\", version = \"{version}\" }}"
            ),
        };
    }
    out
}
