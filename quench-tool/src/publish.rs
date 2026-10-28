//! `quench publish`: sends the signed release APK to the registry the apps are installed from.
//!
//! The registry is the warehouse. The protocol is the one the scripts it replaces used: a bearer
//! token, a `PUT` of the APK to `/warehouse/api/v1/artifacts/<package>/android/<versionCode>`,
//! `201` on success and `409` when that version code was published already (they are immutable).

use std::io::Write;
use std::path::PathBuf;

use quench_serde::{Value, json};

use crate::android;
use crate::error::{Error, Result, fail};
use crate::project::Project;
use crate::system::{Http, System};

/// What `quench publish` was asked.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Build and check everything, send nothing.
    pub dry_run: bool,
    /// Publish this APK instead of building one.
    pub apk: Option<String>,
    /// Publish from a tree with uncommitted changes.
    pub allow_dirty: bool,
}

/// Where to publish: `WAREHOUSE_URL`, else `registry` under `[android]`.
fn server(sys: &dyn System, project: &Project) -> Result<String> {
    sys.var("WAREHOUSE_URL")
        .or_else(|| project.android.registry.clone())
        .map(|s| s.trim_end_matches('/').to_string())
        .ok_or_else(|| {
            Error(
                "no registry: set WAREHOUSE_URL, or `registry` under [android] in quench.toml"
                    .into(),
            )
        })
}

/// A published build must be traceable to a commit: refuse a dirty tree. Returns the commit, if any.
fn commit(sys: &dyn System, project: &Project, allow_dirty: bool) -> Result<Option<String>> {
    let root = project.root.to_string_lossy().into_owned();
    let git = |args: &[&str]| {
        let mut all = vec!["-C", root.as_str()];
        all.extend_from_slice(args);
        sys.capture("git", &all)
    };
    if git(&["rev-parse", "--is-inside-work-tree"]).is_none() {
        return Ok(None);
    }
    let dirty = git(&["status", "--porcelain"]).unwrap_or_default();
    if !dirty.trim().is_empty() && !allow_dirty {
        let shown: Vec<&str> = dirty.lines().take(8).collect();
        return fail(format!(
            "uncommitted changes:\n{}\ncommit them first, so the published build can be traced to a commit (or --allow-dirty)",
            shown.join("\n")
        ));
    }
    let head = git(&["rev-parse", "--short", "HEAD"]).map(|h| h.trim().to_string());
    Ok(head.map(|h| {
        if dirty.trim().is_empty() {
            h
        } else {
            format!("{h}-dirty")
        }
    }))
}

fn token(sys: &dyn System, server: &str) -> Result<String> {
    if let Some(token) = sys.var("WAREHOUSE_TOKEN") {
        return Ok(token);
    }
    let (Some(user), Some(pass)) = (sys.var("WAREHOUSE_USER"), sys.var("WAREHOUSE_PASS")) else {
        return fail("set WAREHOUSE_TOKEN, or WAREHOUSE_USER and WAREHOUSE_PASS to log in");
    };
    let request = Http {
        method: "POST",
        url: format!("{server}/gatehouse/api/v1/auth/login"),
        headers: vec![("content-type".into(), "application/json".into())],
        body: quench_serde::json::to_string(&json!({ "username": user, "password": pass }))
            .into_bytes(),
    };
    let (status, body) = sys
        .http(&request)
        .map_err(|e| Error(format!("login: {e}")))?;
    let answer: Value = std::str::from_utf8(&body)
        .ok()
        .and_then(|t| quench_serde::json::from_str(t).ok())
        .unwrap_or(Value::Null);
    match (status, answer.get("access_token").and_then(Value::as_str)) {
        (200, Some(t)) if !t.is_empty() => Ok(t.to_string()),
        _ => fail(format!("the login was refused (HTTP {status})")),
    }
}

/// Builds (or takes) the signed APK and sends it.
///
/// # Errors
/// A dirty tree, an unsigned build, no credentials, a refusal from the registry.
pub fn run(
    sys: &dyn System,
    out: &mut dyn Write,
    project: &Project,
    options: &Options,
) -> Result<()> {
    let server = server(sys, project)?;
    let commit = commit(sys, project, options.allow_dirty)?;
    let apk: PathBuf = match &options.apk {
        Some(path) => {
            let apk = project.root.join(path);
            android::verify_apk(sys, out, &apk)?;
            apk
        }
        None => android::build_apk(sys, out, project, true, false, None)?,
    };
    let pkg = android::package(project)?;
    let (name, code) = android::version(project);
    let body = std::fs::read(&apk).map_err(|e| Error(format!("{}: {e}", apk.display())))?;
    let what = format!(
        "{pkg} {name} (version_code {code}, {} KiB{}) to {server}",
        body.len() / 1024,
        commit.map(|c| format!(", commit {c}")).unwrap_or_default()
    );
    if options.dry_run {
        writeln!(out, "would publish {what}")?;
        return Ok(());
    }
    let token = token(sys, &server)?;
    writeln!(out, "publishing {what}")?;
    let request = Http {
        method: "PUT",
        url: format!("{server}/warehouse/api/v1/artifacts/{pkg}/android/{code}"),
        headers: vec![
            ("authorization".into(), format!("Bearer {token}")),
            (
                "content-type".into(),
                "application/vnd.android.package-archive".into(),
            ),
        ],
        body,
    };
    let (status, reply) = sys
        .http(&request)
        .map_err(|e| Error(format!("publish: {e}")))?;
    match status {
        201 => {
            writeln!(out, "published {pkg} android {code}")?;
            Ok(())
        }
        409 => fail(format!(
            "version_code {code} is already published (versions are immutable): raise version_code under [android] in quench.toml"
        )),
        _ => fail(format!(
            "publish failed: HTTP {status} {}",
            String::from_utf8_lossy(&reply).trim()
        )),
    }
}
