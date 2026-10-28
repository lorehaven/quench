//! `quench build`: the app for the browser, then the server, and for `--release` a `dist/` directory
//! holding the module and a server binary that has it built in.

use std::io::Write;

use crate::error::{Result, fail};
use crate::project::Project;
use crate::system::{Cmd, System};

const WASM_TARGET: &str = "wasm32-unknown-unknown";

/// Runs `cmd`, saying what it is, and turns a failure into an error.
pub fn step(sys: &dyn System, out: &mut dyn Write, cmd: &Cmd) -> Result<()> {
    writeln!(out, "$ {}", cmd.line())?;
    match sys.run(cmd) {
        Ok(0) => Ok(()),
        Ok(code) => fail(format!("`{}` failed (exit {code})", cmd.line())),
        Err(e) => fail(format!("cannot run `{}`: {e}", cmd.program)),
    }
}

/// The cargo profile for the module: slow and small for a release, quick otherwise.
#[must_use]
pub const fn wasm_profile(release: bool) -> &'static str {
    if release { "wasm" } else { "wasm-dev" }
}

pub(crate) fn build_wasm(
    sys: &dyn System,
    out: &mut dyn Write,
    project: &Project,
    release: bool,
) -> Result<()> {
    let app = project.app_package();
    step(
        sys,
        out,
        &Cmd::new(
            "cargo",
            &[
                "build",
                "--target",
                WASM_TARGET,
                "--profile",
                wasm_profile(release),
                "-p",
                &app,
            ],
            &project.root,
        ),
    )
}

pub(crate) fn build_server(
    sys: &dyn System,
    out: &mut dyn Write,
    project: &Project,
    release: bool,
    embed: bool,
) -> Result<()> {
    let server = project.server_package();
    let mut args = vec!["build", "-p", &server];
    if release {
        args.push("--release");
    }
    if embed {
        args.extend(["--features", "embed"]);
    }
    step(sys, out, &Cmd::new("cargo", &args, &project.root))
}

/// `quench build`.
///
/// # Errors
/// The first step that fails.
pub fn build(
    sys: &dyn System,
    out: &mut dyn Write,
    project: &Project,
    release: bool,
    embed: bool,
) -> Result<()> {
    build_wasm(sys, out, project, release)?;
    if !release {
        build_server(sys, out, project, false, false)?;
        writeln!(out, "built {}", project.server_path("debug").display())?;
        return Ok(());
    }
    let dist = project.dist();
    std::fs::create_dir_all(&dist)?;
    let module = dist.join("app.wasm");
    let built = project.wasm_path(wasm_profile(true));
    std::fs::copy(&built, &module)
        .map_err(|e| crate::error::Error(format!("{}: {e}", built.display())))?;
    if sys.on_path("wasm-opt") {
        let path = module.to_string_lossy().into_owned();
        step(
            sys,
            out,
            &Cmd::new("wasm-opt", &["-Oz", "-o", &path, &path], &project.root),
        )?;
    } else {
        writeln!(out, "wasm-opt not found: the module is as cargo built it")?;
    }
    build_server(sys, out, project, true, embed)?;
    let binary = dist.join(project.server_package());
    let compiled = project.server_path("release");
    std::fs::copy(&compiled, &binary)
        .map_err(|e| crate::error::Error(format!("{}: {e}", compiled.display())))?;
    let size = |p: &std::path::Path| std::fs::metadata(p).map_or(0, |m| m.len());
    writeln!(
        out,
        "module  {} ({} KiB)",
        module.display(),
        size(&module) / 1024
    )?;
    writeln!(
        out,
        "server  {} ({} KiB){}",
        binary.display(),
        size(&binary) / 1024,
        if embed {
            ", module built in"
        } else {
            ", reads the module from WASM or dist/app.wasm"
        }
    )?;
    Ok(())
}

/// `quench build --bundle`: the app as a static bundle in `dist/www`, which the Android host serves.
///
/// The bundle is the page and the hashed assets. The project's server writes it: it is the one
/// place that knows the page and its stylesheets.
///
/// # Errors
/// The first step that fails.
pub fn bundle(sys: &dyn System, out: &mut dyn Write, project: &Project) -> Result<()> {
    build_wasm(sys, out, project, true)?;
    build_server(sys, out, project, false, false)?;
    let target = project.dist().join("www");
    // A bundle that is rebuilt must not keep files of the last one: their names are hashes.
    if target.exists() {
        std::fs::remove_dir_all(&target)?;
    }
    let binary = project.server_path("debug");
    step(
        sys,
        out,
        &Cmd::new(&binary.to_string_lossy(), &[], &project.root)
            .env("QUENCH_EXPORT", &target.to_string_lossy())
            .env(
                "WASM",
                &project.wasm_path(wasm_profile(true)).to_string_lossy(),
            ),
    )?;
    writeln!(out, "bundle {}", target.display())?;
    Ok(())
}
