---
title: Recipes
parent: quench-tool
grand_parent: Crates
nav_order: 5
---

Workflows combining commands. Commands that sign, publish or touch devices are marked; run them only when you mean to.

## A web app from nothing

```bash
quench doctor                      # required: rustc 1.85+, cargo, wasm32 target
quench new demo --template fullstack
cd demo
quench dev --open                  # http://127.0.0.1:3000, reloads on every edit
quench test                        # cargo test --workspace, then cargo check for wasm32
quench build --release             # dist/app.wasm and dist/demo-server (module built in)
./dist/demo-server                 # runs from any directory; PORT=3000 by default
```

## Develop against a checkout of the framework

```bash
quench new demo --quench-path ~/projects/quench
```

Crates come from `{ path = "~/projects/quench/<crate>" }` (canonicalised) and no `.cargo/config.toml` is written.

## Browser test

```bash
quench test --browser              # needs node and a Chromium on PATH
```

## Android: first run on the emulator or a device

```bash
quench android init --package org.example.demo
quench plugin add device           # optional: plugins and their manifest entries
quench run android                 # bundle, debug APK, install, start     (touches the attached device)
quench dev --android               # live reload on the device through adb reverse
```

With several devices attached, choose one with `ANDROID_SERIAL` (the tool runs `adb` without `-s`).

## Android: native code and background work

```bash
quench plugin add jobs             # adds native too
quench android build-native --release
quench build apk                   # includes the native libraries
```

## Android: a signed release

See the [packaging guide](../../guides/packaging.md#android-signing). Short form:

```bash
export QUENCH_KEYSTORE_PASSWORD='...'            # keep out of shell history
quench android keystore                          # once; creates release.keystore   (creates a signing key)
quench build apk --release                       # signs; dist/demo-0.1.0-1.apk
```

`quench android init` writes `version_code = 1` into `.quench.toml`, so the file name is `dist/<name>-<version_name>-<version_code>.apk`. Remove the two version lines to derive both from the Cargo version (`0.1.0` gives code `1000`).

## A native-renderer Android app

```bash
quench android init
quench plugin add surface
quench android launcher surface
quench build apk                   # build-native builds surface/ for both ABIs
```

## Linux desktop

```bash
quench linux init --id org.example.demo
quench build linux                 # dist/demo-linux (debug host; add --release for optimised)
quench run linux                   # opens a window
quench dev --linux                 # window on the dev server
quench build deb                   # dist/demo_0.1.0_amd64.deb
quench build rpm
quench build arch
quench build source
```

*Unverified here:* the book's authors did not build or run the desktop host (it opens windows).

## Upgrade the Kotlin of existing plugins

After upgrading the quench crates:

```bash
quench plugin update               # all added plugins
quench plugin update surface       # one
```

Because the plugin files are copied into the project, your edits to them are overwritten by `update`.

## Reproducible packages

```bash
SOURCE_DATE_EPOCH=1767225600 quench build rpm
```

Applies to `rpm`, `arch` and `source` timestamps; `deb` stamps the `ar` members with 0.
