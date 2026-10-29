use super::support::*;
use quench_tool::android::{build_apk, keystore, package, run, verify_apk, version};
use quench_tool::project::Project;
use std::path::PathBuf;

/// A project with the Android host, a signing key configured, and what Gradle would have built.
fn ready(label: &str) -> (Fake, Project) {
    let p = project_named(label, "my-app", "web");
    quench_tool::android::init(&mut Vec::new(), &p, Some("dev.lorehaven.myapp")).unwrap();
    let manifest = p.root.join(".quench.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        text.replace(
            "version_code = 1",
            "version_code = 7\nkeystore = \"release.keystore\"\nkey_alias = \"release\"",
        ),
    )
    .unwrap();
    let p = Project::find(&p.root).unwrap();
    let out = p.root.join("android/app/build/outputs");
    for file in [
        "apk/debug/app-debug.apk",
        "apk/release/app-release.apk",
        "bundle/release/app-release.aab",
    ] {
        std::fs::create_dir_all(out.join(file).parent().unwrap()).unwrap();
        std::fs::write(out.join(file), file.as_bytes()).unwrap();
    }
    std::fs::create_dir_all(p.wasm_path("wasm").parent().unwrap()).unwrap();
    let mut sys = Fake::default();
    sys.vars.insert("ANDROID_HOME".into(), "/sdk".into());
    sys.vars
        .insert("QUENCH_KEYSTORE_PASSWORD".into(), "secret".into());
    sys.dirs.insert(
        "/sdk/build-tools".into(),
        vec!["34.0.0".into(), "36.0.0".into()],
    );
    sys.existing
        .insert("/sdk/build-tools/36.0.0/apksigner".into());
    sys.existing.insert(p.root.join("release.keystore"));
    sys.captures.insert(
        format!(
            "/sdk/build-tools/36.0.0/apksigner verify --print-certs {}",
            p.dist().join("my-app-0.1.0-7.apk").display()
        ),
        "Signer #1 certificate DN: CN=my-app\n".into(),
    );
    (sys, p)
}

#[quench_test::test]
fn the_package_and_version_come_from_quench_toml() {
    let (_, p) = ready("apk-config");
    assert_eq!(package(&p).unwrap(), "dev.lorehaven.myapp");
    assert_eq!(version(&p), ("0.1.0".to_string(), 7));
    let bare = project("apk-bare", "web");
    assert_eq!(
        version(&bare),
        ("0.1.0".to_string(), 1_000),
        "defaults: the Cargo version, as one number"
    );
    assert!(
        package(&bare)
            .unwrap_err()
            .to_string()
            .contains("quench android init")
    );
}

#[quench_test::test]
fn a_debug_apk_is_built_by_gradle_with_the_version_and_copied_to_dist() {
    let (sys, p) = ready("apk-debug");
    let mut out = Vec::new();
    let apk = build_apk(&sys, &mut out, &p, false, false, None).unwrap();
    let lines = sys.lines();
    assert_eq!(
        lines[0], "cargo build --target wasm32-unknown-unknown --profile wasm -p my-app-app",
        "the bundle comes first"
    );
    assert!(
        lines
            .iter()
            .any(|l| l
                == "./gradlew -q assembleDebug -Pquench.versionCode=7 -Pquench.versionName=0.1.0"),
        "{lines:?}"
    );
    assert_eq!(apk, p.dist().join("my-app-0.1.0-7.apk"));
    assert_eq!(std::fs::read(&apk).unwrap(), b"apk/debug/app-debug.apk");
    let ran = sys.ran.borrow();
    let gradle = ran.iter().find(|c| c.program == "./gradlew").unwrap();
    assert!(gradle.cwd.ends_with("android"));
    assert!(
        gradle.env.iter().all(|(n, _)| n != "QUENCH_KEYSTORE"),
        "a debug build is not signed with the release key"
    );
}

#[quench_test::test]
fn native_code_is_built_for_the_abis_first_when_the_project_has_it() {
    let (sys, p) = ready("apk-native");
    std::fs::create_dir_all(p.root.join("native")).unwrap();
    std::fs::write(p.root.join("native/Cargo.toml"), "").unwrap();
    let mut sys = sys;
    sys.vars.insert("ANDROID_NDK_HOME".into(), "/ndk".into());
    sys.existing.insert(PathBuf::from("/ndk"));
    for target in ["aarch64-linux-android", "x86_64-linux-android"] {
        let dir = p.target_dir().join(target).join("debug");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("quench_native"), b"ELF").unwrap();
    }
    build_apk(&sys, &mut Vec::new(), &p, false, false, None).unwrap();
    let lines = sys.lines();
    let native = lines
        .iter()
        .position(|l| l.contains("my-app-native --target aarch64"))
        .unwrap();
    let gradle = lines
        .iter()
        .position(|l| l.starts_with("./gradlew"))
        .unwrap();
    assert!(native < gradle, "{lines:?}");
}

#[quench_test::test]
fn a_release_is_signed_with_the_configured_key_and_verified() {
    let (sys, p) = ready("apk-release");
    let mut out = Vec::new();
    let apk = build_apk(&sys, &mut out, &p, true, false, None).unwrap();
    assert_eq!(std::fs::read(&apk).unwrap(), b"apk/release/app-release.apk");
    let ran = sys.ran.borrow();
    let gradle = ran.iter().find(|c| c.program == "./gradlew").unwrap();
    assert_eq!(gradle.args[1], "assembleRelease");
    let env: std::collections::HashMap<_, _> = gradle.env.iter().cloned().collect();
    assert!(env["QUENCH_KEYSTORE"].ends_with("release.keystore"));
    assert_eq!(env["QUENCH_KEY_ALIAS"], "release");
    assert!(
        !env.contains_key("QUENCH_KEYSTORE_PASSWORD"),
        "the password stays in the environment, out of the command"
    );
    assert!(!gradle.args.iter().any(|a| a.contains("secret")));
    assert!(text(out).contains("signed: Signer #1 certificate DN: CN=my-app"));
}

#[quench_test::test]
fn an_app_bundle_is_a_release_too() {
    let (sys, p) = ready("apk-aab");
    let aab = build_apk(&sys, &mut Vec::new(), &p, false, true, None).unwrap();
    assert_eq!(aab, p.dist().join("my-app-0.1.0-7.aab"));
    assert!(
        sys.lines()
            .iter()
            .any(|l| l.starts_with("./gradlew -q bundleRelease"))
    );
}

#[quench_test::test]
fn a_release_without_a_key_says_how_to_get_one() {
    let (mut sys, p) = ready("apk-nokey");
    sys.vars.remove("QUENCH_KEYSTORE_PASSWORD");
    let e = build_apk(&sys, &mut Vec::new(), &p, true, false, None)
        .unwrap_err()
        .to_string();
    assert!(e.contains("QUENCH_KEYSTORE_PASSWORD"), "{e}");
    let (mut sys, p) = ready("apk-nofile");
    sys.existing.remove(&p.root.join("release.keystore"));
    assert!(
        build_apk(&sys, &mut Vec::new(), &p, true, false, None)
            .unwrap_err()
            .to_string()
            .contains("does not exist")
    );
    let bare = project_named("apk-unconfigured", "my-app", "web");
    quench_tool::android::init(&mut Vec::new(), &bare, Some("dev.lorehaven.myapp")).unwrap();
    let bare = Project::find(&bare.root).unwrap();
    let e = build_apk(&Fake::default(), &mut Vec::new(), &bare, true, false, None)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("key_alias") && e.contains("quench android keystore"),
        "{e}"
    );
}

#[quench_test::test]
fn verification_needs_apksigner_and_a_valid_signature() {
    let (mut sys, p) = ready("apk-verify");
    let apk = p.dist().join("x.apk");
    let e = verify_apk(&sys, &mut Vec::new(), &apk)
        .unwrap_err()
        .to_string();
    assert!(e.contains("not signed with a valid signature"), "{e}");
    sys.dirs.clear();
    assert!(
        verify_apk(&sys, &mut Vec::new(), &apk)
            .unwrap_err()
            .to_string()
            .contains("apksigner was not found")
    );
}

#[quench_test::test]
fn a_gradle_failure_stops_the_build() {
    let (sys, p) = ready("apk-fail");
    *sys.fail_on.borrow_mut() = Some("assembleDebug".into());
    assert!(
        build_apk(&sys, &mut Vec::new(), &p, false, false, None)
            .unwrap_err()
            .to_string()
            .contains("assembleDebug")
    );
    let (sys2, p2) = ready("apk-nooutput");
    std::fs::remove_file(
        p2.root
            .join("android/app/build/outputs/apk/debug/app-debug.apk"),
    )
    .unwrap();
    assert!(build_apk(&sys2, &mut Vec::new(), &p2, false, false, None).is_err());
}

#[quench_test::test]
fn run_builds_for_the_devices_abi_installs_and_starts() {
    let (mut sys, p) = ready("run");
    sys.captures.insert(
        "adb devices".into(),
        "List of devices attached\nemulator-5554\tdevice\n".into(),
    );
    sys.captures.insert(
        "adb shell getprop ro.product.cpu.abi".into(),
        "x86_64\n".into(),
    );
    std::fs::create_dir_all(p.root.join("native")).unwrap();
    std::fs::write(p.root.join("native/Cargo.toml"), "").unwrap();
    sys.vars.insert("ANDROID_NDK_HOME".into(), "/ndk".into());
    sys.existing.insert(PathBuf::from("/ndk"));
    let dir = p.target_dir().join("x86_64-linux-android/debug");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("quench_native"), b"ELF").unwrap();
    run(&sys, &mut Vec::new(), &p, None).unwrap();
    let lines = sys.lines();
    assert!(
        lines
            .iter()
            .any(|l| l == "cargo build -p my-app-native --target x86_64-linux-android"),
        "only the device's ABI: {lines:?}"
    );
    assert!(!lines.iter().any(|l| l.contains("aarch64")));
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("./gradlew -q installDebug"))
    );
    assert_eq!(
        lines.last().unwrap(),
        "adb shell am start -n dev.lorehaven.myapp/.MainActivity --es dev_url off"
    );
}

#[quench_test::test]
fn run_needs_a_device_and_an_abi_it_can_build() {
    let (sys, p) = ready("run-nodevice");
    let e = run(&sys, &mut Vec::new(), &p, None)
        .unwrap_err()
        .to_string();
    assert!(e.contains("no device"), "{e}");
    let (mut sys, p) = ready("run-abi");
    sys.captures
        .insert("adb devices".into(), "emulator-5554\tdevice\n".into());
    std::fs::create_dir_all(p.root.join("native")).unwrap();
    std::fs::write(p.root.join("native/Cargo.toml"), "").unwrap();
    let e = run(&sys, &mut Vec::new(), &p, Some("mips"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("`mips`"), "{e}");
}

#[quench_test::test]
fn a_keystore_is_made_by_keytool_with_passwords_from_the_environment() {
    let (sys, p) = ready("keystore");
    std::fs::remove_file(p.root.join("release.keystore")).ok();
    let mut sys = sys;
    sys.existing.remove(&p.root.join("release.keystore"));
    let mut out = Vec::new();
    keystore(&sys, &mut out, &p, "release.keystore", "release").unwrap();
    let ran = sys.ran.borrow();
    let cmd = &ran[0];
    assert_eq!(cmd.program, "keytool");
    assert!(
        cmd.args.contains(&"-storepass:env".to_string())
            && cmd.args.contains(&"QUENCH_KEYSTORE_PASSWORD".to_string())
    );
    assert!(
        !cmd.args.iter().any(|a| a == "secret"),
        "the password is never an argument"
    );
    assert!(
        !cmd.args.contains(&"-keypass:env".to_string()),
        "no separate key password was given"
    );
    assert!(text(out).contains("named it in .quench.toml"));
    let manifest = std::fs::read_to_string(p.root.join(".quench.toml")).unwrap();
    assert!(
        manifest.contains("keystore = \"release.keystore\"")
            && manifest.contains("key_alias = \"release\""),
        "{manifest}"
    );
    assert_eq!(manifest.matches("keystore =").count(), 1);
}

#[quench_test::test]
fn a_keystore_is_made_once_and_needs_a_password() {
    let (sys, p) = ready("keystore-guard");
    let e = keystore(&sys, &mut Vec::new(), &p, "release.keystore", "release")
        .unwrap_err()
        .to_string();
    assert!(e.contains("exists already"), "{e}");
    let (mut sys, p) = ready("keystore-pw");
    sys.existing.remove(&p.root.join("release.keystore"));
    sys.vars.remove("QUENCH_KEYSTORE_PASSWORD");
    assert!(
        keystore(&sys, &mut Vec::new(), &p, "k.keystore", "a")
            .unwrap_err()
            .to_string()
            .contains("QUENCH_KEYSTORE_PASSWORD")
    );
    sys.vars
        .insert("QUENCH_KEYSTORE_PASSWORD".into(), "short".into());
    assert!(
        keystore(&sys, &mut Vec::new(), &p, "k.keystore", "a")
            .unwrap_err()
            .to_string()
            .contains("6 characters")
    );
    sys.vars
        .insert("QUENCH_KEYSTORE_PASSWORD".into(), "longenough".into());
    sys.vars
        .insert("QUENCH_KEY_PASSWORD".into(), "keypw".into());
    keystore(&sys, &mut Vec::new(), &p, "k.keystore", "a").unwrap();
    assert!(
        sys.ran.borrow()[0]
            .args
            .contains(&"-keypass:env".to_string())
    );
}

#[quench_test::test]
fn the_version_is_the_cargo_version_unless_quench_toml_says_otherwise() {
    let (_, p) = ready("apk-version-derived");
    // `ready` writes version_code = 7 into .quench.toml: that wins for the code, the name is `.quench.toml`'s too.
    assert_eq!(version(&p).1, 7);
    let manifest = p.root.join(".quench.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        text.lines()
            .filter(|l| !l.starts_with("version_code") && !l.starts_with("version_name"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    std::fs::write(
        p.root.join("Cargo.toml"),
        "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"2.3.4\"\n",
    )
    .unwrap();
    let p = Project::find(&p.root).unwrap();
    assert_eq!(version(&p), ("2.3.4".to_string(), 2_003_004));
}

#[quench_test::test]
fn a_project_with_only_a_surface_still_builds_its_libraries_for_every_abi() {
    // The screens-only app (no `native/`) has `surface/`; its library is built for both ABIs, so the APK is not
    // left with whatever an earlier build put in jniLibs.
    let (mut sys, p) = ready("apk-surface");
    std::fs::create_dir_all(p.root.join("surface")).unwrap();
    std::fs::write(p.root.join("surface/Cargo.toml"), "").unwrap();
    sys.vars.insert("ANDROID_NDK_HOME".into(), "/ndk".into());
    sys.existing.insert(PathBuf::from("/ndk"));
    for target in ["aarch64-linux-android", "x86_64-linux-android"] {
        let dir = p.target_dir().join(target).join("release");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("quench_surface"), b"ELF").unwrap();
    }
    build_apk(&sys, &mut Vec::new(), &p, true, false, None).unwrap();
    let lines = sys.lines();
    for abi in ["aarch64", "x86_64"] {
        assert!(
            lines
                .iter()
                .any(|l| l.contains(&format!("my-app-surface --target {abi}"))),
            "{abi} in {lines:?}"
        );
    }
    for abi in ["arm64-v8a", "x86_64"] {
        assert!(
            p.root
                .join(format!(
                    "android/app/src/main/jniLibs/{abi}/libquench_surface.so"
                ))
                .is_file(),
            "{abi}"
        );
    }
}
