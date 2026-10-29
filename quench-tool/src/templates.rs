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
        ".quench.toml",
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

/// One file of the Android host project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFile {
    pub path: String,
    pub bytes: Vec<u8>,
    pub executable: bool,
}

struct HostTemplate {
    path: &'static str,
    body: &'static [u8],
    /// Text files are rendered; the Gradle wrapper's jar and scripts are copied as they are.
    render: bool,
    executable: bool,
}

const fn host(path: &'static str, body: &'static [u8]) -> HostTemplate {
    HostTemplate {
        path,
        body,
        render: true,
        executable: false,
    }
}

const fn verbatim(path: &'static str, body: &'static [u8], executable: bool) -> HostTemplate {
    HostTemplate {
        path,
        body,
        render: false,
        executable,
    }
}

const HOST_FILES: &[HostTemplate] = &[
    host(
        ".gitignore",
        include_bytes!("../templates/android/.gitignore.tpl"),
    ),
    host(
        "settings.gradle.kts",
        include_bytes!("../templates/android/settings.gradle.kts.tpl"),
    ),
    host(
        "build.gradle.kts",
        include_bytes!("../templates/android/build.gradle.kts.tpl"),
    ),
    host(
        "gradle.properties",
        include_bytes!("../templates/android/gradle.properties.tpl"),
    ),
    host(
        "app/build.gradle.kts",
        include_bytes!("../templates/android/app/build.gradle.kts.tpl"),
    ),
    host(
        "app/src/main/AndroidManifest.xml",
        include_bytes!("../templates/android/app/src/main/AndroidManifest.xml.tpl"),
    ),
    host(
        "app/src/main/res/values/themes.xml",
        include_bytes!("../templates/android/app/src/main/res/values/themes.xml.tpl"),
    ),
    host(
        "app/src/main/res/xml/network_security_config.xml",
        include_bytes!("../templates/android/app/src/main/res/xml/network_security_config.xml.tpl"),
    ),
    host(
        "app/src/debug/res/xml/network_security_config.xml",
        include_bytes!(
            "../templates/android/app/src/debug/res/xml/network_security_config.xml.tpl"
        ),
    ),
    host(
        "app/src/main/kotlin/@package_path@/MainActivity.kt",
        include_bytes!("../templates/android/app/src/main/kotlin/MainActivity.kt.tpl"),
    ),
    host(
        "app/src/main/kotlin/@package_path@/Bridge.kt",
        include_bytes!("../templates/android/app/src/main/kotlin/Bridge.kt.tpl"),
    ),
    host(
        "app/src/main/kotlin/@package_path@/Plugin.kt",
        include_bytes!("../templates/android/app/src/main/kotlin/Plugin.kt.tpl"),
    ),
    host(
        "app/src/main/kotlin/@package_path@/Plugins.kt",
        include_bytes!("../templates/android/app/src/main/kotlin/Plugins.kt.tpl"),
    ),
    host(
        "app/src/main/kotlin/@package_path@/EchoPlugin.kt",
        include_bytes!("../templates/android/app/src/main/kotlin/EchoPlugin.kt.tpl"),
    ),
    host(
        "app/src/debug/AndroidManifest.xml",
        include_bytes!("../templates/android/app/src/debug/AndroidManifest.xml.tpl"),
    ),
    host(
        "app/src/main/kotlin/@package_path@/AppAssets.kt",
        include_bytes!("../templates/android/app/src/main/kotlin/AppAssets.kt.tpl"),
    ),
    verbatim(
        "gradlew",
        include_bytes!("../templates/android/gradlew"),
        true,
    ),
    verbatim(
        "gradlew.bat",
        include_bytes!("../templates/android/gradlew.bat"),
        false,
    ),
    verbatim(
        "gradle/wrapper/gradle-wrapper.jar",
        include_bytes!("../templates/android/gradle/wrapper/gradle-wrapper.jar"),
        false,
    ),
    verbatim(
        "gradle/wrapper/gradle-wrapper.properties",
        include_bytes!("../templates/android/gradle/wrapper/gradle-wrapper.properties"),
        false,
    ),
];

/// Whether `package` is an Android application id: two or more dot-separated parts, each a
/// lowercase letter followed by lowercase letters, digits and `_`.
///
/// # Errors
/// What is wrong with it.
pub fn check_package(package: &str) -> Result<()> {
    let parts: Vec<&str> = package.split('.').collect();
    let ok = parts.len() >= 2
        && parts.iter().all(|p| {
            p.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        });
    if ok {
        Ok(())
    } else {
        Err(Error(format!(
            "`{package}` is not a package name: use at least two lowercase parts joined by dots, such as com.example.app"
        )))
    }
}

/// The files of the Android host for the project `name`, with application id `package`.
///
/// # Errors
/// A bad package name, or a template that does not render (a bug caught by this tool's tests).
pub fn render_android(name: &str, package: &str) -> Result<Vec<HostFile>> {
    check_package(package)?;
    let context = json!({ "name": name, "package": package });
    let package_path = package.replace('.', "/");
    let mut env = Environment::new();
    let mut out = Vec::new();
    for f in HOST_FILES {
        let path = f.path.replace("@package_path@", &package_path);
        let bytes = if f.render {
            let text =
                std::str::from_utf8(f.body).map_err(|e| Error(format!("template {path}: {e}")))?;
            env.add_template(&path, text)
                .map_err(|e| Error(format!("template {path}: {e}")))?;
            env.render(&path, &context)
                .map_err(|e| Error(format!("template {path}: {e}")))?
                .into_bytes()
        } else {
            f.body.to_vec()
        };
        out.push(HostFile {
            path,
            bytes,
            executable: f.executable,
        });
    }
    Ok(out)
}
