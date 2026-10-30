---
title: Packaging apps
parent: Guides
nav_order: 13
---

How to turn a Quench project into something you can install: an Android APK or App Bundle, a Debian, RPM or Arch package for the Linux desktop host, or a source tarball. Everything here is done by `quench build <target>` ([commands](../crates/quench-tool/commands.md#quench-build)); the tool never publishes, it only writes files into `dist/`.

## What each target produces

| Command | Output in `dist/` | Needs |
|---|---|---|
| `quench build apk` | `<name>-<versionName>-<versionCode>.apk` (debug-signed) | `android/` (`quench android init`), JDK 17+, Android SDK; the NDK if the project has `native/` or `surface/` |
| `quench build apk --release` | same name, release-signed with your key, verified with `apksigner` | the above plus a keystore and `QUENCH_KEYSTORE_PASSWORD` |
| `quench build aab` | `<name>-<versionName>-<versionCode>.aab` (always release, signed) | as `apk --release` |
| `quench build deb` | `<name>_<version>_amd64.deb` | `desktop/` (`quench linux init`), GTK 3 and webkit2gtk-4.1 dev libraries |
| `quench build rpm` | `<name>-<version>-1.x86_64.rpm` | as `deb` |
| `quench build arch` | `<name>-<version>-1-x86_64.pkg.tar.zst` | as `deb` |
| `quench build source` | `<name>-<version>.tar.gz` | nothing (it builds nothing) |

Check the machine first: `quench doctor`.

## Android

### Debug APK

```bash
quench android init --package org.example.demo   # once
quench build apk
```

Steps the tool runs: `quench build --bundle` (the web bundle into `dist/www`), `quench android build-native` for both ABIs if `native/` or `surface/` exists (`--abi arm64-v8a` or `x86_64` limits it), then `./gradlew -q assembleDebug -Pquench.versionCode=<code> -Pquench.versionName=<name>` in `android/`, and copies `app-debug.apk` to `dist/`. A project with no `server/` crate (a native-renderer app) skips the bundle step.

### Signing

A release is signed with a key in a keystore. The tool reads the location from `.quench.toml` and the **passwords only from the environment**, never from a file or a command line.

```toml
[android]
package = "org.example.demo"
version_name = "0.1.0"
version_code = 1
keystore = "release.keystore"      # from the project root; the generated .gitignore excludes *.keystore and *.jks
key_alias = "release"
```

1. Create the key once (this makes a signing key; do it deliberately):

   ```bash
   export QUENCH_KEYSTORE_PASSWORD='at-least-6-chars'
   export QUENCH_KEY_PASSWORD='optional-separate-key-password'   # leave unset to use the keystore password
   quench android keystore --out release.keystore --alias release
   ```

   It runs `keytool -genkeypair` (RSA 4096, validity 10000 days, `CN=<name>`) with `-storepass:env QUENCH_KEYSTORE_PASSWORD` and, if `QUENCH_KEY_PASSWORD` is set, `-keypass:env QUENCH_KEY_PASSWORD`. It refuses to overwrite an existing file and writes `keystore` and `key_alias` into `[android]`. Keep the keystore and its password safe: a lost key means a new app.

2. Build a signed release:

   ```bash
   export QUENCH_KEYSTORE_PASSWORD='...'
   quench build apk --release          # or: quench build aab
   ```

   The tool checks that `keystore` and `key_alias` are set, that `QUENCH_KEYSTORE_PASSWORD` is set and that the file exists, then runs Gradle with `QUENCH_KEYSTORE=<absolute path>` and `QUENCH_KEY_ALIAS=<alias>` added to the environment. `android/app/build.gradle.kts` builds the signing config from `QUENCH_KEYSTORE`, `QUENCH_KEYSTORE_PASSWORD`, `QUENCH_KEY_ALIAS` and `QUENCH_KEY_PASSWORD` (falling back to the keystore password). After the build, an APK is verified with the SDK's newest `apksigner verify --print-certs` and the certificate line is printed as `signed: <DN>`; a bundle (`aab`) is not verified by the tool.

| Variable | Used for |
|---|---|
| `QUENCH_KEYSTORE_PASSWORD` | `android keystore` (min. 6 characters), release builds |
| `QUENCH_KEY_PASSWORD` | the key's own password, if different |
| `QUENCH_KEYSTORE`, `QUENCH_KEY_ALIAS` | set by the tool for Gradle; do not set by hand |

Errors you may see: ``a release build is signed: set `keystore` and `key_alias` under [android] in .quench.toml`` ; ``set QUENCH_KEYSTORE_PASSWORD (and QUENCH_KEY_PASSWORD if the key has its own) to sign the release``; ``the keystore <path> does not exist: `quench android keystore` makes one``.

### Version name and `versionCode`

`versionName` is what people see; `versionCode` must increase with every release (Android rejects an update whose code is not higher). Both come from `.quench.toml` if set, otherwise from the project's Cargo version, so one number to bump serves every platform.

The project version is read from `[workspace.package] version` in the root `Cargo.toml`, else `[package] version` of `desktop/Cargo.toml`, else of `surface/Cargo.toml`, else `0.1.0`.

The derived code is `major * 1_000_000 + minor * 1_000 + patch`:

| Version | versionCode |
|---|---|
| `0.1.0` | 1000 |
| `0.1.3` | 1003 |
| `0.1.13` | 1013 |
| `1.2.3` | 1002003 |
| `12.0.7` | 12000007 |
| `1.2.3-rc.1` | 1002003 (the text is split at `.`, `-`, `+`; pre-release parts are ignored) |
| `2` | 2000000 |
| `1.5000.0` | 1999000 (minor and patch are capped at 999) |
| `nonsense` | 0 (non-numeric parts count as 0) |

The arithmetic saturates at `u32::MAX`. Because two releases of different versions always compare the same way as their codes (while minor and patch stay under 1000), `0.9.9 < 1.0.0` holds. `quench android init` writes `version_name = "0.1.0"` and `version_code = 1`, which **override** the derived values; delete those two lines to derive them, or bump them by hand. The Gradle properties are `quench.versionCode` and `quench.versionName`.

The output file is named `dist/<project name>-<versionName>-<versionCode>.apk` (or `.aab`).

### Plugins and the Play Store

Some plugins need permissions that Google Play restricts (`packages`: `REQUEST_INSTALL_PACKAGES`, `QUERY_ALL_PACKAGES`). See [Android plugins](../crates/quench-tool/plugins.md#packages).

## Linux packages

```bash
quench linux init --id org.example.demo   # once; writes desktop/ and [linux] id
quench build deb
quench build rpm
quench build arch
quench build source
```

Each of `deb`, `rpm`, `arch` builds the **release** host (as `quench build linux --release`: the bundle, then `cargo build --release`) and packs two files, plus the icon if `[linux] icon` is set:

| Installed path | Contents |
|---|---|
| `/usr/bin/<name>` | the host binary (mode 0755); `<name>` is the project name, not the `-desktop` binary name |
| `/usr/share/applications/<id>.desktop` | the menu entry |
| `/usr/share/icons/hicolor/<w>x<w>/apps/<id>.png` | optional icon |

The `.desktop` entry:

```ini
[Desktop Entry]
Type=Application
Name=<title>
Exec=/usr/bin/<name>
Terminal=false
Categories=Utility;
StartupWMClass=<wm_class or name>
Icon=<id>            # only with [linux] icon
```

Configuration keys (all optional) in `[linux]`: `id`, `title`, `bin`, `depends`, `wm_class`, `icon`; see [configuration](../crates/quench-tool/configuration.md#linux-optional-table-every-key-optional).

| Package | Notes |
|---|---|
| `.deb` | `Package`, `Version`, `Architecture: amd64`, `Maintainer: <name> <noreply@example.com>`, `Installed-Size`, `Depends: libwebkit2gtk-4.1-0, libgtk-3-0` (change with `depends`; `depends = ""` omits it), `Section: utils`, `Priority: optional`, `Description: <title>`. Read by `dpkg-deb`. |
| `.rpm` | name, version, release `1`, `x86_64`, license `unspecified`, SHA-256 file digests, gzip payload. No dependencies, no signature. |
| Arch | zstd tar with `.PKGINFO` (`pkgver = <version>-1`, `arch = x86_64`, `packager = Unknown Packager`). No dependencies, no signature. Install with `pacman -U`. |
| source | `<name>-<version>/…` tarball; skips `target`, `dist`, `.git`, `.idea`, `.gradle`, `build`, `node_modules`, `local.properties`, `.DS_Store` and `*.keystore`, `*.jks`, `*.so`. A project inside a larger workspace brings that workspace's `Cargo.toml` (members cut down to the project's) and `Cargo.lock`. |

`SOURCE_DATE_EPOCH` fixes the timestamps of `rpm`, `arch` and `source` (default: now). All binary packages are 64-bit x86 only.

## Publishing

The tool stops at `dist/`. Sending the files somewhere is outside it (for example `anvil artifact release` in the Forge repository).
