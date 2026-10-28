use super::support::*;
use quench_tool::android;
use quench_tool::templates::{check_package, render_android};

fn get(files: &[quench_tool::templates::HostFile], path: &str) -> String {
    let f = files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("no {path}"));
    String::from_utf8(f.bytes.clone()).unwrap()
}

#[quench_test::test]
fn package_names_are_checked() {
    for good in [
        "com.example.app",
        "dev.lorehaven.gallery",
        "a.b",
        "org.x9.my_app",
    ] {
        assert!(check_package(good).is_ok(), "{good}");
    }
    for bad in [
        "app",
        "",
        "com..app",
        "Com.example",
        "com.1app",
        "com.my-app",
        "com.example.",
    ] {
        assert!(check_package(bad).is_err(), "{bad}");
    }
}

#[quench_test::test]
fn the_host_project_has_every_file_in_the_packages_directory() {
    let files = render_android("my-app", "dev.lorehaven.my_app").unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    for want in [
        "settings.gradle.kts",
        "build.gradle.kts",
        "gradle.properties",
        ".gitignore",
        "app/build.gradle.kts",
        "app/src/main/AndroidManifest.xml",
        "app/src/main/res/values/themes.xml",
        "app/src/main/res/xml/network_security_config.xml",
        "app/src/debug/res/xml/network_security_config.xml",
        "app/src/main/kotlin/dev/lorehaven/my_app/MainActivity.kt",
        "app/src/main/kotlin/dev/lorehaven/my_app/AppAssets.kt",
        "gradlew",
        "gradlew.bat",
        "gradle/wrapper/gradle-wrapper.jar",
        "gradle/wrapper/gradle-wrapper.properties",
    ] {
        assert!(paths.contains(&want), "{want} missing from {paths:?}");
    }
    assert_eq!(files.len(), 20);
}

#[quench_test::test]
fn names_reach_the_gradle_files_the_manifest_and_the_sources() {
    let files = render_android("my-app", "dev.lorehaven.my_app").unwrap();
    assert!(get(&files, "settings.gradle.kts").contains("rootProject.name = \"my-app\""));
    let gradle = get(&files, "app/build.gradle.kts");
    assert!(gradle.contains("namespace = \"dev.lorehaven.my_app\""));
    assert!(gradle.contains("applicationId = \"dev.lorehaven.my_app\""));
    assert!(
        gradle.contains("../../dist/www"),
        "the host serves the bundle `quench build --bundle` writes"
    );
    assert!(get(&files, "app/src/main/AndroidManifest.xml").contains("android:label=\"my-app\""));
    let activity = get(
        &files,
        "app/src/main/kotlin/dev/lorehaven/my_app/MainActivity.kt",
    );
    assert!(activity.starts_with("package dev.lorehaven.my_app"));
    assert!(activity.contains("am start -n dev.lorehaven.my_app/.MainActivity"));
}

#[quench_test::test]
fn the_host_keeps_the_underscore_directory_and_is_https_only_in_release() {
    let files = render_android("a", "com.example.a").unwrap();
    // aapt drops asset directories starting with `_` unless told not to: the bundle's are `_quench/`.
    assert!(get(&files, "app/build.gradle.kts").contains("ignoreAssetsPatterns"));
    let release = get(&files, "app/src/main/res/xml/network_security_config.xml");
    assert!(
        !release.contains("cleartextTrafficPermitted=\"true\""),
        "{release}"
    );
    let debug = get(&files, "app/src/debug/res/xml/network_security_config.xml");
    assert!(
        debug.contains("127.0.0.1") && debug.contains("10.0.2.2"),
        "{debug}"
    );
    // The page's policy: nothing inline, nothing from elsewhere.
    let assets = get(&files, "app/src/main/kotlin/com/example/a/AppAssets.kt");
    assert!(assets.contains("default-src 'self'") && assets.contains("'wasm-unsafe-eval'"));
}

#[quench_test::test]
fn the_host_has_the_bridge_and_only_the_apps_own_origins_get_it() {
    let files = render_android("my-app", "com.example.my_app").unwrap();
    let src = "app/src/main/kotlin/com/example/my_app";
    let bridge = get(&files, &format!("{src}/Bridge.kt"));
    // The port is addressed to the page's origin and offered only after the origin is checked.
    assert!(
        bridge.contains("postWebMessage") && bridge.contains("Uri.parse(origin(uri))"),
        "{bridge}"
    );
    assert!(
        bridge.contains("if (!allowed(uri))") && bridge.contains("refused origin"),
        "{bridge}"
    );
    assert!(
        bridge.contains("MAX_MESSAGE")
            && bridge.contains("invalid_args")
            && bridge.contains("unknown_plugin")
    );
    assert!(
        bridge.contains("fun onPermissionResult"),
        "the permission flow"
    );
    let activity = get(&files, &format!("{src}/MainActivity.kt"));
    assert!(
        activity.contains("Bridge.USER_AGENT") && activity.contains("onRequestPermissionsResult"),
        "{activity}"
    );
    assert!(
        activity.contains("assets_.owns(url) || isDevServer(url)"),
        "the allowed origins: the bundle and the dev server"
    );
    assert!(
        activity.contains("override fun onPageStarted") && activity.contains("bridge.disconnect()")
    );
    assert!(get(&files, &format!("{src}/Plugins.kt")).contains("EchoPlugin()"));
    assert!(get(&files, &format!("{src}/EchoPlugin.kt")).contains("POST_NOTIFICATIONS"));
    // The permission the echo plugin asks for is declared in debug builds only.
    assert!(get(&files, "app/src/debug/AndroidManifest.xml").contains("POST_NOTIFICATIONS"));
    assert!(!get(&files, "app/src/main/AndroidManifest.xml").contains("POST_NOTIFICATIONS"));
}

#[quench_test::test]
fn no_template_syntax_is_left_and_the_wrapper_is_copied_as_it_is() {
    let files = render_android("my-app", "com.example.my_app").unwrap();
    for f in &files {
        if std::path::Path::new(&f.path)
            .extension()
            .is_some_and(|e| e == "jar")
        {
            assert!(f.bytes.starts_with(b"PK"), "a jar is a zip");
            assert!(f.bytes.len() > 10_000);
        } else {
            let text = String::from_utf8(f.bytes.clone()).unwrap();
            assert!(
                !text.contains("{{") && !text.contains("{%"),
                "{}:\n{text}",
                f.path
            );
        }
    }
    let executable: Vec<&str> = files
        .iter()
        .filter(|f| f.executable)
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(executable, ["gradlew"]);
    assert!(render_android("x", "nodots").is_err());
}

#[quench_test::test]
fn init_writes_the_project_once() {
    let p = project_named("android-init", "my-app", "web");
    let mut out = Vec::new();
    android::init(&mut out, &p, None).unwrap();
    assert!(text(out).contains("com.example.my_app"));
    let root = p.root.join("android");
    assert!(
        root.join("app/src/main/kotlin/com/example/my_app/MainActivity.kt")
            .is_file()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(root.join("gradlew"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "gradlew must be runnable");
    }
    let e = android::init(&mut Vec::new(), &p, Some("dev.other.app"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("exists already"), "{e}");
    assert_eq!(android::default_package(&p), "com.example.my_app");
}

#[quench_test::test]
fn init_takes_the_package_from_the_option_and_rejects_a_bad_one() {
    let p = project("android-pkg", "web");
    let e = android::init(&mut Vec::new(), &p, Some("Bad"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("not a package name"), "{e}");
    assert!(
        !p.root.join("android").exists(),
        "nothing is written for a bad name"
    );
    android::init(&mut Vec::new(), &p, Some("dev.lorehaven.demo")).unwrap();
    assert!(
        p.root
            .join("android/app/src/main/kotlin/dev/lorehaven/demo")
            .is_dir()
    );
}
