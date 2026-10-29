//! The Linux desktop host: `quench linux init`, `quench build linux`, `quench run linux` and the `.deb`.
//!
//! The host is a small crate, `desktop/`, whose binary shows the project's bundle in a window
//! (`quench-desktop`). Its build script embeds `dist/www`, so building the host is: make the bundle,
//! then build the crate.

use std::io::Write;
use std::path::{Path, PathBuf};

use quench_serde::json;
use quench_template::Environment;

use crate::error::{Error, Result, fail};
use crate::project::Project;
use crate::system::{Cmd, Process, System};
use crate::{release, versions};

const CARGO: &str = include_str!("../templates/linux/Cargo.toml.tpl");
const BUILD: &str = include_str!("../templates/linux/build.rs");
const MAIN: &str = include_str!("../templates/linux/main.rs.tpl");

/// The id the app is known by to the keyring and the desktop (`com.example.<name>` unless chosen).
#[must_use]
pub fn app_id(project: &Project) -> String {
    project
        .linux
        .id
        .clone()
        .unwrap_or_else(|| format!("com.example.{}", project.snake()))
}

/// The window title: the name chosen in `[linux]`, else the project's.
#[must_use]
pub fn title(project: &Project) -> String {
    project
        .linux
        .title
        .clone()
        .unwrap_or_else(|| project.name.clone())
}

/// `<name>-desktop`, the host's package and binary.
#[must_use]
pub fn package(project: &Project) -> String {
    format!("{}-desktop", project.name)
}

fn check_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 100
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        && !id.starts_with(['.', '-']);
    if ok {
        Ok(())
    } else {
        fail(format!(
            "`{id}` is not an application id: use letters, digits, `.`, `-` and `_` (com.example.notes)"
        ))
    }
}

/// `quench linux init`: writes `desktop/` and joins it to the project's workspace (a project with no
/// workspace gets a crate of its own, depending on `quench_path`'s checkout or the registry).
///
/// # Errors
/// A bad id, a `desktop/` that exists already, or a file that cannot be written.
pub fn init(
    out: &mut dyn Write,
    project: &Project,
    id: Option<&str>,
    quench_path: Option<&str>,
) -> Result<()> {
    let id = id.map_or_else(|| app_id(project), str::to_string);
    check_id(&id)?;
    let root = project.root.join("desktop");
    if root.exists() {
        return fail(format!("{} exists already", root.display()));
    }
    let cargo = project.workspace_manifest();
    let workspace = std::fs::read_to_string(&cargo).ok();
    let standalone = workspace.is_none();
    let dependency = versions::dependency_lines(quench_path);
    let quench_desktop = dependency
        .lines()
        .find(|l| l.starts_with("quench-desktop = "))
        .unwrap_or_default()
        // The window is off by default (it needs GTK to link); the host crate is the one that wants it.
        .replacen(" }", ", features = [\"window\"] }", 1);
    let context = json!({
        "name": project.name, "title": title(project), "id": id,
        "standalone": standalone, "flat": project.flat(), "quench_desktop": quench_desktop,
    });
    let mut env = Environment::new();
    for (name, text) in [("Cargo.toml", CARGO), ("src/main.rs", MAIN)] {
        env.add_template(name, text)
            .map_err(|e| Error(format!("template {name}: {e}")))?;
    }
    std::fs::create_dir_all(root.join("src"))?;
    for name in ["Cargo.toml", "src/main.rs"] {
        let text = env
            .render(name, &context)
            .map_err(|e| Error(format!("template {name}: {e}")))?;
        std::fs::write(root.join(name), text)?;
    }
    std::fs::write(root.join("build.rs"), BUILD)?;
    if let Some(text) = workspace {
        let text = crate::plugins::ensure_dependency(&text, "quench-desktop")?;
        let member = project.member_path("desktop");
        let text = if text.contains(&format!("\"{member}\"")) {
            text
        } else {
            text.replacen("members = [", &format!("members = [\"{member}\", "), 1)
        };
        std::fs::write(&cargo, text)?;
    }
    if standalone {
        // A crate of its own builds into its own `target/`, which the project's `.gitignore` does not know.
        let ignore = project.root.join(".gitignore");
        let text = std::fs::read_to_string(&ignore).unwrap_or_default();
        if !text.lines().any(|l| l.trim() == "/desktop/target") {
            let sep = if text.is_empty() || text.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            std::fs::write(&ignore, format!("{text}{sep}/desktop/target\n"))?;
        }
    }
    // What the build and the package read.
    let manifest = project.root.join(crate::project::MANIFEST);
    let text = std::fs::read_to_string(&manifest)?;
    if !text.contains("[linux]") {
        std::fs::write(&manifest, format!("{text}\n[linux]\nid = \"{id}\"\n"))?;
    }
    writeln!(out, "created {} ({id})", root.display())?;
    Ok(())
}

fn require_desktop(project: &Project) -> Result<()> {
    if project.root.join("desktop/Cargo.toml").is_file() {
        Ok(())
    } else {
        fail("this project has no desktop/ yet: run `quench linux init`")
    }
}

/// Whether the host is a crate of its own (`quench linux init` found no workspace to join).
fn standalone(project: &Project) -> bool {
    std::fs::read_to_string(project.root.join("desktop/Cargo.toml"))
        .is_ok_and(|text| text.contains("\n[workspace]"))
}

/// Where cargo puts the host's binary built with `profile` (`debug` or `release`).
fn binary(project: &Project, profile: &str) -> PathBuf {
    let target = if standalone(project) {
        // A crate of its own builds into its own `target/`, unless `CARGO_TARGET_DIR` says otherwise.
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(
            || project.root.join("desktop/target"),
            |dir| project.root.join("desktop").join(dir),
        )
    } else {
        project.target_dir()
    };
    target.join(profile).join(
        project
            .linux
            .bin
            .clone()
            .unwrap_or_else(|| package(project)),
    )
}

/// `quench build linux`: the bundle, then the host. Returns the binary, copied to `dist/`.
///
/// # Errors
/// No `desktop/`, or the first step that fails.
pub fn build(
    sys: &dyn System,
    out: &mut dyn Write,
    project: &Project,
    release_build: bool,
) -> Result<PathBuf> {
    require_desktop(project)?;
    release::bundle(sys, out, project)?;
    let profile = if release_build { "release" } else { "debug" };
    let alone = standalone(project);
    let (dir, mut args) = if alone {
        (project.root.join("desktop"), vec!["build"])
    } else {
        (project.root.clone(), vec!["build", "-p"])
    };
    let name = package(project);
    if !alone {
        args.push(&name);
    }
    if release_build {
        args.push("--release");
    }
    release::step(sys, out, &Cmd::new("cargo", &args, &dir))?;
    let built = binary(project, profile);
    let target = project.dist().join(format!("{}-linux", project.name));
    std::fs::create_dir_all(project.dist())?;
    std::fs::copy(&built, &target).map_err(|e| Error(format!("{}: {e}", built.display())))?;
    writeln!(out, "linux {}", target.display())?;
    Ok(target)
}

/// `quench run linux`: builds the host and shows the app.
///
/// # Errors
/// As [`build`], or the window failing to open.
pub fn run(sys: &dyn System, out: &mut dyn Write, project: &Project) -> Result<()> {
    let built = build(sys, out, project, false)?;
    let code = sys
        .run(&Cmd::new(&built.to_string_lossy(), &[], &project.root))
        .map_err(|e| Error(format!("cannot start {}: {e}", built.display())))?;
    if code != 0 {
        return fail(format!("the app stopped with exit code {code}"));
    }
    Ok(())
}

/// `quench dev --linux`: waits for the dev server, builds the host and opens its window on it. The
/// window stays across the server's restarts (the page reloads itself).
///
/// # Errors
/// Never: a host that cannot start is reported and the dev loop goes on without it.
pub fn launch_dev(
    sys: &dyn System,
    out: &mut dyn Write,
    project: &Project,
    port: u16,
) -> Result<Option<Box<dyn Process>>> {
    let mut attempt = || -> Result<Box<dyn Process>> {
        require_desktop(project)?;
        if !sys.wait_port(port, 15_000) {
            return fail(format!("the server did not start listening on port {port}"));
        }
        let built = build(sys, out, project, false)?;
        sys.spawn(
            &Cmd::new(&built.to_string_lossy(), &[], &project.root)
                .env("QUENCH_DEV_URL", &format!("http://127.0.0.1:{port}")),
        )
        .map_err(|e| Error(format!("cannot start {}: {e}", built.display())))
    };
    match attempt() {
        Ok(process) => Ok(Some(process)),
        Err(e) => {
            writeln!(out, "the desktop window did not start: {e}")?;
            Ok(None)
        }
    }
}

/// The version of the project: the workspace's, else the host crate's.
#[must_use]
pub fn version(project: &Project) -> String {
    let find = |path: &Path, section: &str| -> Option<String> {
        let text = std::fs::read_to_string(path).ok()?;
        let after = &text[text.find(section)? + section.len()..];
        let line = after
            .lines()
            .take_while(|l| !l.starts_with('['))
            .find(|l| l.starts_with("version = "))?;
        Some(
            line.trim_start_matches("version = ")
                .trim_matches('"')
                .to_string(),
        )
    };
    find(&project.root.join("Cargo.toml"), "[workspace.package]")
        .or_else(|| find(&project.root.join("desktop/Cargo.toml"), "[package]"))
        .unwrap_or_else(|| "0.1.0".to_string())
}

/// The integer a store wants for a release: `0.1.3` is `1003`, `1.2.3` is `1002003`.
///
/// Every platform's build of one version gets the same number, so the tool that publishes them can derive it too.
/// A part over 999 is capped (a version that big has no room in the scheme); anything that is not `major.minor.patch`
/// counts as `0.0.0`.
#[must_use]
pub fn version_code(version: &str) -> u32 {
    let mut parts = version
        .split(['.', '-', '+'])
        .map(|p| p.parse::<u32>().unwrap_or(0));
    let (major, minor, patch) = (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0).min(999),
        parts.next().unwrap_or(0).min(999),
    );
    major
        .saturating_mul(1_000_000)
        .saturating_add(minor * 1_000 + patch)
}

/// One member of an `ar` archive (what a `.deb` is made of).
fn ar_member(out: &mut Vec<u8>, name: &str, data: &[u8]) {
    // name, mtime, owner, group, mode (octal), size, then the two bytes that end a header.
    let header = format!(
        "{name:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
        0,
        0,
        0,
        100_644,
        data.len()
    );
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(b'\n');
    }
}

/// The icon the packages install, if `[linux] icon` names one: `(install path, bytes)`, filed under the size the PNG
/// says it is in the `hicolor` theme.
///
/// # Errors
/// The file is missing, is not a PNG, or is not square.
pub fn icon(project: &Project) -> Result<Option<(String, Vec<u8>)>> {
    let Some(name) = &project.linux.icon else {
        return Ok(None);
    };
    let path = project.root.join(name);
    let bytes =
        std::fs::read(&path).map_err(|e| Error(format!("[linux] icon {}: {e}", path.display())))?;
    if bytes.len() < 24 || bytes[..8] != [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a] {
        return fail(format!("[linux] icon {name} is not a PNG"));
    }
    let side =
        |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let (width, height) = (side(16), side(20));
    if width != height {
        return fail(format!(
            "[linux] icon {name} is {width}x{height}: it must be square"
        ));
    }
    Ok(Some((
        format!(
            "/usr/share/icons/hicolor/{width}x{width}/apps/{}.png",
            app_id(project)
        ),
        bytes,
    )))
}

/// The `.desktop` entry that puts the app in the menus.
#[must_use]
pub fn desktop_entry(project: &Project) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={}\nExec=/usr/bin/{}\nTerminal=false\nCategories=Utility;\nStartupWMClass={}\n{}",
        title(project),
        project.name,
        project.linux.wm_class.as_deref().unwrap_or(&project.name),
        if project.linux.icon.is_some() {
            format!("Icon={}\n", app_id(project))
        } else {
            String::new()
        }
    )
}

/// `quench build deb`: the release host as `dist/<name>_<version>_amd64.deb`, which installs the binary
/// to `/usr/bin` and an entry to the application menu.
///
/// # Errors
/// As [`build`], or a file that cannot be written.
pub fn deb(sys: &dyn System, out: &mut dyn Write, project: &Project) -> Result<PathBuf> {
    let built = build(sys, out, project, true)?;
    let binary = std::fs::read(&built)?;
    let version = version(project);
    let name = &project.name;

    let mut data = quench_archive::tar::Builder::new();
    for dir in [
        "./usr/",
        "./usr/bin/",
        "./usr/share/",
        "./usr/share/applications/",
    ] {
        data.append(&quench_archive::tar::Header::directory(dir), &[])
            .map_err(|e| Error(e.to_string()))?;
    }
    let mut exe = quench_archive::tar::Header::file(&format!("./usr/bin/{name}"));
    exe.mode = 0o755;
    data.append(&exe, &binary)
        .map_err(|e| Error(e.to_string()))?;
    data.append(
        &quench_archive::tar::Header::file(&format!(
            "./usr/share/applications/{}.desktop",
            app_id(project)
        )),
        desktop_entry(project).as_bytes(),
    )
    .map_err(|e| Error(e.to_string()))?;
    if let Some((path, bytes)) = icon(project)? {
        // The directories above the icon, then the icon.
        let relative = path.trim_start_matches('/');
        let mut dir = String::from(".");
        for part in relative.rsplit_once('/').map_or("", |p| p.0).split('/') {
            dir = format!("{dir}/{part}");
            if matches!(dir.as_str(), "./usr" | "./usr/share") {
                continue;
            }
            data.append(
                &quench_archive::tar::Header::directory(&format!("{dir}/")),
                &[],
            )
            .map_err(|e| Error(e.to_string()))?;
        }
        data.append(
            &quench_archive::tar::Header::file(&format!("./{relative}")),
            &bytes,
        )
        .map_err(|e| Error(e.to_string()))?;
    }

    let size_kib = binary.len().div_ceil(1024);
    let control_text = format!(
        "Package: {name}\nVersion: {version}\nArchitecture: amd64\nMaintainer: {name} <noreply@example.com>\nInstalled-Size: {size_kib}\n{}Section: utils\nPriority: optional\nDescription: {}\n",
        match project.linux.depends.as_deref() {
            None => "Depends: libwebkit2gtk-4.1-0, libgtk-3-0\n".to_string(),
            Some("") => String::new(),
            Some(depends) => format!("Depends: {depends}\n"),
        },
        title(project)
    );
    let mut control = quench_archive::tar::Builder::new();
    control
        .append_file("./control", control_text.as_bytes())
        .map_err(|e| Error(e.to_string()))?;

    let mut archive = b"!<arch>\n".to_vec();
    ar_member(&mut archive, "debian-binary", b"2.0\n");
    ar_member(
        &mut archive,
        "control.tar.gz",
        &quench_archive::gzip::compress(&control.finish()),
    );
    ar_member(
        &mut archive,
        "data.tar.gz",
        &quench_archive::gzip::compress(&data.finish()),
    );
    let target = project.dist().join(format!("{name}_{version}_amd64.deb"));
    std::fs::write(&target, archive)?;
    writeln!(out, "deb {}", target.display())?;
    Ok(target)
}
