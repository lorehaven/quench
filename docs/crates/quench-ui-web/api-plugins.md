---
title: API plugins
parent: quench-ui-web
grand_parent: Crates
nav_order: 6
---

`quench_ui_web::plugins` (`src/plugins.rs`): typed clients for the plugins that `quench plugin add` puts in the Android host. Each is declared with [`#[plugin]`](../quench-ui-web-macros/index.md), so `Jobs::run_now("heartbeat".into()).await` sends the call `{"plugin":"jobs","method":"run_now","args":{"task":"heartbeat"}}`. What each method does on the device is documented with the Android host (`quench-android` in this book); this page lists the Rust side exactly.

Every plugin is a unit struct `#[derive(Clone, Copy, Debug)] pub struct X;` with `pub const NAME: &'static str` and one `pub async fn` per method returning `Result<T, BridgeError>` ([`BridgeError`](api-bridge.md#bridgeerror)). Argument names are the JSON keys of `args`; the Kotlin class must use the same names. Without a host every call fails with `unavailable`. Events a plugin pushes arrive through [`bridge::on_event`](api-bridge.md#on_event) as `(plugin, event, data)`.

All data structs below derive `Clone, Debug, PartialEq, Eq, Deserialize, Serialize` (the `quench_serde` traits) and have public fields, so a reply is read by field name. Unknown extra fields in a reply are not part of the type; a missing field makes the call fail with `bad_response`.

## `Jobs` (`"jobs"`)

Background work. `network` is `none`, `connected` or `unmetered`; a periodic job runs at most every 15 minutes.

| Method | Arguments | Returns |
|---|---|---|
| `schedule` | `task: String, interval_minutes: u32, network: String, charging: bool, battery_not_low: bool` | `Scheduled` |
| `run_now` | `task: String` | `Scheduled` |
| `cancel` | `task: String` | `()` |
| `pending` | none | `Vec<PendingJob>` |
| `last_run` | `task: String` | `LastRun` |

```rust
pub struct Scheduled { pub scheduled: bool }                   // whether the scheduler took the job
pub struct PendingJob { pub task: String, pub periodic: bool, pub interval_minutes: u64 }
pub struct LastRun { pub runs: u32, pub at: i64, pub result: String }   // runs is 0 before the first run
```

## `Device` (`"device"`)

The network, the battery and power saving. `watch` makes the host push a `device`/`state` event on every change until `unwatch` (or the page goes).

| Method | Arguments | Returns |
|---|---|---|
| `state` | none | `DeviceState` |
| `watch` | none | `()` |
| `unwatch` | none | `()` |

```rust
#[allow(clippy::struct_excessive_bools)]
pub struct DeviceState {
    pub online: bool,
    pub transport: String,        // wifi, cellular, ethernet, other or none
    pub metered: bool,
    pub battery_percent: i32,     // 0-100, or -1 when unknown
    pub charging: bool,
    pub battery_low: bool,
    pub power_save: bool,
}
```

## `SecureStorage` (`"secure_storage"`)

Small secrets, encrypted with a key that stays in the Android Keystore. A value that cannot be decrypted is the error `corrupt`.

| Method | Arguments | Returns |
|---|---|---|
| `set` | `key: String, value: String` | `()` |
| `get` | `key: String` | `Option<String>` |
| `remove` | `key: String` | `()` |
| `keys` | none | `Vec<String>` |
| `clear` | none | `()` |

## `Notifications` (`"notifications"`)

Notifications and reminders that survive a reboot. A repeating reminder is a clock time in a named zone (`repeat_zone: "Europe/Warsaw"`, `repeat_time: "08:00"`) that daylight saving does not move; give `at_ms` 0 to start from the next one. Needs the `native` plugin.

| Method | Arguments | Returns |
|---|---|---|
| `channel` | `id: String, name: String, importance: i32, description: String` | `()` |
| `show` | `id: i32, title: String, body: String, channel: String` | `Posted` |
| `schedule` | `id: i32, at_ms: i64, title: String, body: String, channel: String, repeat_zone: Option<String>, repeat_time: Option<String>` | `Armed` |
| `cancel` | `id: i32` | `()` |
| `pending` | none | `Vec<Reminder>` |
| `permission` | none | `Permission` |

```rust
pub struct Posted { pub posted: bool }          // false when the app may not post notifications
pub struct Reminder {
    pub id: i32, pub at_ms: i64, pub title: String, pub body: String,
    pub channel: Option<String>, pub repeat_zone: Option<String>, pub repeat_time: Option<String>,
}
pub struct Armed { pub exact: bool, pub at_ms: i64 }   // whether the system fires it exactly on time
pub struct Permission { pub granted: bool }
```

`Option` arguments are sent as JSON `null` when `None`.

## `Files` (`"files"`)

The media library, folders and files the person picks, and moving their bytes without holding them in the page. `kind` is `images`, `video` or `audio`. `headers` is a JSON object (`quench_serde::Value`). Upload progress comes as `files` events `progress`, `done`, `failed`.

| Method | Arguments | Returns |
|---|---|---|
| `media` | `kind: String, limit: u32, offset: u32` | `Vec<MediaItem>` |
| `pick_folder` | none | `Picked` |
| `pick_files` | `multiple: bool` | `Picked` |
| `list_folder` | `uri: String, limit: u32` | `Vec<Entry>` |
| `stat` | `uri: String` | `FileStat` |
| `read` | `uri: String, offset: i64, length: u32` | `Chunk` |
| `upload` | `id: String, uri: String, url: String, method: String, headers: Value` | `Upload` |
| `cancel_upload` | `id: String` | `()` |

```rust
pub struct MediaItem { pub uri: String, pub name: String, pub size: i64, pub mime: String, pub modified_ms: i64 }
pub struct Picked { pub uris: Vec<String> }     // the grant is kept across restarts
pub struct Entry { pub uri: String, pub name: String, pub mime: String, pub is_dir: bool,
                   pub size: i64 /* -1 when the provider does not say */, pub modified_ms: i64 }
pub struct FileStat { pub name: String, pub size: i64, pub mime: Option<String> }
pub struct Chunk { pub data: String /* base64, at most 256 KiB */, pub size: i64, pub eof: bool }
pub struct Upload { pub id: String, pub total: i64 }
```

## `Packages` (`"packages"`)

Installing and updating apps from a downloaded APK. The outcome is the `packages` event `install` with `status` `success`, `failure`, `cancelled` or `pending`. Outside Google Play only.

| Method | Arguments | Returns |
|---|---|---|
| `can_install` | none | `Allowed` |
| `open_install_settings` | none | `()` |
| `installed` | `package: String` | `Installed` |
| `install_from_url` | `url: String` | `()` |
| `install` | `uri: String` | `()` |

```rust
pub struct Allowed { pub allowed: bool }
pub struct Installed { pub installed: bool, pub version_name: Option<String>, pub version_code: Option<i64> }
```

## `Tts` (`"tts"`)

Text to speech with the device's voices. Events `start`, `done`, `error` carry the utterance id.

| Method | Arguments | Returns |
|---|---|---|
| `speak` | `text: String, lang: String, rate: f64, pitch: f64` | `Spoken` |
| `stop` | none | `()` |
| `voices` | none | `Vec<String>` (the language tags the engine has) |

```rust
pub struct Spoken { pub id: String }
```

## `Audio` (`"audio"`)

Audio playback, one track at a time; events `ended` and `error`.

| Method | Arguments | Returns |
|---|---|---|
| `play` | `uri: String` | `Playing` |
| `pause`, `resume`, `stop` | none | `()` |
| `seek` | `ms: i32` | `()` |
| `state` | none | `AudioState` |

```rust
pub struct Playing { pub duration_ms: i64 }
pub struct AudioState { pub playing: bool, pub position_ms: i64, pub duration_ms: i64 }
```

## `Share` (`"share"`)

Sharing to other apps and opening things in them.

| Method | Arguments | Returns |
|---|---|---|
| `text` | `text: String, title: String` | `()` |
| `file` | `uri: String, mime: String, title: String` | `()` |
| `open` | `uri: String, mime: String` | `()` |

## `Intents` (`"intents"`)

What other apps send: `take` returns what arrived since the last call (an app calls it at start); each one is also pushed as an `intents`/`intent` event while a page is connected.

| Method | Arguments | Returns |
|---|---|---|
| `take` | none | `Vec<Incoming>` |

```rust
pub struct Incoming { pub action: String, pub r#type: String, pub uri: String, pub text: String }
```

`Incoming.r#type` is the raw identifier `type`; serde (`quench_serde` derive) is expected to use the name `type` on the wire. *Unverified:* that derive's handling of raw identifiers was not checked here.

## `Widget` (`"widget"`)

A home-screen widget showing a title and a line of text.

| Method | Arguments | Returns |
|---|---|---|
| `set` | `title: String, body: String` | `()` |
| `pin` | none | `Pinned` |
| `count` | none | `Placed` |

```rust
pub struct Pinned { pub requested: bool }   // whether the launcher was asked to place a widget
pub struct Placed { pub count: u32 }        // how many widgets are placed
```

## Summary of public items

11 plugin structs (`Jobs`, `Device`, `SecureStorage`, `Notifications`, `Files`, `Packages`, `Tts`, `Audio`, `Share`, `Intents`, `Widget`), 48 methods in total, and 22 data structs (`Scheduled`, `PendingJob`, `LastRun`, `DeviceState`, `Posted`, `Reminder`, `Armed`, `Permission`, `MediaItem`, `Picked`, `Entry`, `FileStat`, `Chunk`, `Upload`, `Allowed`, `Installed`, `Spoken`, `Playing`, `AudioState`, `Incoming`, `Pinned`, `Placed`).
