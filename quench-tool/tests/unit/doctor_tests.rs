use super::support::*;
use quench_tool::doctor::{Status, checks, java_major, parse_version, report};
use std::path::PathBuf;

#[quench_test::test]
fn versions_are_read_from_tool_output() {
    assert_eq!(
        parse_version("rustc 1.98.1 (48a229cea 2026-09-01)"),
        Some((1, 98, 1))
    );
    assert_eq!(parse_version("v22.23.4"), Some((22, 23, 4)));
    assert_eq!(parse_version("Gradle 8.14"), Some((8, 14, 0)));
    assert_eq!(parse_version("21"), Some((21, 0, 0)));
    assert_eq!(parse_version("no digits"), None);
}

#[quench_test::test]
fn the_java_major_is_read_from_both_version_schemes() {
    assert_eq!(
        java_major("openjdk version \"21.0.3\" 2024-04-16"),
        Some(21)
    );
    assert_eq!(java_major("java version \"1.8.0_392\""), Some(8));
    assert_eq!(java_major("openjdk version \"17\""), Some(17));
    assert_eq!(java_major("unknown"), None);
}

/// A machine that has everything.
fn complete() -> Fake {
    let mut f = Fake::default();
    let sysroot = "/usr/lib/rustlib-root";
    f.captures
        .insert("rustc --version".into(), "rustc 1.98.1 (x)\n".into());
    f.captures
        .insert("cargo --version".into(), "cargo 1.98.1 (x)\n".into());
    f.captures
        .insert("rustc --print sysroot".into(), format!("{sysroot}\n"));
    for t in [
        "wasm32-unknown-unknown",
        "aarch64-linux-android",
        "x86_64-linux-android",
    ] {
        f.existing
            .insert(PathBuf::from(sysroot).join("lib/rustlib").join(t));
    }
    f.captures
        .insert("node --version".into(), "v22.1.0\n".into());
    f.captures.insert(
        "java -version".into(),
        "openjdk version \"21.0.3\"\n".into(),
    );
    f.captures.insert(
        "pkg-config --modversion webkit2gtk-4.1".into(),
        "2.44\n".into(),
    );
    f.vars.insert("ANDROID_HOME".into(), "/sdk".into());
    f.vars.insert("ANDROID_NDK_HOME".into(), "/ndk".into());
    f.existing.insert("/sdk".into());
    f.existing.insert("/ndk".into());
    for p in ["wasm-opt", "chromium", "adb", "gradle"] {
        f.programs.insert(p.into());
    }
    f
}

fn status<'a>(all: &'a [quench_tool::doctor::Check], name: &str) -> &'a Status {
    &all.iter()
        .find(|c| c.name.starts_with(name))
        .unwrap()
        .status
}

#[quench_test::test]
fn a_complete_machine_passes_everything() {
    let all = checks(&complete());
    assert!(
        all.iter().all(|c| matches!(c.status, Status::Ok(_))),
        "{all:#?}"
    );
    let mut out = Vec::new();
    assert!(report(&mut out, &all).unwrap());
    assert!(text(out).contains("everything needed for web development"));
}

#[quench_test::test]
fn a_bare_machine_fails_on_the_required_checks_only() {
    let all = checks(&Fake::default());
    let mut out = Vec::new();
    assert!(!report(&mut out, &all).unwrap());
    let said = text(out);
    assert!(said.contains("MISSING  rustc"), "{said}");
    assert!(
        said.contains("MISSING  wasm32 target")
            && said.contains("rustup target add wasm32-unknown-unknown"),
        "{said}"
    );
    assert!(
        said.contains("absent   adb"),
        "optional gaps are not failures:\n{said}"
    );
    assert!(said.contains("fix the MISSING lines"));
}

#[quench_test::test]
fn an_old_compiler_is_refused_with_the_version_needed() {
    let mut f = complete();
    f.captures
        .insert("rustc --version".into(), "rustc 1.80.0 (x)\n".into());
    let all = checks(&f);
    let Status::Missing(why) = status(&all, "rustc") else {
        panic!()
    };
    assert!(why.contains("1.85"), "{why}");
}

#[quench_test::test]
fn android_pieces_are_found_in_the_sdk_when_the_variables_are_unset() {
    let mut f = complete();
    f.vars.clear();
    f.vars.insert("HOME".into(), "/home/me".into());
    f.existing.insert("/home/me/Android/Sdk".into());
    f.dirs.insert(
        "/home/me/Android/Sdk/ndk".into(),
        vec!["27.0.1".into(), "28.2.1".into()],
    );
    let all = checks(&f);
    assert_eq!(
        status(&all, "android sdk"),
        &Status::Ok("/home/me/Android/Sdk".into())
    );
    let Status::Ok(ndk) = status(&all, "android ndk") else {
        panic!()
    };
    assert!(
        ndk.starts_with("28.2.1") && ndk.contains("ANDROID_NDK_HOME"),
        "{ndk}"
    );
    let mut g = Fake::default();
    g.vars.insert("ANDROID_HOME".into(), "/sdk".into());
    g.existing.insert("/sdk".into());
    assert!(matches!(
        status(&checks(&g), "android ndk"),
        Status::Missing(_)
    ));
}

#[quench_test::test]
fn an_old_jdk_and_a_missing_android_target_are_explained() {
    let mut f = complete();
    f.captures.insert(
        "java -version".into(),
        "openjdk version \"11.0.2\"\n".into(),
    );
    f.existing.remove(&PathBuf::from(
        "/usr/lib/rustlib-root/lib/rustlib/aarch64-linux-android",
    ));
    let all = checks(&f);
    let Status::Missing(java) = status(&all, "java") else {
        panic!()
    };
    assert!(java.contains("JDK 11") && java.contains("17"), "{java}");
    assert_eq!(
        status(&all, "android rust targets"),
        &Status::Missing("rustup target add aarch64-linux-android".into())
    );
}

#[quench_test::test]
fn the_second_webkit_name_is_accepted() {
    let mut f = complete();
    f.captures.clear();
    f.captures.insert(
        "pkg-config --modversion webkitgtk-6.0".into(),
        "2.46\n".into(),
    );
    assert_eq!(
        status(&checks(&f), "webkit2gtk"),
        &Status::Ok("webkitgtk-6.0".into())
    );
}
