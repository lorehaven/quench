//! `quench plugin ...`: native capabilities for the Android host.
//!
//! A plugin is a few Kotlin files (and sometimes a Rust crate), what the manifest needs, and a line
//! in `plugins()`; adding one writes all of that into the project. The Rust client of each plugin
//! is in `quench_ui_web::plugins`.

use std::io::Write;
use std::path::Path;

use quench_serde::json;
use quench_template::Environment;

use crate::error::{Error, Result};
use crate::project::Project;
use crate::versions;

/// One plugin and what adding it changes.
struct Plugin {
    name: &'static str,
    about: &'static str,
    /// Plugins this one is no use without; added first.
    needs: &'static [&'static str],
    /// Files to write, relative to the project root. `@package_path@` is the package as a path.
    files: &'static [(&'static str, &'static [u8])],
    /// Permissions for the manifest, with the attributes they need (`maxSdkVersion`).
    permissions: &'static [(&'static str, &'static str)],
    /// Other elements for the top of the manifest (`<queries>`).
    top: &'static str,
    /// Elements for the manifest's `<application>`.
    components: &'static str,
    /// Elements for the main activity (`<intent-filter>`).
    activity: &'static str,
    /// Kotlin expressions to add to `plugins(activity)`.
    register: &'static [&'static str],
    /// A crate to add to the workspace members.
    member: Option<&'static str>,
    /// Quench crates the workspace needs to depend on.
    crates: &'static [&'static str],
}

macro_rules! file {
    ($path:expr, $template:expr) => {
        (
            $path,
            include_bytes!(concat!("../templates/android/plugins/", $template)) as &[u8],
        )
    };
}

const PLUGINS: &[Plugin] = &[
    Plugin {
        name: "native",
        about: "the app's Rust code on the device, as a program the host runs (needs the NDK)",
        needs: &[],
        files: &[
            file!(
                "android/app/src/main/kotlin/dev/quench/Native.kt",
                "native/kotlin/Native.kt.tpl"
            ),
            file!("native/Cargo.toml", "native/crate/Cargo.toml.tpl"),
            file!("native/src/main.rs", "native/crate/main.rs.tpl"),
        ],
        permissions: &[],
        top: "",
        components: "",
        activity: "",
        register: &[],
        member: Some("native"),
        crates: &["quench-android", "quench-core", "quench-serde"],
    },
    Plugin {
        name: "surface",
        about: "an activity whose screen is drawn by the app's Rust program with no WebView (the native renderer)",
        needs: &[],
        files: &[
            file!(
                "android/app/src/main/kotlin/@package_path@/SurfaceHost.kt",
                "surface/kotlin/SurfaceHost.kt.tpl"
            ),
            file!(
                "android/app/src/main/kotlin/@package_path@/QuenchSurfaceView.kt",
                "surface/kotlin/QuenchSurfaceView.kt.tpl"
            ),
            file!(
                "android/app/src/main/kotlin/@package_path@/SurfaceActivity.kt",
                "surface/kotlin/SurfaceActivity.kt.tpl"
            ),
            file!("surface/Cargo.toml", "surface/crate/Cargo.toml.tpl"),
            file!("surface/src/main.rs", "surface/crate/main.rs.tpl"),
        ],
        permissions: &[],
        top: "",
        components: "<activity android:name=\".SurfaceActivity\" android:exported=\"true\" android:windowSoftInputMode=\"adjustResize\" android:configChanges=\"orientation|screenSize|smallestScreenSize|screenLayout|keyboard|keyboardHidden|uiMode|density|fontScale|layoutDirection|locale\" />",
        activity: "",
        register: &[],
        member: Some("surface"),
        crates: &[
            "quench-android-surface",
            "quench-style",
            "quench-ui",
            "quench-ui-native",
        ],
    },
    Plugin {
        name: "jobs",
        about: "background work: scheduled tasks run by native code with no page open, with constraints",
        needs: &["native"],
        files: &[
            file!(
                "android/app/src/main/kotlin/@package_path@/JobsPlugin.kt",
                "jobs/JobsPlugin.kt.tpl"
            ),
            file!(
                "android/app/src/main/kotlin/@package_path@/QuenchJobService.kt",
                "jobs/QuenchJobService.kt.tpl"
            ),
        ],
        permissions: &[("android.permission.RECEIVE_BOOT_COMPLETED", "")],
        top: "",
        activity: "",
        components: "<service android:name=\".QuenchJobService\" android:permission=\"android.permission.BIND_JOB_SERVICE\" android:exported=\"false\" />",
        register: &["JobsPlugin(activity)"],
        member: None,
        crates: &[],
    },
    Plugin {
        name: "device",
        about: "the network, the battery and power saving, once or as events",
        needs: &[],
        files: &[file!(
            "android/app/src/main/kotlin/@package_path@/DevicePlugin.kt",
            "device/DevicePlugin.kt.tpl"
        )],
        permissions: &[("android.permission.ACCESS_NETWORK_STATE", "")],
        top: "",
        components: "",
        activity: "",
        register: &["DevicePlugin(activity)"],
        member: None,
        crates: &[],
    },
    Plugin {
        name: "secure-storage",
        about: "small secrets encrypted with a key that stays in the Android Keystore",
        needs: &[],
        files: &[file!(
            "android/app/src/main/kotlin/@package_path@/SecureStoragePlugin.kt",
            "secure-storage/SecureStoragePlugin.kt.tpl"
        )],
        permissions: &[],
        top: "",
        components: "",
        activity: "",
        register: &["SecureStoragePlugin(activity)"],
        member: None,
        crates: &[],
    },
    Plugin {
        name: "notifications",
        about: "notifications and reminders that survive a reboot, repeating by clock time in a time zone",
        needs: &["native"],
        files: &[file!(
            "android/app/src/main/kotlin/@package_path@/NotificationsPlugin.kt",
            "notifications/NotificationsPlugin.kt.tpl"
        )],
        permissions: &[
            ("android.permission.POST_NOTIFICATIONS", ""),
            ("android.permission.RECEIVE_BOOT_COMPLETED", ""),
            ("android.permission.USE_EXACT_ALARM", ""),
            (
                "android.permission.SCHEDULE_EXACT_ALARM",
                "android:maxSdkVersion=\"32\"",
            ),
        ],
        top: "",
        components: "<receiver android:name=\".ReminderReceiver\" android:exported=\"false\" />\n        <receiver android:name=\".BootReceiver\" android:exported=\"true\">\n            <intent-filter>\n                <action android:name=\"android.intent.action.BOOT_COMPLETED\" />\n                <action android:name=\"android.intent.action.TIMEZONE_CHANGED\" />\n                <action android:name=\"android.intent.action.TIME_SET\" />\n                <action android:name=\"android.intent.action.MY_PACKAGE_REPLACED\" />\n            </intent-filter>\n        </receiver>",
        activity: "",
        register: &["NotificationsPlugin(activity)"],
        member: None,
        crates: &[],
    },
    Plugin {
        name: "files",
        about: "the media library, folders and files the person picks, reading in chunks, streaming uploads",
        needs: &[],
        files: &[file!(
            "android/app/src/main/kotlin/@package_path@/FilesPlugin.kt",
            "files/FilesPlugin.kt.tpl"
        )],
        permissions: &[
            ("android.permission.READ_MEDIA_IMAGES", ""),
            ("android.permission.READ_MEDIA_VIDEO", ""),
            ("android.permission.READ_MEDIA_AUDIO", ""),
            (
                "android.permission.READ_EXTERNAL_STORAGE",
                "android:maxSdkVersion=\"32\"",
            ),
        ],
        top: "",
        components: "",
        activity: "",
        register: &["FilesPlugin(activity)"],
        member: None,
        crates: &[],
    },
    Plugin {
        name: "packages",
        about: "installing and updating apps from a downloaded APK (see the Play policy note)",
        needs: &[],
        files: &[file!(
            "android/app/src/main/kotlin/@package_path@/PackagesPlugin.kt",
            "packages/PackagesPlugin.kt.tpl"
        )],
        permissions: &[("android.permission.REQUEST_INSTALL_PACKAGES", "")],
        top: "",
        components: "<receiver android:name=\".InstallReceiver\" android:exported=\"false\">\n            <intent-filter>\n                <action android:name=\"{{ package }}.INSTALL_RESULT\" />\n            </intent-filter>\n        </receiver>",
        activity: "",
        register: &["PackagesPlugin(activity)"],
        member: None,
        crates: &[],
    },
    Plugin {
        name: "media",
        about: "text to speech and audio playback",
        needs: &[],
        files: &[
            file!(
                "android/app/src/main/kotlin/@package_path@/TtsPlugin.kt",
                "media/TtsPlugin.kt.tpl"
            ),
            file!(
                "android/app/src/main/kotlin/@package_path@/AudioPlugin.kt",
                "media/AudioPlugin.kt.tpl"
            ),
        ],
        permissions: &[],
        top: "<queries>\n        <intent>\n            <action android:name=\"android.intent.action.TTS_SERVICE\" />\n        </intent>\n    </queries>",
        components: "",
        activity: "",
        register: &["TtsPlugin(activity)", "AudioPlugin(activity)"],
        member: None,
        crates: &[],
    },
    Plugin {
        name: "share",
        about: "sharing to and opening in other apps, and receiving shares and opened files",
        needs: &[],
        files: &[
            file!(
                "android/app/src/main/kotlin/@package_path@/SharePlugin.kt",
                "share/SharePlugin.kt.tpl"
            ),
            file!(
                "android/app/src/main/kotlin/@package_path@/IntentsPlugin.kt",
                "share/IntentsPlugin.kt.tpl"
            ),
        ],
        permissions: &[],
        top: "",
        components: "",
        activity: "<intent-filter>\n                <action android:name=\"android.intent.action.SEND\" />\n                <category android:name=\"android.intent.category.DEFAULT\" />\n                <data android:mimeType=\"text/plain\" />\n            </intent-filter>",
        register: &["SharePlugin(activity)", "IntentsPlugin()"],
        member: None,
        crates: &[],
    },
    Plugin {
        name: "widget",
        about: "a home-screen widget the app can update",
        needs: &[],
        files: &[
            file!(
                "android/app/src/main/kotlin/@package_path@/WidgetPlugin.kt",
                "widget/WidgetPlugin.kt.tpl"
            ),
            file!(
                "android/app/src/main/res/layout/quench_widget.xml",
                "widget/res/quench_widget.xml.tpl"
            ),
            file!(
                "android/app/src/main/res/xml/quench_widget_info.xml",
                "widget/res/quench_widget_info.xml.tpl"
            ),
        ],
        permissions: &[],
        top: "",
        components: "<receiver android:name=\".QuenchWidgetProvider\" android:exported=\"true\">\n            <intent-filter>\n                <action android:name=\"android.appwidget.action.APPWIDGET_UPDATE\" />\n            </intent-filter>\n            <meta-data android:name=\"android.appwidget.provider\" android:resource=\"@xml/quench_widget_info\" />\n        </receiver>",
        activity: "",
        register: &["WidgetPlugin(activity)"],
        member: None,
        crates: &[],
    },
];

/// The plugin names, in the order they are listed.
#[must_use]
pub fn names() -> Vec<&'static str> {
    PLUGINS.iter().map(|p| p.name).collect()
}

fn find(name: &str) -> Result<&'static Plugin> {
    PLUGINS.iter().find(|p| p.name == name).ok_or_else(|| {
        Error(format!(
            "no plugin `{name}`; they are: {}",
            names().join(", ")
        ))
    })
}

/// The plugins already added, from `android/.quench-plugins`.
#[must_use]
pub fn installed(project: &Project) -> Vec<String> {
    std::fs::read_to_string(project.root.join("android/.quench-plugins"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .filter(|l| !l.is_empty())
        .collect()
}

/// `quench plugin list`.
///
/// # Errors
/// Output that cannot be written.
pub fn list(out: &mut dyn Write, project: &Project) -> Result<()> {
    let have = installed(project);
    for p in PLUGINS {
        let mark = if have.iter().any(|h| h == p.name) {
            "added"
        } else {
            "     "
        };
        writeln!(out, "  {mark}  {:<10} {}", p.name, p.about)?;
    }
    Ok(())
}

/// The application id the host was made with.
fn package_of(project: &Project) -> Result<String> {
    let gradle = std::fs::read_to_string(project.root.join("android/app/build.gradle.kts"))
        .map_err(|_| {
            Error("no android/ in this project: run `quench android init` first".into())
        })?;
    gradle
        .lines()
        .find_map(|l| l.trim().strip_prefix("namespace = \""))
        .and_then(|l| l.split('"').next())
        .map(str::to_string)
        .ok_or_else(|| Error("android/app/build.gradle.kts has no namespace".into()))
}

fn insert_before(text: &str, marker: &str, addition: &str) -> Result<String> {
    let at = text.find(marker).ok_or_else(|| {
        Error(format!(
            "the marker `{marker}` is gone from the file; add it back where plugins go"
        ))
    })?;
    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let indent = &text[line_start..at];
    Ok(format!(
        "{}{indent}{addition}\n{}",
        &text[..line_start],
        &text[line_start..]
    ))
}

fn edit(path: &Path, change: impl FnOnce(String) -> Result<String>) -> Result<()> {
    let text =
        std::fs::read_to_string(path).map_err(|e| Error(format!("{}: {e}", path.display())))?;
    let new = change(text.clone())?;
    if new != text {
        std::fs::write(path, new)?;
    }
    Ok(())
}

/// Makes sure the workspace depends on `name` the way it depends on the other quench crates: from
/// the same checkout, or from the registry.
pub(crate) fn ensure_dependency(cargo: &str, name: &str) -> Result<String> {
    if cargo.contains(&format!("\n{name} = ")) {
        return Ok(cargo.to_string());
    }
    let path = cargo.lines().find_map(|l| {
        let rest = l.strip_prefix("quench-ui = { path = \"")?;
        rest.strip_suffix("/quench-ui\" }")
    });
    let line = versions::dependency_lines(path)
        .lines()
        .find(|l| l.starts_with(&format!("{name} = ")))
        .map(str::to_string)
        .ok_or_else(|| Error(format!("no version known for {name}")))?;
    let at = cargo
        .find("[workspace.dependencies]\n")
        .ok_or_else(|| Error("Cargo.toml has no [workspace.dependencies]".into()))?
        + "[workspace.dependencies]\n".len();
    Ok(format!("{}{line}\n{}", &cargo[..at], &cargo[at..]))
}

/// A manifest snippet with the package filled in.
fn snippets(snippet: &str, context: &quench_serde::Value) -> Result<String> {
    let mut env = Environment::new();
    env.add_template("snippet", snippet)
        .map_err(|e| Error(format!("manifest snippet: {e}")))?;
    env.render("snippet", context)
        .map_err(|e| Error(format!("manifest snippet: {e}")))
}

fn apply(out: &mut dyn Write, project: &Project, package: &str, p: &Plugin) -> Result<()> {
    let context = json!({ "name": project.name, "snake": project.snake(), "package": package });
    let package_path = package.replace('.', "/");
    let mut env = Environment::new();
    for (path, body) in p.files {
        let target = project
            .root
            .join(path.replace("@package_path@", &package_path));
        if target.exists() {
            continue;
        }
        let text = std::str::from_utf8(body).map_err(|e| Error(format!("{path}: {e}")))?;
        env.add_template(path, text)
            .map_err(|e| Error(format!("template {path}: {e}")))?;
        let rendered = env
            .render(path, &context)
            .map_err(|e| Error(format!("template {path}: {e}")))?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, rendered)?;
    }
    let manifest = project
        .root
        .join("android/app/src/main/AndroidManifest.xml");
    edit(&manifest, |mut text| {
        for (permission, attributes) in p.permissions {
            if !text.contains(&format!("\"{permission}\"")) {
                let extra = if attributes.is_empty() {
                    String::new()
                } else {
                    format!(" {attributes}")
                };
                text = insert_before(
                    &text,
                    "<!-- quench:permissions -->",
                    &format!("<uses-permission android:name=\"{permission}\"{extra} />"),
                )?;
            }
        }
        for (snippet, marker) in [
            (p.top, "<!-- quench:permissions -->"),
            (p.components, "<!-- quench:components -->"),
            (p.activity, "<!-- quench:activity -->"),
        ] {
            if snippet.is_empty() {
                continue;
            }
            let rendered = snippets(snippet, &context)?;
            if !text.contains(&rendered) {
                text = insert_before(&text, marker, &rendered)?;
            }
        }
        Ok(text)
    })?;
    let registry = project.root.join(format!(
        "android/app/src/main/kotlin/{package_path}/Plugins.kt"
    ));
    edit(&registry, |mut text| {
        for expression in p.register {
            if !text.contains(expression) {
                text = insert_before(&text, "// quench:plugins", &format!("{expression},"))?;
            }
        }
        Ok(text)
    })?;
    let cargo = project.root.join("Cargo.toml");
    edit(&cargo, |mut text| {
        for name in p.crates {
            text = ensure_dependency(&text, name)?;
        }
        if let Some(member) = p.member
            && !text.contains(&format!("\"{member}\""))
        {
            text = text.replacen("members = [", &format!("members = [\"{member}\", "), 1);
        }
        Ok(text)
    })?;
    let list = project.root.join("android/.quench-plugins");
    let mut have = installed(project);
    if !have.iter().any(|h| h == p.name) {
        have.push(p.name.to_string());
        std::fs::write(list, have.join("\n") + "\n")?;
    }
    writeln!(out, "added {}", p.name)?;
    Ok(())
}

/// `quench plugin add <name>`: the plugin and what it needs, once each.
///
/// # Errors
/// An unknown plugin, a project without `android/`, or a file that cannot be written.
pub fn add(out: &mut dyn Write, project: &Project, name: &str) -> Result<()> {
    let package = package_of(project)?;
    let mut order: Vec<&'static Plugin> = Vec::new();
    let mut pending = vec![find(name)?];
    while let Some(p) = pending.pop() {
        if order.iter().any(|o| o.name == p.name) {
            continue;
        }
        for need in p.needs {
            pending.push(find(need)?);
        }
        order.push(p);
    }
    // Dependencies first: `order` has each plugin before the ones that needed it reversed.
    order.reverse();
    let mut seen = Vec::new();
    for p in order {
        if seen.contains(&p.name) {
            continue;
        }
        seen.push(p.name);
        if installed(project).iter().any(|h| h == p.name) {
            writeln!(out, "{} is already there", p.name)?;
            continue;
        }
        apply(out, project, &package, p)?;
    }
    if order_needs_native(name) {
        writeln!(out, "next: quench android build-native")?;
    }
    Ok(())
}

/// Writes the host code of a plugin again from this tool's templates, for when the crates were upgraded and the
/// Kotlin copied into the project at `plugin add` is older than the Rust side it talks to.
///
/// Only the Kotlin and resources under `android/app/src/main` are replaced; the project's own Rust (`surface/`,
/// `native/`), the manifest and the registry of plugins are left as they are. `name` is one plugin, or all that
/// the project has.
///
/// # Errors
/// No plugin of that name, or one the project does not have, or a file that cannot be written.
pub fn update(out: &mut dyn Write, project: &Project, name: Option<&str>) -> Result<()> {
    let package = package_of(project)?;
    let have = installed(project);
    let chosen: Vec<&'static Plugin> = match name {
        Some(n) => {
            let p = find(n)?;
            if !have.iter().any(|h| h == p.name) {
                return Err(Error(format!(
                    "this project does not have {n}: `quench plugin add {n}`"
                )));
            }
            vec![p]
        }
        None => PLUGINS
            .iter()
            .filter(|p| have.iter().any(|h| h == p.name))
            .collect(),
    };
    let context = json!({ "name": project.name, "snake": project.snake(), "package": package });
    let package_path = package.replace('.', "/");
    let mut changed = 0;
    for p in chosen {
        let mut env = Environment::new();
        for (path, body) in p.files {
            if !path.starts_with("android/app/src/main/") || path.ends_with("AndroidManifest.xml") {
                continue;
            }
            let target = project
                .root
                .join(path.replace("@package_path@", &package_path));
            let text = std::str::from_utf8(body).map_err(|e| Error(format!("{path}: {e}")))?;
            env.add_template(path, text)
                .map_err(|e| Error(format!("template {path}: {e}")))?;
            let rendered = env
                .render(path, &context)
                .map_err(|e| Error(format!("template {path}: {e}")))?;
            if std::fs::read_to_string(&target).ok().as_deref() == Some(rendered.as_str()) {
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, rendered)?;
            writeln!(
                out,
                "updated {}",
                path.replace("@package_path@", &package_path)
            )?;
            changed += 1;
        }
    }
    if changed == 0 {
        writeln!(out, "the plugins' host code is up to date")?;
    }
    Ok(())
}

/// Whether adding `name` brings in native code (the plugin is `native` or needs it).
fn order_needs_native(name: &str) -> bool {
    find(name).is_ok_and(|p| p.name == "native" || p.needs.contains(&"native"))
}

/// Fails unless `name` is a plugin.
///
/// # Errors
/// The list of plugins.
pub fn check(name: &str) -> Result<()> {
    find(name).map(|_| ())
}
