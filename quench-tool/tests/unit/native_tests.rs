use super::support::*;
use quench_tool::android::{ABIS, build_native, ndk_home};
use std::path::{Path, PathBuf};

fn with_native(label: &str) -> quench_tool::project::Project {
    let p = project_named(label, "my-app", "web");
    std::fs::create_dir_all(p.root.join("native")).unwrap();
    std::fs::write(p.root.join("native/Cargo.toml"), "").unwrap();
    p
}

fn fake_with_ndk() -> Fake {
    let mut f = Fake::default();
    f.vars.insert("ANDROID_NDK_HOME".into(), "/ndk".into());
    f.existing.insert(PathBuf::from("/ndk"));
    f
}

/// Puts the program where cargo would have, for the build to collect.
fn built(p: &quench_tool::project::Project, target: &str, profile: &str) {
    let dir = p.target_dir().join(target).join(profile);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("quench_native"), b"ELF").unwrap();
}

#[quench_test::test]
fn the_ndk_is_found_where_the_environment_says_or_in_the_sdk() {
    assert_eq!(ndk_home(&fake_with_ndk()).unwrap(), Path::new("/ndk"));
    let mut sdk = Fake::default();
    sdk.vars.insert("ANDROID_HOME".into(), "/sdk".into());
    sdk.dirs
        .insert("/sdk/ndk".into(), vec!["28.2.1".into(), "30.0.2".into()]);
    assert_eq!(
        ndk_home(&sdk).unwrap(),
        Path::new("/sdk/ndk/30.0.2"),
        "the newest"
    );
    let mut gone = Fake::default();
    gone.vars.insert("ANDROID_NDK_HOME".into(), "/gone".into());
    let e = ndk_home(&gone).unwrap_err().to_string();
    assert!(e.contains("ANDROID_NDK_HOME"), "{e}");
}

#[quench_test::test]
fn each_abi_is_built_with_the_ndk_linker_and_collected_for_gradle() {
    let p = with_native("native-both");
    for (_, target) in ABIS {
        built(&p, target, "debug");
    }
    let sys = fake_with_ndk();
    let mut out = Vec::new();
    build_native(&sys, &mut out, &p, false, None).unwrap();
    let lines = sys.lines();
    assert_eq!(
        lines[0],
        "cargo build -p my-app-native --target aarch64-linux-android"
    );
    assert_eq!(
        lines[1],
        "cargo build -p my-app-native --target x86_64-linux-android"
    );
    let ran = sys.ran.borrow();
    let (name, value) = &ran[0].env[0];
    assert_eq!(name, "CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER");
    assert!(
        value.starts_with("/ndk/toolchains/llvm/prebuilt/")
            && value.ends_with("/bin/aarch64-linux-android26-clang"),
        "{value}"
    );
    assert_eq!(ran[1].env[0].0, "CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER");
    for (abi, _) in ABIS {
        assert!(
            p.root
                .join(format!(
                    "android/app/src/main/jniLibs/{abi}/libquench_native.so"
                ))
                .is_file(),
            "{abi}"
        );
    }
    assert!(text(out).contains("native arm64-v8a"));
}

#[quench_test::test]
fn one_abi_and_release_are_options() {
    let p = with_native("native-one");
    built(&p, "x86_64-linux-android", "release");
    let sys = fake_with_ndk();
    build_native(&sys, &mut Vec::new(), &p, true, Some("x86_64")).unwrap();
    assert_eq!(
        sys.lines(),
        ["cargo build -p my-app-native --target x86_64-linux-android --release"]
    );
    assert!(
        p.root
            .join("android/app/src/main/jniLibs/x86_64/libquench_native.so")
            .is_file()
    );
    assert!(
        !p.root
            .join("android/app/src/main/jniLibs/arm64-v8a")
            .exists()
    );
}

#[quench_test::test]
fn mistakes_are_explained() {
    let none = project("native-none", "web");
    let e = build_native(&fake_with_ndk(), &mut Vec::new(), &none, false, None)
        .unwrap_err()
        .to_string();
    assert!(e.contains("quench plugin add native"), "{e}");
    let p = with_native("native-abi");
    let e = build_native(&fake_with_ndk(), &mut Vec::new(), &p, false, Some("mips"))
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("unknown ABI `mips`") && e.contains("arm64-v8a"),
        "{e}"
    );
    let e = build_native(&Fake::default(), &mut Vec::new(), &p, false, None)
        .unwrap_err()
        .to_string();
    assert!(e.contains("NDK was not found"), "{e}");
    let sys = fake_with_ndk();
    *sys.fail_on.borrow_mut() = Some("aarch64".into());
    assert!(
        build_native(&sys, &mut Vec::new(), &p, false, None)
            .unwrap_err()
            .to_string()
            .contains("failed")
    );
    let e = build_native(&fake_with_ndk(), &mut Vec::new(), &p, false, Some("x86_64"))
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("quench_native"),
        "a build that left no program: {e}"
    );
}
