//! `quench doctor`: what this machine has and lacks, with the command that fixes each gap.
//!
//! Web development is required; Android and desktop are shown as optional until the commands that
//! need them exist, so a web-only machine passes.

use std::io::Write;
use std::path::PathBuf;

use crate::error::Result;
use crate::system::System;

/// The oldest Rust that builds edition 2024.
pub const MIN_RUSTC: (u32, u32) = (1, 85);
/// Android's Gradle plugin needs a JDK of at least this version.
pub const MIN_JAVA: u32 = 17;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Ok(String),
    Missing(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: &'static str,
    pub status: Status,
    pub required: bool,
}

/// The numbers in the first `a.b` or `a.b.c` found in `text`.
#[must_use]
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let mut numbers = text[start..]
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .next()?
        .split('.')
        .map(|part| part.parse::<u32>().ok());
    let major = numbers.next()??;
    let minor = numbers.next().flatten().unwrap_or(0);
    let patch = numbers.next().flatten().unwrap_or(0);
    Some((major, minor, patch))
}

/// The major version of a JDK from `java -version`: `"17.0.9"` is 17, `"1.8.0_392"` is 8.
#[must_use]
pub fn java_major(text: &str) -> Option<u32> {
    let quoted = text.split('"').nth(1).unwrap_or(text);
    let (a, b, _) = parse_version(quoted)?;
    Some(if a == 1 { b } else { a })
}

fn ok(detail: impl Into<String>) -> Status {
    Status::Ok(detail.into())
}

fn missing(fix: impl Into<String>) -> Status {
    Status::Missing(fix.into())
}

fn tool(sys: &dyn System, program: &str, args: &[&str], fix: &str) -> Status {
    sys.capture(program, args).map_or_else(
        || missing(fix),
        |text| ok(text.lines().next().unwrap_or("").trim()),
    )
}

fn rustc(sys: &dyn System) -> Status {
    let Some(text) = sys.capture("rustc", &["--version"]) else {
        return missing("install Rust: https://rustup.rs");
    };
    match parse_version(&text) {
        Some((a, b, _)) if (a, b) >= MIN_RUSTC => ok(text.trim()),
        _ => missing(format!(
            "{} is too old for edition 2024; need {}.{} or later (rustup update)",
            text.trim(),
            MIN_RUSTC.0,
            MIN_RUSTC.1
        )),
    }
}

fn rust_target(sys: &dyn System, target: &str) -> Status {
    let fix = format!("rustup target add {target}");
    let Some(sysroot) = sys.capture("rustc", &["--print", "sysroot"]) else {
        return missing(fix);
    };
    let dir = PathBuf::from(sysroot.trim())
        .join("lib/rustlib")
        .join(target);
    if sys.exists(&dir) {
        ok("installed")
    } else {
        missing(fix)
    }
}

fn rust_targets(sys: &dyn System, targets: &[&str]) -> Status {
    let absent: Vec<&str> = targets
        .iter()
        .copied()
        .filter(|t| matches!(rust_target(sys, t), Status::Missing(_)))
        .collect();
    if absent.is_empty() {
        ok(targets.join(", "))
    } else {
        missing(format!("rustup target add {}", absent.join(" ")))
    }
}

fn home(sys: &dyn System, relative: &str) -> Option<PathBuf> {
    sys.var("HOME").map(|h| PathBuf::from(h).join(relative))
}

fn android_sdk(sys: &dyn System) -> Option<PathBuf> {
    ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .iter()
        .filter_map(|v| sys.var(v).map(PathBuf::from))
        .chain(home(sys, "Android/Sdk"))
        .chain([PathBuf::from("/opt/android-sdk")])
        .find(|p| sys.exists(p))
}

fn android_ndk(sys: &dyn System, sdk: Option<&PathBuf>) -> Status {
    let fix = "install the NDK with sdkmanager \"ndk;28.2.13676358\" and set ANDROID_NDK_HOME";
    for var in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT"] {
        if let Some(path) = sys.var(var).map(PathBuf::from)
            && sys.exists(&path)
        {
            return ok(path.display().to_string());
        }
    }
    let versions = sdk
        .map(|s| sys.list_dir(&s.join("ndk")))
        .unwrap_or_default();
    versions.last().map_or_else(
        || missing(fix),
        |v| ok(format!("{v} (in the SDK; set ANDROID_NDK_HOME to use it)")),
    )
}

fn java(sys: &dyn System) -> Status {
    let fix = format!("install a JDK {MIN_JAVA} or later");
    match sys
        .capture("java", &["-version"])
        .as_deref()
        .and_then(java_major)
    {
        Some(major) if major >= MIN_JAVA => ok(format!("JDK {major}")),
        Some(major) => missing(format!("JDK {major} is too old; {fix}")),
        None => missing(fix),
    }
}

fn webkit(sys: &dyn System) -> Status {
    for module in ["webkit2gtk-4.1", "webkitgtk-6.0"] {
        if sys
            .capture("pkg-config", &["--modversion", module])
            .is_some()
        {
            return ok(module);
        }
    }
    missing("install webkit2gtk (Arch: webkit2gtk-4.1, Debian: libwebkit2gtk-4.1-dev)")
}

fn on_path(sys: &dyn System, programs: &[&str], fix: &str) -> Status {
    programs
        .iter()
        .find(|p| sys.on_path(p))
        .map_or_else(|| missing(fix), |p| ok(format!("found {p}")))
}

/// Looks at the machine.
#[must_use]
pub fn checks(sys: &dyn System) -> Vec<Check> {
    let sdk = android_sdk(sys);
    let check = |name, status, required| Check {
        name,
        status,
        required,
    };
    vec![
        check("rustc", rustc(sys), true),
        check(
            "cargo",
            tool(
                sys,
                "cargo",
                &["--version"],
                "install Rust: https://rustup.rs",
            ),
            true,
        ),
        check(
            "wasm32 target",
            rust_target(sys, "wasm32-unknown-unknown"),
            true,
        ),
        check(
            "wasm-opt",
            on_path(
                sys,
                &["wasm-opt"],
                "optional, shrinks release modules: install binaryen",
            ),
            false,
        ),
        check(
            "node 22+ (browser tests)",
            tool(
                sys,
                "node",
                &["--version"],
                "optional, runs quench-ui-web's browser tests",
            ),
            false,
        ),
        check(
            "chromium (browser tests)",
            on_path(
                sys,
                &[
                    "chromium",
                    "chromium-browser",
                    "google-chrome",
                    "google-chrome-stable",
                ],
                "optional, runs the browser tests",
            ),
            false,
        ),
        check(
            "android sdk",
            sdk.as_ref().map_or_else(
                || missing("install the Android SDK and set ANDROID_HOME"),
                |p| ok(p.display().to_string()),
            ),
            false,
        ),
        check("android ndk", android_ndk(sys, sdk.as_ref()), false),
        check(
            "adb",
            on_path(
                sys,
                &["adb"],
                "install platform-tools (sdkmanager platform-tools)",
            ),
            false,
        ),
        check(
            "android rust targets",
            rust_targets(sys, &["aarch64-linux-android", "x86_64-linux-android"]),
            false,
        ),
        check("java", java(sys), false),
        check(
            "gradle",
            on_path(
                sys,
                &["gradle"],
                "optional: a project's gradle wrapper is used when it has one",
            ),
            false,
        ),
        check("webkit2gtk (linux desktop)", webkit(sys), false),
    ]
}

/// Prints the checks; true when everything required is present.
///
/// # Errors
/// Output that cannot be written.
pub fn report(out: &mut dyn Write, checks: &[Check]) -> Result<bool> {
    let mut pass = true;
    for c in checks {
        match &c.status {
            Status::Ok(detail) => writeln!(out, "  ok       {:<28} {detail}", c.name)?,
            Status::Missing(fix) if c.required => {
                pass = false;
                writeln!(out, "  MISSING  {:<28} {fix}", c.name)?;
            }
            Status::Missing(fix) => writeln!(out, "  absent   {:<28} {fix}", c.name)?,
        }
    }
    if pass {
        writeln!(out, "everything needed for web development is here")?;
    } else {
        writeln!(out, "fix the MISSING lines first")?;
    }
    Ok(pass)
}
