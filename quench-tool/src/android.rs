//! `quench android ...`: the Android host of a project.

use std::io::Write;

use crate::error::{Result, fail};
use crate::project::Project;
use crate::system::{Cmd, System};
use crate::templates;

/// The application id used when none is given.
#[must_use]
pub fn default_package(project: &Project) -> String {
    format!("com.example.{}", project.snake())
}

/// `quench android init`: writes `android/`, the Gradle project of the host, next to the app.
///
/// # Errors
/// A bad package name, an `android/` directory that already exists, or a file that cannot be written.
pub fn init(out: &mut dyn Write, project: &Project, package: Option<&str>) -> Result<()> {
    let package = package.map_or_else(|| default_package(project), str::to_string);
    let root = project.root.join("android");
    if root.exists() {
        return fail(format!("{} exists already", root.display()));
    }
    let files = templates::render_android(&project.name, &package)?;
    for file in &files {
        let target = root.join(&file.path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, &file.bytes)?;
        #[cfg(unix)]
        if file.executable {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    // What the build and `quench publish` read.
    let manifest = project.root.join(crate::project::MANIFEST);
    let text = std::fs::read_to_string(&manifest)?;
    if !text.contains("[android]") {
        let table = format!(
            "\n[android]\npackage = \"{package}\"\nversion_name = \"0.1.0\"\nversion_code = 1\n"
        );
        std::fs::write(&manifest, text + &table)?;
    }
    writeln!(
        out,
        "created {} ({package}, {} files)",
        root.display(),
        files.len()
    )?;
    Ok(())
}

/// The Rust target and the folder name Android uses for each ABI.
pub const ABIS: &[(&str, &str)] = &[
    ("arm64-v8a", "aarch64-linux-android"),
    ("x86_64", "x86_64-linux-android"),
];

/// The API level the native library is linked for (the host's `minSdk`).
const API: &str = "26";

/// Where the NDK is: `ANDROID_NDK_HOME`, `ANDROID_NDK_ROOT`, or the newest under the SDK's `ndk/`.
///
/// # Errors
/// When there is none, with what to set.
pub fn ndk_home(sys: &dyn System) -> Result<std::path::PathBuf> {
    for var in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT"] {
        if let Some(path) = sys.var(var).map(std::path::PathBuf::from)
            && sys.exists(&path)
        {
            return Ok(path);
        }
    }
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(sdk) = sys.var(var) {
            let ndk = std::path::Path::new(&sdk).join("ndk");
            if let Some(newest) = sys.list_dir(&ndk).last() {
                return Ok(ndk.join(newest));
            }
        }
    }
    fail(
        "the Android NDK was not found: set ANDROID_NDK_HOME (`quench doctor` says what is missing)",
    )
}

fn host_tag() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin-x86_64",
        "windows" => "windows-x86_64",
        _ => "linux-x86_64",
    }
}

/// `quench android build-native`: builds the program in `native/` for each ABI with the NDK's linker
/// and puts it where Gradle packages it: `android/app/src/main/jniLibs/<abi>/libquench_native.so`.
///
/// # Errors
/// No `native/` (add the plugin), no NDK, an unknown ABI, a build that fails.
pub fn build_native(
    sys: &dyn System,
    out: &mut dyn Write,
    project: &Project,
    release: bool,
    abi: Option<&str>,
) -> Result<()> {
    // The programs this project has: the native operations, and the program that draws a screen.
    let programs: Vec<(&str, &str)> = [("native", "quench_native"), ("surface", "quench_surface")]
        .into_iter()
        .filter(|(dir, _)| project.root.join(dir).join("Cargo.toml").is_file())
        .collect();
    if programs.is_empty() {
        return fail("no native/ in this project: `quench plugin add native` makes it");
    }
    let abis: Vec<(&str, &str)> = match abi {
        Some(wanted) => match ABIS.iter().find(|(a, _)| *a == wanted) {
            Some(found) => vec![*found],
            None => {
                return fail(format!(
                    "unknown ABI `{wanted}`: {}",
                    ABIS.iter()
                        .map(|(a, _)| *a)
                        .collect::<Vec<_>>()
                        .join(" or ")
                ));
            }
        },
        None => ABIS.to_vec(),
    };
    let bin = ndk_home(sys)?
        .join("toolchains/llvm/prebuilt")
        .join(host_tag())
        .join("bin");
    let profile = if release { "release" } else { "debug" };
    for (abi, target) in abis {
        let linker = bin.join(format!("{target}{API}-clang"));
        let env_name = format!(
            "CARGO_TARGET_{}_LINKER",
            target.to_uppercase().replace('-', "_")
        );
        for (dir, binary) in &programs {
            let package = format!("{}-{dir}", project.name);
            let mut args = vec!["build", "-p", &package, "--target", target];
            if release {
                args.push("--release");
            }
            // Crates that build C code (`ring`, for TLS) find the NDK's compiler and archiver by these names.
            let underscored = target.replace('-', "_");
            let cmd = Cmd::new("cargo", &args, &project.root)
                .env(&env_name, &linker.to_string_lossy())
                .env(&format!("CC_{underscored}"), &linker.to_string_lossy())
                .env(
                    &format!("AR_{underscored}"),
                    &bin.join("llvm-ar").to_string_lossy(),
                );
            crate::release::step(sys, out, &cmd)?;
            // The program the host starts. Android only extracts files named lib*.so from the APK, so it
            // is packaged under that name.
            let built = project.target_dir().join(target).join(profile).join(binary);
            let dest_dir = project.root.join("android/app/src/main/jniLibs").join(abi);
            std::fs::create_dir_all(&dest_dir)?;
            let dest = dest_dir.join(format!("lib{binary}.so"));
            std::fs::copy(&built, &dest)
                .map_err(|e| crate::error::Error(format!("{}: {e}", built.display())))?;
            writeln!(out, "{dir} {abi}: {}", dest.display())?;
        }
    }
    Ok(())
}

const MAIN_ACTIVITY: &str = ".MainActivity";

/// The application id of the project's host: from `quench.toml`, else what `android/` was made with.
///
/// # Errors
/// When there is no Android host.
pub fn package(project: &Project) -> Result<String> {
    if let Some(p) = &project.android.package {
        return Ok(p.clone());
    }
    let gradle = std::fs::read_to_string(project.root.join("android/app/build.gradle.kts"))
        .map_err(|_| {
            crate::error::Error(
                "no android/ in this project: run `quench android init` first".into(),
            )
        })?;
    gradle
        .lines()
        .find_map(|l| l.trim().strip_prefix("namespace = \""))
        .and_then(|l| l.split('"').next())
        .map(str::to_string)
        .ok_or_else(|| crate::error::Error("android/app/build.gradle.kts has no namespace".into()))
}

/// `(versionName, versionCode)` as the build and the registry see them.
#[must_use]
pub fn version(project: &Project) -> (String, u32) {
    (
        project
            .android
            .version_name
            .clone()
            .unwrap_or_else(|| "0.1.0".to_string()),
        project.android.version_code.unwrap_or(1),
    )
}

/// Runs Gradle in `android/` for `tasks`. A release build signs with the key `quench.toml` names;
/// its passwords come from the environment, never from a file.
fn gradle(
    sys: &dyn System,
    out: &mut dyn Write,
    project: &Project,
    tasks: &[&str],
    release: bool,
) -> Result<()> {
    let (name, code) = version(project);
    let (code_arg, name_arg) = (
        format!("-Pquench.versionCode={code}"),
        format!("-Pquench.versionName={name}"),
    );
    let mut args = vec!["-q"];
    args.extend_from_slice(tasks);
    args.extend([code_arg.as_str(), name_arg.as_str()]);
    let mut cmd = Cmd::new("./gradlew", &args, &project.root.join("android"));
    if release {
        let (Some(keystore), Some(alias)) = (&project.android.keystore, &project.android.key_alias)
        else {
            return fail(
                "a release build is signed: set `keystore` and `key_alias` under [android] in quench.toml (`quench android keystore` makes a key)",
            );
        };
        if sys.var("QUENCH_KEYSTORE_PASSWORD").is_none() {
            return fail(
                "set QUENCH_KEYSTORE_PASSWORD (and QUENCH_KEY_PASSWORD if the key has its own) to sign the release",
            );
        }
        let path = project.root.join(keystore);
        if !sys.exists(&path) {
            return fail(format!(
                "the keystore {} does not exist: `quench android keystore` makes one",
                path.display()
            ));
        }
        cmd = cmd
            .env("QUENCH_KEYSTORE", &path.to_string_lossy())
            .env("QUENCH_KEY_ALIAS", alias);
    }
    crate::release::step(sys, out, &cmd)
}

/// The newest `apksigner` of the SDK's build tools.
fn apksigner(sys: &dyn System) -> Option<std::path::PathBuf> {
    ["ANDROID_HOME", "ANDROID_SDK_ROOT"].iter().find_map(|v| {
        let tools = std::path::Path::new(&sys.var(v)?).join("build-tools");
        let newest = sys
            .list_dir(&tools)
            .into_iter()
            .rfind(|d| sys.exists(&tools.join(d).join("apksigner")))?;
        Some(tools.join(newest).join("apksigner"))
    })
}

/// Checks the APK's signature and prints who signed it.
///
/// # Errors
/// When the signature is not valid, or there is no `apksigner` to check it with.
pub fn verify_apk(sys: &dyn System, out: &mut dyn Write, apk: &std::path::Path) -> Result<()> {
    let Some(tool) = apksigner(sys) else {
        return fail(
            "apksigner was not found in the SDK's build-tools: install them with sdkmanager",
        );
    };
    let Some(report) = sys.capture(
        &tool.to_string_lossy(),
        &["verify", "--print-certs", &apk.to_string_lossy()],
    ) else {
        return fail(format!(
            "{} is not signed with a valid signature",
            apk.display()
        ));
    };
    if let Some(line) = report.lines().find(|l| l.contains("certificate DN")) {
        writeln!(out, "signed: {}", line.trim())?;
    }
    Ok(())
}

/// `quench build apk|aab`: the bundle, the native libraries when there is native code, and Gradle.
/// Returns the file in `dist/`.
///
/// # Errors
/// A step that fails; a release without a key.
pub fn build_apk(
    sys: &dyn System,
    out: &mut dyn Write,
    project: &Project,
    release: bool,
    aab: bool,
    abi: Option<&str>,
) -> Result<std::path::PathBuf> {
    package(project)?;
    crate::release::bundle(sys, out, project)?;
    if project.root.join("native/Cargo.toml").is_file() {
        build_native(sys, out, project, release, abi)?;
    }
    let (task, built) = match (release, aab) {
        (_, true) => (
            "bundleRelease",
            "app/build/outputs/bundle/release/app-release.aab",
        ),
        (true, false) => (
            "assembleRelease",
            "app/build/outputs/apk/release/app-release.apk",
        ),
        (false, false) => ("assembleDebug", "app/build/outputs/apk/debug/app-debug.apk"),
    };
    gradle(sys, out, project, &[task], release || aab)?;
    let (name, code) = version(project);
    let dist = project.dist();
    std::fs::create_dir_all(&dist)?;
    let target = dist.join(format!(
        "{}-{name}-{code}.{}",
        project.name,
        if aab { "aab" } else { "apk" }
    ));
    let source = project.root.join("android").join(built);
    std::fs::copy(&source, &target)
        .map_err(|e| crate::error::Error(format!("{}: {e}", source.display())))?;
    if release && !aab {
        verify_apk(sys, out, &target)?;
    }
    writeln!(
        out,
        "{} ({} KiB)",
        target.display(),
        std::fs::metadata(&target).map_or(0, |m| m.len()) / 1024
    )?;
    Ok(target)
}

fn adb(project: &Project, args: &[&str]) -> Cmd {
    Cmd::new("adb", args, &project.root)
}

/// `quench run android`: builds the app for the attached device, installs it and starts it.
///
/// # Errors
/// No device, an ABI the tool cannot build, a step that fails.
pub fn run(
    sys: &dyn System,
    out: &mut dyn Write,
    project: &Project,
    abi: Option<&str>,
) -> Result<()> {
    let pkg = package(project)?;
    let devices = sys.capture("adb", &["devices"]).unwrap_or_default();
    if !devices.lines().any(|l| l.ends_with("\tdevice")) {
        return fail(
            "no device: attach one or start an emulator (`adb devices` lists what is there)",
        );
    }
    let abi = abi.map_or_else(
        || {
            sys.capture("adb", &["shell", "getprop", "ro.product.cpu.abi"])
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        },
        str::to_string,
    );
    if project.root.join("native/Cargo.toml").is_file() && !ABIS.iter().any(|(a, _)| *a == abi) {
        return fail(format!(
            "the device's ABI `{abi}` is not one this tool builds native code for"
        ));
    }
    crate::release::bundle(sys, out, project)?;
    if project.root.join("native/Cargo.toml").is_file() {
        build_native(sys, out, project, false, Some(&abi))?;
    }
    gradle(sys, out, project, &["installDebug"], false)?;
    let component = format!("{pkg}/{MAIN_ACTIVITY}");
    crate::release::step(
        sys,
        out,
        &adb(
            project,
            &[
                "shell", "am", "start", "-n", &component, "--es", "dev_url", "off",
            ],
        ),
    )?;
    writeln!(out, "running {pkg} on the device")?;
    Ok(())
}

/// For `quench dev --android`: lets the device reach the dev server (`adb reverse`) and starts the
/// app on it. Said, not fatal, when it cannot: the server is running either way.
///
/// # Errors
/// Output that cannot be written.
pub fn launch_dev(
    sys: &dyn System,
    out: &mut dyn Write,
    project: &Project,
    port: u16,
) -> Result<()> {
    let mut attempt = || -> Result<()> {
        let pkg = package(project)?;
        // The device must not meet a server that is still starting: its page would be an error with
        // no reload script to recover it.
        if !sys.wait_port(port, 15_000) {
            return fail(format!("the server did not start listening on port {port}"));
        }
        let at = format!("tcp:{port}");
        crate::release::step(sys, out, &adb(project, &["reverse", &at, &at]))?;
        let url = format!("http://127.0.0.1:{port}");
        let component = format!("{pkg}/{MAIN_ACTIVITY}");
        crate::release::step(
            sys,
            out,
            &adb(
                project,
                &[
                    "shell", "am", "start", "-n", &component, "--es", "dev_url", &url,
                ],
            ),
        )
    };
    if let Err(e) = attempt() {
        writeln!(out, "the device did not start: {e}")?;
    }
    Ok(())
}

/// `quench android keystore`: a release key, with `keytool`. The passwords are read from the
/// environment (`QUENCH_KEYSTORE_PASSWORD`, and `QUENCH_KEY_PASSWORD` if the key has its own).
///
/// # Errors
/// An existing keystore, no password, or `keytool` failing.
pub fn keystore(
    sys: &dyn System,
    out: &mut dyn Write,
    project: &Project,
    path: &str,
    alias: &str,
) -> Result<()> {
    let target = project.root.join(path);
    if sys.exists(&target) {
        return fail(format!(
            "{} exists already: a release key is made once and kept",
            target.display()
        ));
    }
    let Some(password) = sys.var("QUENCH_KEYSTORE_PASSWORD") else {
        return fail(
            "set QUENCH_KEYSTORE_PASSWORD (at least 6 characters) to protect the keystore",
        );
    };
    if password.len() < 6 {
        return fail("the keystore password needs at least 6 characters");
    }
    let dname = format!("CN={}", project.name);
    let path_arg = target.to_string_lossy().into_owned();
    let mut args = vec![
        "-genkeypair",
        "-keystore",
        &path_arg,
        "-alias",
        alias,
        "-keyalg",
        "RSA",
        "-keysize",
        "4096",
        "-validity",
        "10000",
        "-dname",
        &dname,
        "-storepass:env",
        "QUENCH_KEYSTORE_PASSWORD",
    ];
    if sys.var("QUENCH_KEY_PASSWORD").is_some() {
        args.extend(["-keypass:env", "QUENCH_KEY_PASSWORD"]);
    }
    crate::release::step(sys, out, &Cmd::new("keytool", &args, &project.root))?;
    // Where the build looks for it: under [android] in quench.toml.
    let manifest = project.root.join(crate::project::MANIFEST);
    let text = std::fs::read_to_string(&manifest)?;
    if !text.contains("keystore =") {
        let lines = format!("keystore = \"{path}\"\nkey_alias = \"{alias}\"\n");
        let new = text.find("[android]\n").map_or_else(
            || format!("{text}\n[android]\n{lines}"),
            |at| {
                let end = at + "[android]\n".len();
                format!("{}{lines}{}", &text[..end], &text[end..])
            },
        );
        std::fs::write(&manifest, new)?;
    }
    writeln!(
        out,
        "made {path_arg} and named it in quench.toml.\nKeep the keystore and its password safe, and out of git: a lost key means a new app."
    )?;
    Ok(())
}

const LAUNCHER_FILTER: &str = "<intent-filter>
                <action android:name=\"android.intent.action.MAIN\" />
                <category android:name=\"android.intent.category.LAUNCHER\" />
            </intent-filter>";

/// The byte range of the `<activity ...>` element named `.name`, and whether it is written `<activity ... />`.
fn activity_span(manifest: &str, name: &str) -> Option<(usize, usize, bool)> {
    let named = manifest.find(&format!("android:name=\"{name}\""))?;
    let start = manifest[..named].rfind("<activity")?;
    let open_end = start + manifest[start..].find('>')?;
    if manifest[..open_end].ends_with('/') {
        return Some((start, open_end + 1, true));
    }
    let close = open_end + manifest[open_end..].find("</activity>")? + "</activity>".len();
    Some((start, close, false))
}

/// Makes `which` the activity the launcher starts.
///
/// The launcher intent filter moves between `.MainActivity` (the web app) and `.SurfaceActivity` (the native
/// renderer, added by `quench plugin add surface`).
///
/// # Errors
/// No Android host, no surface plugin, or a manifest that has lost its activities.
pub fn launcher(
    out: &mut dyn Write,
    project: &Project,
    which: crate::cli::LauncherChoice,
) -> Result<()> {
    let path = project
        .root
        .join("android/app/src/main/AndroidManifest.xml");
    let Ok(mut manifest) = std::fs::read_to_string(&path) else {
        return fail("no android/ in this project: `quench android init` makes it");
    };
    let (to, from) = match which {
        crate::cli::LauncherChoice::Surface => (".SurfaceActivity", ".MainActivity"),
        crate::cli::LauncherChoice::Web => (".MainActivity", ".SurfaceActivity"),
    };
    if activity_span(&manifest, to).is_none() {
        return fail(if to == ".SurfaceActivity" {
            "this project has no surface activity: `quench plugin add surface` adds it".to_string()
        } else {
            format!("the manifest has no {to}")
        });
    }
    // Take it off the other activity (an element that is left empty goes back to `<activity ... />`).
    if let Some((start, end, _)) = activity_span(&manifest, from) {
        let element = manifest[start..end].to_string();
        if let Some(at) = element
            .find("<intent-filter>")
            .filter(|_| element.contains("category.LAUNCHER"))
        {
            let filter_end = at
                + element[at..]
                    .find("</intent-filter>")
                    .map_or(0, |e| e + "</intent-filter>".len());
            let mut changed = element.clone();
            changed.replace_range(at..filter_end, "");
            let leftover = changed
                .find('>')
                .map(|open| changed[open + 1..].replace("</activity>", ""))
                .unwrap_or_default();
            if leftover.trim().is_empty()
                && !changed.contains("<!--")
                && let Some(open) = changed.find('>')
            {
                changed = format!("{} />", changed[..open].trim_end());
            }
            manifest.replace_range(start..end, &changed);
        }
    }
    // Put it on this one, unless it is there.
    if let Some((start, end, self_closing)) = activity_span(&manifest, to) {
        let element = manifest[start..end].to_string();
        if !element.contains("category.LAUNCHER") {
            let changed = if self_closing {
                let head = element.trim_end_matches("/>").trim_end();
                format!("{head}>\n            {LAUNCHER_FILTER}\n        </activity>")
            } else if let Some(marker) = element.find("<!-- quench:activity -->") {
                let mut e = element;
                e.insert_str(marker, &format!("{LAUNCHER_FILTER}\n            "));
                e
            } else {
                let close = element.rfind("</activity>").unwrap_or(element.len());
                let mut e = element;
                e.insert_str(close, &format!("    {LAUNCHER_FILTER}\n        "));
                e
            };
            manifest.replace_range(start..end, &changed);
        }
    }
    std::fs::write(&path, manifest)?;
    writeln!(out, "the launcher starts {to}")?;
    Ok(())
}
