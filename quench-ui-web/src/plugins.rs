//! Typed clients for the plugins `quench plugin add` puts in the Android host.
//!
//! Each is declared with [`plugin`](crate::plugin), so `Jobs::run_now("heartbeat".into()).await` is a
//! call to the Kotlin class of the same name; what each method does is in `docs/quench-android.md`.
//!
//! Events a plugin pushes (`files` upload progress, `device` state changes, `packages` installs,
//! `tts`/`audio` playback, `intents`) arrive through [`bridge::on_event`](crate::bridge::on_event) as
//! `(plugin, event, data)`.

use quench_serde::{Deserialize, Serialize, Value};

use crate::plugin;

/// What `jobs.schedule` and `jobs.run_now` answer: whether the scheduler took the job.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Scheduled {
    pub scheduled: bool,
}

/// A job waiting in the scheduler.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PendingJob {
    pub task: String,
    pub periodic: bool,
    pub interval_minutes: u64,
}

/// When a task last ran and what it answered. `runs` is 0 before the first run.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct LastRun {
    pub runs: u32,
    pub at: i64,
    pub result: String,
}

/// Background work. `network` is `none`, `connected` or `unmetered`; a periodic job runs at most
/// every 15 minutes.
#[plugin("jobs")]
pub trait Jobs {
    async fn schedule(
        task: String,
        interval_minutes: u32,
        network: String,
        charging: bool,
        battery_not_low: bool,
    ) -> Result<Scheduled, BridgeError>;
    async fn run_now(task: String) -> Result<Scheduled, BridgeError>;
    async fn cancel(task: String) -> Result<(), BridgeError>;
    async fn pending() -> Result<Vec<PendingJob>, BridgeError>;
    async fn last_run(task: String) -> Result<LastRun, BridgeError>;
}

/// What the device is doing.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[allow(clippy::struct_excessive_bools)] // the host reports these facts, each its own flag
pub struct DeviceState {
    pub online: bool,
    /// `wifi`, `cellular`, `ethernet`, `other` or `none`.
    pub transport: String,
    pub metered: bool,
    /// 0-100, or -1 when unknown.
    pub battery_percent: i32,
    pub charging: bool,
    pub battery_low: bool,
    pub power_save: bool,
}

/// The network, the battery and power saving. `watch` makes the host push a `device`/`state` event on
/// every change until `unwatch` (or the page goes).
#[plugin("device")]
pub trait Device {
    async fn state() -> Result<DeviceState, BridgeError>;
    async fn watch() -> Result<(), BridgeError>;
    async fn unwatch() -> Result<(), BridgeError>;
}

/// Small secrets, encrypted with a key that stays in the Android Keystore. A value that cannot be
/// decrypted is the error `corrupt`.
#[plugin("secure_storage")]
pub trait SecureStorage {
    async fn set(key: String, value: String) -> Result<(), BridgeError>;
    async fn get(key: String) -> Result<Option<String>, BridgeError>;
    async fn remove(key: String) -> Result<(), BridgeError>;
    async fn keys() -> Result<Vec<String>, BridgeError>;
    async fn clear() -> Result<(), BridgeError>;
}

/// Whether a notification was posted (false when the app may not post them).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Posted {
    pub posted: bool,
}

/// A reminder as the host keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Reminder {
    pub id: i32,
    pub at_ms: i64,
    pub title: String,
    pub body: String,
    pub channel: Option<String>,
    pub repeat_zone: Option<String>,
    pub repeat_time: Option<String>,
}

/// When a reminder was armed, and whether the system will fire it exactly on time.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Armed {
    pub exact: bool,
    pub at_ms: i64,
}

/// Whether the person has allowed notifications.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Permission {
    pub granted: bool,
}

/// Notifications and reminders that survive a reboot. A repeating reminder is a clock time in a
/// named zone (`repeat_zone: "Europe/Warsaw"`, `repeat_time: "08:00"`) that daylight saving does not
/// move; give `at_ms` 0 to start from the next one. Needs the `native` plugin.
#[plugin("notifications")]
pub trait Notifications {
    async fn channel(
        id: String,
        name: String,
        importance: i32,
        description: String,
    ) -> Result<(), BridgeError>;
    async fn show(
        id: i32,
        title: String,
        body: String,
        channel: String,
    ) -> Result<Posted, BridgeError>;
    async fn schedule(
        id: i32,
        at_ms: i64,
        title: String,
        body: String,
        channel: String,
        repeat_zone: Option<String>,
        repeat_time: Option<String>,
    ) -> Result<Armed, BridgeError>;
    async fn cancel(id: i32) -> Result<(), BridgeError>;
    async fn pending() -> Result<Vec<Reminder>, BridgeError>;
    async fn permission() -> Result<Permission, BridgeError>;
}

/// An item of the media library.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct MediaItem {
    pub uri: String,
    pub name: String,
    pub size: i64,
    pub mime: String,
    pub modified_ms: i64,
}

/// What the picker gave back (the grant is kept across restarts).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Picked {
    pub uris: Vec<String>,
}

/// An entry of a folder the person picked.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Entry {
    pub uri: String,
    pub name: String,
    pub mime: String,
    pub is_dir: bool,
    /// -1 when the provider does not say.
    pub size: i64,
    pub modified_ms: i64,
}

/// What is known of a file.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct FileStat {
    pub name: String,
    pub size: i64,
    pub mime: Option<String>,
}

/// A piece of a file: base64 text, at most 256 KiB.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Chunk {
    pub data: String,
    pub size: i64,
    pub eof: bool,
}

/// An upload that has started; its progress comes as `files` events (`progress`, `done`, `failed`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Upload {
    pub id: String,
    pub total: i64,
}

/// The media library, folders and files the person picks, and moving their bytes without holding
/// them in the page. `kind` is `images`, `video` or `audio`. `headers` is a JSON object.
#[plugin("files")]
pub trait Files {
    async fn media(kind: String, limit: u32, offset: u32) -> Result<Vec<MediaItem>, BridgeError>;
    async fn pick_folder() -> Result<Picked, BridgeError>;
    async fn pick_files(multiple: bool) -> Result<Picked, BridgeError>;
    async fn list_folder(uri: String, limit: u32) -> Result<Vec<Entry>, BridgeError>;
    async fn stat(uri: String) -> Result<FileStat, BridgeError>;
    async fn read(uri: String, offset: i64, length: u32) -> Result<Chunk, BridgeError>;
    async fn upload(
        id: String,
        uri: String,
        url: String,
        method: String,
        headers: Value,
    ) -> Result<Upload, BridgeError>;
    async fn cancel_upload(id: String) -> Result<(), BridgeError>;
}

/// Whether this app may install apps.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Allowed {
    pub allowed: bool,
}

/// An installed package.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Installed {
    pub installed: bool,
    pub version_name: Option<String>,
    pub version_code: Option<i64>,
}

/// Installing and updating apps from a downloaded APK. The outcome is the `packages` event
/// `install` with `status` `success`, `failure`, `cancelled` or `pending`. Outside Google Play only.
#[plugin("packages")]
pub trait Packages {
    async fn can_install() -> Result<Allowed, BridgeError>;
    async fn open_install_settings() -> Result<(), BridgeError>;
    async fn installed(package: String) -> Result<Installed, BridgeError>;
    async fn install_from_url(url: String) -> Result<(), BridgeError>;
    async fn install(uri: String) -> Result<(), BridgeError>;
}

/// An utterance that was queued; `tts` events `start`, `done` and `error` carry its id.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Spoken {
    pub id: String,
}

/// Text to speech with the device's voices.
#[plugin("tts")]
pub trait Tts {
    async fn speak(
        text: String,
        lang: String,
        rate: f64,
        pitch: f64,
    ) -> Result<Spoken, BridgeError>;
    async fn stop() -> Result<(), BridgeError>;
    /// The language tags the engine has.
    async fn voices() -> Result<Vec<String>, BridgeError>;
}

/// A track that started.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Playing {
    pub duration_ms: i64,
}

/// Where playback is.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct AudioState {
    pub playing: bool,
    pub position_ms: i64,
    pub duration_ms: i64,
}

/// Audio playback, one track at a time; events `ended` and `error`.
#[plugin("audio")]
pub trait Audio {
    async fn play(uri: String) -> Result<Playing, BridgeError>;
    async fn pause() -> Result<(), BridgeError>;
    async fn resume() -> Result<(), BridgeError>;
    async fn stop() -> Result<(), BridgeError>;
    async fn seek(ms: i32) -> Result<(), BridgeError>;
    async fn state() -> Result<AudioState, BridgeError>;
}

/// Sharing to other apps and opening things in them.
#[plugin("share")]
pub trait Share {
    async fn text(text: String, title: String) -> Result<(), BridgeError>;
    async fn file(uri: String, mime: String, title: String) -> Result<(), BridgeError>;
    async fn open(uri: String, mime: String) -> Result<(), BridgeError>;
}

/// Something another app sent to this one.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Incoming {
    pub action: String,
    pub r#type: String,
    pub uri: String,
    pub text: String,
}

/// What other apps send: `take` returns what arrived since the last call (an app calls it at start),
/// and each one is also pushed as an `intents`/`intent` event while a page is connected.
#[plugin("intents")]
pub trait Intents {
    async fn take() -> Result<Vec<Incoming>, BridgeError>;
}

/// Whether the launcher was asked to place a widget.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Pinned {
    pub requested: bool,
}

/// How many widgets are placed.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Placed {
    pub count: u32,
}

/// A home-screen widget showing a title and a line of text.
#[plugin("widget")]
pub trait Widget {
    async fn set(title: String, body: String) -> Result<(), BridgeError>;
    async fn pin() -> Result<Pinned, BridgeError>;
    async fn count() -> Result<Placed, BridgeError>;
}
