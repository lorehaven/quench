use super::support::*;
use quench_tool::android::launcher;
use quench_tool::cli::LauncherChoice;
use quench_tool::plugins::{add, check, installed, list, names};
use quench_tool::project::Project;

/// A project with the Android host made, so plugins can be added.
fn host(label: &str) -> Project {
    let p = project_named(label, "my-app", "web");
    // The workspace the template makes: what `plugin add` edits.
    std::fs::write(
        p.root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"core\", \"app\", \"server\"]\n\n[workspace.dependencies]\nquench-ui = { path = \"/q/quench-ui\" }\nquench-core = { path = \"/q/quench-core\" }\n",
    )
    .unwrap();
    quench_tool::android::init(&mut Vec::new(), &p, Some("dev.lorehaven.myapp")).unwrap();
    p
}

fn read(p: &Project, path: &str) -> String {
    std::fs::read_to_string(p.root.join(path)).unwrap()
}

const SRC: &str = "android/app/src/main/kotlin/dev/lorehaven/myapp";

#[quench_test::test]
fn every_plugin_is_listed_and_checked() {
    assert_eq!(
        names(),
        [
            "native",
            "surface",
            "jobs",
            "device",
            "secure-storage",
            "notifications",
            "files",
            "packages",
            "media",
            "share",
            "widget"
        ]
    );
    assert!(check("jobs").is_ok());
    let e = check("teleport").unwrap_err().to_string();
    assert!(
        e.contains("no plugin `teleport`") && e.contains("secure-storage"),
        "{e}"
    );
}

#[quench_test::test]
fn adding_a_plugin_adds_what_it_needs_first_and_edits_the_project() {
    let p = host("plugins-jobs");
    let mut out = Vec::new();
    add(&mut out, &p, "jobs").unwrap();
    let said = text(out);
    assert!(
        said.find("added native").unwrap() < said.find("added jobs").unwrap(),
        "{said}"
    );
    assert_eq!(installed(&p), ["native", "jobs"]);
    // The Rust crate, its workspace entry and the crates it uses.
    assert!(
        p.root.join("native/Cargo.toml").is_file() && p.root.join("native/src/main.rs").is_file()
    );
    let cargo = read(&p, "Cargo.toml");
    assert!(cargo.contains("members = [\"native\", \"core\""), "{cargo}");
    assert!(
        cargo.contains("quench-android = { path = \"/q/quench-android\" }"),
        "from the same checkout: {cargo}"
    );
    assert!(cargo.contains("quench-serde = { path = \"/q/quench-serde\" }"));
    assert_eq!(
        cargo.matches("quench-core =").count(),
        1,
        "one that was there is not added again"
    );
    assert!(read(&p, "native/Cargo.toml").contains("name = \"my-app-native\""));
    // Kotlin, in the package's directory (and the fixed one for the JNI class).
    assert!(p.root.join(format!("{SRC}/JobsPlugin.kt")).is_file());
    assert!(
        p.root
            .join("android/app/src/main/kotlin/dev/quench/Native.kt")
            .is_file()
    );
    assert!(
        read(&p, &format!("{SRC}/QuenchJobService.kt")).starts_with("package dev.lorehaven.myapp")
    );
    // The manifest and the registry.
    let manifest = read(&p, "android/app/src/main/AndroidManifest.xml");
    assert!(
        manifest.contains("android.permission.RECEIVE_BOOT_COMPLETED"),
        "{manifest}"
    );
    assert!(
        manifest.contains("<service android:name=\".QuenchJobService\""),
        "{manifest}"
    );
    assert!(
        manifest.contains("<!-- quench:permissions -->")
            && manifest.contains("<!-- quench:components -->"),
        "markers stay"
    );
    assert!(read(&p, &format!("{SRC}/Plugins.kt")).contains("JobsPlugin(activity),"));
}

#[quench_test::test]
fn the_surface_plugin_adds_an_activity_and_the_program_that_draws_it() {
    let p = host("plugins-surface");
    add(&mut Vec::new(), &p, "surface").unwrap();
    assert_eq!(installed(&p), ["surface"]);
    for file in [
        "SurfaceHost.kt",
        "QuenchSurfaceView.kt",
        "SurfaceActivity.kt",
    ] {
        let kotlin = read(&p, &format!("{SRC}/{file}"));
        assert!(kotlin.starts_with("package dev.lorehaven.myapp"), "{file}");
    }
    assert!(read(&p, "surface/Cargo.toml").contains("name = \"my-app-surface\""));
    assert!(read(&p, "surface/src/main.rs").contains("quench_android_surface::run"));
    let cargo = read(&p, "Cargo.toml");
    assert!(
        cargo.contains("members = [\"surface\", \"core\""),
        "{cargo}"
    );
    assert!(
        cargo.contains("quench-android-surface = { path = \"/q/quench-android-surface\" }"),
        "{cargo}"
    );
    assert!(cargo.contains("quench-ui-native ="), "{cargo}");
    let manifest = read(&p, "android/app/src/main/AndroidManifest.xml");
    assert!(
        manifest.contains("<activity android:name=\".SurfaceActivity\""),
        "{manifest}"
    );
    assert!(
        manifest.contains("<!-- quench:components -->"),
        "the marker stays"
    );
    assert!(
        !read(&p, &format!("{SRC}/Plugins.kt")).contains("Surface"),
        "it is an activity, not a bridge plugin"
    );
}

#[quench_test::test]
fn adding_twice_changes_nothing() {
    let p = host("plugins-twice");
    add(&mut Vec::new(), &p, "notifications").unwrap();
    let before = (
        read(&p, "android/app/src/main/AndroidManifest.xml"),
        read(&p, &format!("{SRC}/Plugins.kt")),
        read(&p, "Cargo.toml"),
        read(&p, "android/.quench-plugins"),
    );
    let mut out = Vec::new();
    add(&mut out, &p, "notifications").unwrap();
    assert!(text(out).contains("notifications is already there"));
    let after = (
        read(&p, "android/app/src/main/AndroidManifest.xml"),
        read(&p, &format!("{SRC}/Plugins.kt")),
        read(&p, "Cargo.toml"),
        read(&p, "android/.quench-plugins"),
    );
    assert_eq!(before, after);
}

#[quench_test::test]
fn permissions_carry_their_attributes_and_snippets_the_package() {
    let p = host("plugins-attrs");
    for name in [
        "files",
        "notifications",
        "packages",
        "media",
        "share",
        "widget",
    ] {
        add(&mut Vec::new(), &p, name).unwrap();
    }
    let manifest = read(&p, "android/app/src/main/AndroidManifest.xml");
    assert!(
        manifest
            .contains("android.permission.READ_EXTERNAL_STORAGE\" android:maxSdkVersion=\"32\""),
        "{manifest}"
    );
    assert!(
        manifest.contains("android.permission.SCHEDULE_EXACT_ALARM\" android:maxSdkVersion=\"32\"")
    );
    assert_eq!(
        manifest.matches("RECEIVE_BOOT_COMPLETED").count(),
        1,
        "shared permissions once"
    );
    assert!(
        manifest.contains("dev.lorehaven.myapp.INSTALL_RESULT"),
        "the package reaches the snippet: {manifest}"
    );
    assert!(manifest.contains("<queries>") && manifest.contains("TTS_SERVICE"));
    // An intent filter goes inside the activity, a receiver inside the application.
    let activity =
        &manifest[manifest.find("<activity").unwrap()..manifest.find("</activity>").unwrap()];
    assert!(
        activity.contains("android.intent.action.SEND"),
        "{activity}"
    );
    assert!(
        manifest.contains("android:name=\".BootReceiver\"")
            && manifest.contains("QuenchWidgetProvider")
    );
    assert!(
        p.root
            .join("android/app/src/main/res/layout/quench_widget.xml")
            .is_file()
    );
    let registry = read(&p, &format!("{SRC}/Plugins.kt"));
    for want in [
        "FilesPlugin(activity)",
        "NotificationsPlugin(activity)",
        "TtsPlugin(activity)",
        "AudioPlugin(activity)",
        "IntentsPlugin()",
    ] {
        assert!(registry.contains(want), "{want} in {registry}");
    }
}

#[quench_test::test]
fn every_plugin_renders_without_template_syntax_left() {
    let p = host("plugins-all");
    for name in names() {
        add(&mut Vec::new(), &p, name).unwrap();
    }
    let mut checked = 0;
    for entry in walk(&p.root) {
        if entry
            .extension()
            .is_some_and(|e| e == "kt" || e == "xml" || e == "rs" || e == "toml")
        {
            let text = std::fs::read_to_string(&entry).unwrap();
            assert!(
                !text.contains("{{") && !text.contains("{%"),
                "{}",
                entry.display()
            );
            checked += 1;
        }
    }
    assert!(checked > 20, "{checked}");
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let path = e.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[quench_test::test]
fn the_registry_dependency_is_used_when_the_project_is_not_from_a_checkout() {
    let p = project_named("plugins-registry", "my-app", "web");
    std::fs::write(
        p.root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"core\"]\n\n[workspace.dependencies]\nquench-ui = { registry = \"ennor\", version = \"0.1.1\" }\n",
    )
    .unwrap();
    quench_tool::android::init(&mut Vec::new(), &p, Some("dev.lorehaven.myapp")).unwrap();
    add(&mut Vec::new(), &p, "native").unwrap();
    assert!(
        read(&p, "Cargo.toml")
            .contains("quench-android = { registry = \"ennor\", version = \"0.1.0\" }")
    );
}

#[quench_test::test]
fn it_needs_the_android_host_and_the_markers() {
    let bare = project("plugins-bare", "web");
    let e = add(&mut Vec::new(), &bare, "jobs").unwrap_err().to_string();
    assert!(e.contains("quench android init"), "{e}");
    let p = host("plugins-markers");
    let manifest = p.root.join("android/app/src/main/AndroidManifest.xml");
    let without = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace("<!-- quench:components -->", "");
    std::fs::write(&manifest, without).unwrap();
    let e = add(&mut Vec::new(), &p, "jobs").unwrap_err().to_string();
    assert!(e.contains("quench:components"), "{e}");
}

#[quench_test::test]
fn list_shows_what_the_project_has() {
    let p = host("plugins-list");
    add(&mut Vec::new(), &p, "device").unwrap();
    let mut out = Vec::new();
    list(&mut out, &p).unwrap();
    let said = text(out);
    assert!(
        said.lines()
            .any(|l| l.contains("added") && l.contains("device")),
        "{said}"
    );
    assert!(
        said.lines()
            .any(|l| !l.contains("added") && l.contains("widget")),
        "{said}"
    );
}

#[quench_test::test]
fn the_native_watchdog_cannot_take_the_app_down() {
    let p = host("plugins-watchdog");
    add(&mut Vec::new(), &p, "native").unwrap();
    let kotlin = read(&p, "android/app/src/main/kotlin/dev/quench/Native.kt");
    // An exception that escapes a thread ends the process: the thread that is interrupted when the
    // call is over (it was found by a device test) must catch the interruption.
    let watchdog = &kotlin[kotlin.find("val watchdog = thread").unwrap()..];
    assert!(
        watchdog[..=watchdog.find("try {").unwrap()].contains("thread"),
        "{kotlin}"
    );
    assert!(
        kotlin.contains("catch (_: InterruptedException)"),
        "{kotlin}"
    );
    // The stderr reader is blocked in a read when the process is destroyed.
    let reader =
        &kotlin[kotlin.find("val reader = thread").unwrap()..kotlin.find("val watchdog").unwrap()];
    assert!(
        reader.contains("try {") && reader.contains("catch (_: IOException)"),
        "{reader}"
    );
    assert!(
        kotlin.contains("ProcessBuilder") && !kotlin.contains("System.loadLibrary"),
        "a program, not a loaded library"
    );
}

#[quench_test::test]
fn the_launcher_moves_between_the_web_app_and_the_native_renderer() {
    let p = host("plugins-launcher");
    let manifest = |p: &Project| read(p, "android/app/src/main/AndroidManifest.xml");
    // Without the surface activity there is nothing to move to.
    let e = launcher(&mut Vec::new(), &p, LauncherChoice::Surface)
        .unwrap_err()
        .to_string();
    assert!(e.contains("plugin add surface"), "{e}");
    add(&mut Vec::new(), &p, "surface").unwrap();
    let launcher_in = |m: &str, name: &str| {
        let named = m.find(&format!("android:name=\"{name}\"")).unwrap();
        let start = m[..named].rfind("<activity").unwrap();
        let open = start + m[start..].find('>').unwrap();
        let end = if m[..open].ends_with('/') {
            open
        } else {
            open + m[open..].find("</activity>").unwrap()
        };
        m[start..end].contains("category.LAUNCHER")
    };
    assert!(launcher_in(&manifest(&p), ".MainActivity"));
    assert!(!launcher_in(&manifest(&p), ".SurfaceActivity"));
    let mut out = Vec::new();
    launcher(&mut out, &p, LauncherChoice::Surface).unwrap();
    assert!(text(out).contains(".SurfaceActivity"));
    let m = manifest(&p);
    assert!(
        launcher_in(&m, ".SurfaceActivity") && !launcher_in(&m, ".MainActivity"),
        "{m}"
    );
    assert_eq!(m.matches("category.LAUNCHER").count(), 1);
    assert!(
        m.contains("<!-- quench:components -->") && m.contains("<!-- quench:activity -->"),
        "markers stay: {m}"
    );
    // Again changes nothing, and back restores the web app (and the surface element to its short form).
    let before = manifest(&p);
    launcher(&mut Vec::new(), &p, LauncherChoice::Surface).unwrap();
    assert_eq!(manifest(&p), before);
    launcher(&mut Vec::new(), &p, LauncherChoice::Web).unwrap();
    let m = manifest(&p);
    assert!(
        launcher_in(&m, ".MainActivity") && !launcher_in(&m, ".SurfaceActivity"),
        "{m}"
    );
    assert_eq!(m.matches("category.LAUNCHER").count(), 1);
    assert!(m.contains(".SurfaceActivity"));
    // No android/ at all.
    let none = project("plugins-launcher-none", "web");
    assert!(launcher(&mut Vec::new(), &none, LauncherChoice::Web).is_err());
}
