---
title: Configuration
parent: quench-tool
grand_parent: Crates
nav_order: 2
---

## `.quench.toml`

Read by every command except `new` and `doctor`; found by looking in the current directory and then upwards. Parsed with `quench_serde::toml`. Unknown keys are not rejected by the code (*Unverified:* whether the TOML deserializer ignores unknown fields; the struct has no `deny_unknown_fields`).

```toml
# Read by the `quench` command. Packages are demo-app, demo-server and so on.
name = "demo"
template = "web"

[android]
package = "org.example.demo"
version_name = "0.1.0"
version_code = 1
keystore = "release.keystore"
key_alias = "release"

[linux]
id = "org.example.demo"
title = "Demo"
bin = "demo-desktop"
depends = "libwebkit2gtk-4.1-0, libgtk-3-0"
wm_class = "demo"
icon = "assets/icon.png"
```

### Top-level keys (both required)

| Key | Type | Meaning |
|---|---|---|
| `name` | string | The project name; validated like `quench new`'s `<NAME>` (lowercase letter first; lowercase letters, digits, `-`, `_`; at most 40; not starting with `quench`). Failure: `.quench.toml: <reason>`. |
| `template` | string | `web` or `fullstack`; anything else: `.quench.toml: unknown template `<x>` (web or fullstack)`. |

### `[android]` (optional table; every key optional)

| Key | Type | Default | Meaning |
|---|---|---|---|
| `package` | string | read from `namespace = "..."` in `android/app/build.gradle.kts` | The application id. Written by `quench android init`. |
| `version_name` | string | the project's Cargo version (see below) | `versionName` of the APK and the part of the file name `dist/<name>-<version_name>-<version_code>.apk`. |
| `version_code` | integer (u32) | derived from the version name: `major * 1_000_000 + minor * 1_000 + patch` | Android `versionCode`. Must go up with every release. |
| `keystore` | string | none | Path of the release keystore, relative to the project root. Written by `quench android keystore`. |
| `key_alias` | string | none | The key's name inside the keystore. Both `keystore` and `key_alias` are required for `--release`/`aab`. |

The passwords are never in the file: see the environment variables below.

### `[linux]` (optional table; every key optional)

| Key | Type | Default | Meaning |
|---|---|---|---|
| `id` | string | `com.example.<snake name>` | Application id for the keyring (`secret-tool` attribute `app`), notifications (`--app-name`), the menu entry file name `/usr/share/applications/<id>.desktop` and the icon file name. |
| `title` | string | the project's name | Window title; menu entry `Name=`; package summary and `.deb` `Description`. |
| `bin` | string | `<name>-desktop` | The host binary's name when `desktop/Cargo.toml` names its `[[bin]]` differently. |
| `depends` | string | `libwebkit2gtk-4.1-0, libgtk-3-0` | The `.deb`'s `Depends:`; the empty string omits the line (a native app has none). Used by `deb` only (the RPM and Arch packages declare no dependencies). |
| `wm_class` | string | the project's name | `StartupWMClass` of the menu entry; for a native window it is the application id. |
| `icon` | string | none | A square PNG (path from the project root) installed as `/usr/share/icons/hicolor/<w>x<w>/apps/<id>.png` and named by `Icon=<id>` in the menu entry. Errors: missing file, not a PNG (8-byte signature), not square (`[linux] icon <name> is <w>x<h>: it must be square`). |

### Version resolution

The project version used for `.deb`, `.rpm`, Arch and source file names, and for the Android defaults, is read from, in order: `[workspace.package]` `version = "..."` in `<root>/Cargo.toml`; `[package]` `version` in `<root>/desktop/Cargo.toml`; `[package]` `version` in `<root>/surface/Cargo.toml`; else `0.1.0`. (The line must start with `version = ` at the start of a line within the section.)

## Environment variables

| Variable | Read by | Meaning |
|---|---|---|
| `QUENCH_KEYSTORE_PASSWORD` | `android keystore`, `build apk --release`, `build aab`, Gradle | Keystore password. Required for key creation (at least 6 characters) and for release builds. |
| `QUENCH_KEY_PASSWORD` | `android keystore`, Gradle | The key's own password; Gradle falls back to the keystore password when it is unset. |
| `QUENCH_KEYSTORE`, `QUENCH_KEY_ALIAS` | Gradle (set by the tool) | The keystore path (absolute) and alias passed to `android/app/build.gradle.kts`. Set only for release/aab builds. |
| `ANDROID_NDK_HOME`, `ANDROID_NDK_ROOT` | `build-native`, `doctor` | NDK location. |
| `ANDROID_HOME`, `ANDROID_SDK_ROOT` | `build-native` (NDK under `ndk/`), `apksigner` lookup (newest in `build-tools/`), `doctor` | SDK location. |
| `CARGO_TARGET_DIR` | project target directory | If set, the target directory (relative values are from the project root). Otherwise `<workspace root>/target`. For a standalone `desktop/` crate: `desktop/target`, or `desktop/<CARGO_TARGET_DIR>`. |
| `SOURCE_DATE_EPOCH` | `build rpm`, `arch`, `source` | Timestamp for reproducible packages (default: now). Not used by `deb`. |
| `HOME` | `doctor` | For `~/Android/Sdk`. |
| `PATH` | everywhere | Tool lookup (`wasm-opt`, `xdg-open`, `chromium`, ...). |

Set by the tool for the programs it starts:

| Variable | For | Value |
|---|---|---|
| `PORT` | dev/test server | the port |
| `QUENCH_DEV` | dev server | `1` (the server enables page reload) |
| `WASM` | dev/test/bundle server | path of the module |
| `QUENCH_EXPORT` | bundle server | `dist/www` |
| `QUENCH_DEV_URL` | `dev --linux` host | `http://127.0.0.1:<port>` |
| `CHROMIUM` | `tests/browser.mjs` | the Chromium found |
| `CARGO_TARGET_<TRIPLE>_LINKER`, `CC_<triple>`, `AR_<triple>` | `build-native` cargo | NDK clang/llvm-ar paths |

## Files the tool writes

| Path | By |
|---|---|
| `.quench.toml`, `Cargo.toml`, `.gitignore`, `README.md`, `.cargo/config.toml` (registry only), `core/`, `app/`, `server/`, `shared/`, `tests/browser.mjs` | `new` |
| `android/**`, `[android]` table | `android init` |
| `android/.quench-plugins` (one plugin name per line) | `plugin add` |
| `android/app/src/main/jniLibs/<abi>/libquench_{native,surface}.so` | `build-native` |
| `desktop/**`, `[linux]` table, `/desktop/target` in `.gitignore` | `linux init` |
| `dist/app.wasm`, `dist/<name>-server`, `dist/www/`, `dist/<name>-linux`, `dist/*.apk|aab|deb|rpm|pkg.tar.zst|tar.gz` | `build` |
| `release.keystore` (default) and `keystore`/`key_alias` lines | `android keystore` |

## Pinned versions

`src/versions.rs` lists the quench crates a new project depends on (the oldest version each template needs): `quench-android 0.1.0`, `quench-android-surface 0.1.1`, `quench-core 0.1.6`, `quench-desktop 0.1.0`, `quench-fullstack 0.1.3`, `quench-http 0.1.14`, `quench-serde 0.1.6`, `quench-style 0.1.2`, `quench-test 0.1.2`, `quench-ui 0.1.1`, `quench-ui-native 0.1.1`, `quench-ui-web 0.1.3`, from the registry `ennor` (index `sparse+https://ennor.ddns.net/warehouse/index/`) or `{ path = "<quench-path>/<crate>" }`. The generated workspace also pins `tokio = 1.54`.
