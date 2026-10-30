---
title: Plugins
parent: quench-desktop
grand_parent: Crates
nav_order: 4
---

The plugins of `quench_desktop::plugins`, under the names and with the methods of the Android ones, so the typed clients in `quench_ui_web::plugins` work on both. What a desktop cannot do answers `unsupported`. Standard error codes: `unknown_method` (``<plugin> has no method `<m>` ``), `invalid_args`, `unavailable` (a helper program is missing), `permission_denied`, `not_found`, `cancelled`, `internal`.

Helper programs are run through a `Runner`, so tests can see exactly what would run (see [API](#api)). `quench doctor` checks that `secret-tool`, `notify-send` and `xdg-open` exist.

## `echo` (`Echo`)

For trying the bridge; leave it out of a real app.

| Method | Arguments | Result |
|---|---|---|
| `say` | `text` | the text (else `invalid_args`: ``` `text` is needed, as text ```) |
| `device` | | `{"sdk": 0, "model": <hostname>}` (`/proc/sys/kernel/hostname`, else `linux`) |
| `add` | `a`, `b` (integers) | `a + b` (else ``invalid_args`: `a` and `b` are needed, as integers``) |
| `fail` | `code` (default `failed`) | rejects with that code and message `asked to fail` |
| `notifications` | | `true` (a desktop has no permission dialog) |

## `device` (`DeviceState`)

Reads `/sys`. `state` returns `{"online","transport","metered","battery_percent","charging","battery_low","power_save"}`:

* `online` / `transport`: the first interface (sorted by name, not `lo`) whose `operstate` is `up` and that has a `device` entry (hardware only). `transport`: `wifi` for names starting `wl`, `cellular` for `ww`, `ethernet` for `en` or `eth`, else `other`; `none` when offline.
* `metered`: always `false`.
* `battery_percent`: `capacity` of the first `BAT*` supply, else `-1`. `charging`: its `status` is `Charging` or `Full`, or, with no battery, an `AC*`/`ADP*` supply is `online` (`1`). `battery_low`: 0..=15 and not charging.
* `power_save`: `/sys/firmware/acpi/platform_profile` is `low-power`.

`watch` starts a thread that re-reads every 5 seconds and emits the `state` event when it changed; `unwatch` (or a new `watch`) stops the earlier thread. Both answer null. Methods: `state`, `watch`, `unwatch`.

## `secure_storage` (`SecureStorage`)

Secrets in the desktop keyring through `secret-tool` (libsecret: GNOME Keyring, KWallet's Secret Service, KeePassXC). Items carry the attributes `app` (the app id) and `key`; the value goes to the program on standard input, never on a command line.

| Method | Arguments | Runs | Result |
|---|---|---|---|
| `set` | `key`, `value` | `secret-tool store --label "<app> <key>" app <app> key <key>` | null; `internal` (`the keyring refused the secret (is it unlocked?)`) on failure |
| `get` | `key` | `secret-tool lookup app <app> key <key>` | the secret text (stdout as is), or null when `lookup` fails |
| `remove` | `key` | `secret-tool clear app <app> key <key>` | null |
| `keys` | | `secret-tool search --all app <app>` | sorted, de-duplicated array of `key` values read from `attribute.key = ` lines of standard error and standard output (never the secrets) |
| `clear` | | `secret-tool clear app <app>` | null |

When `secret-tool` cannot be started: `unavailable` (``the keyring needs `secret-tool` (libsecret-tools) and a running secret service: <e>``).

## `notifications` (`Notifications`)

Desktop notifications through `notify-send --app-name <app> --replace-id <id> <title> <body>`.

| Method | Arguments | Result |
|---|---|---|
| `channel` | | null (channels are an Android notion) |
| `permission` | | `{"granted": true}` |
| `show` | `id` (default 0), `title`, `body` | `{"posted": true}`; `unavailable` (``notify-send` is missing or no notification service is running``) |
| `schedule` | `id`, `at_ms`, `title`, `body`, `channel` | `{"exact": true, "at_ms": at_ms}`; reminders wait in this process and are posted by one thread at the right time; a reminder does not outlive the app; rejects `unsupported` if `repeat_zone` or `repeat_time` is non-null (`repeating reminders need an app that is always running; schedule the next one when this fires`); `invalid_args` without `at_ms`, `title`, `body` |
| `cancel` | `id` | null |
| `pending` | | array of `{"id","at_ms","title","body","channel","repeat_zone":null,"repeat_time":null}` |

## `share` (`Share`)

Opens things in the desktop's apps with `xdg-open` (no share sheet).

| Method | Arguments | Behaviour |
|---|---|---|
| `text` | `text`, `title` | opens `mailto:?subject=<title>&body=<text>` (percent-encoded: everything except `A-Za-z0-9-_.~`) |
| `file`, `open` | `uri` | opens the address |

Only addresses starting `file://`, `https://`, `http://` or `mailto:` are accepted (else `invalid_args`: `only file, http(s) and mailto addresses can be opened`); the address is passed as one argument. `mime` and `title` of `file`/`open` are ignored. Missing `xdg-open`: `unavailable`. Result: null.

## `files` (`Files`)

The user's files, and nothing else. The page can name any path in a `file://` URI, so the plugin only reads what the user handed over **in this run**: a picked file, or a picked folder and what is under it (links that leave it are refused). Anything else answers `permission_denied` (``the user has not picked this file or folder``). Grants do not outlive the process. The check is made on the opened file descriptor (its real path read from `/proc/self/fd`), not on a path looked up again; without `/proc` nothing can be opened. Files are opened non-blocking, so a named pipe in a picked folder cannot hang a call.

| Method | Arguments | Result |
|---|---|---|
| `pick_folder` | | (GTK's `GtkFileChooserNative`) `{"uris": [..]}` or rejects `cancelled` (`nothing was picked`) |
| `pick_files` | `multiple` (default **false**) | as above |
| `list_folder` | `uri`, `limit` (default 500, at least 1) | array of `{"uri","name","mime","is_dir","size","modified_ms"}`, folders first, then by lowercase name; entries that resolve outside the grants are omitted |
| `stat` | `uri` | `{"name","size","mime"}` (`mime` null for a directory) |
| `read` | `uri`, `offset` (default 0), `length` (default and maximum 262144, at least 1) | `{"data": base64, "size", "eof"}`; `invalid_args` for a negative offset or a folder |
| `media`, `upload`, `cancel_upload` | | rejects `unsupported` (``files.<method> is not available on the desktop host``) |

MIME guesses by extension: `txt md log` -> `text/plain`, `html htm` -> `text/html`, `json` -> `application/json`, `pdf`, `png`, `jpg jpeg`, `gif`, `svg` (`image/svg+xml`), `mp3` (`audio/mpeg`), `mp4` (`video/mp4`), `zip`, else `application/octet-stream`; directories `inode/directory`. `size` of a directory is 0.

## API

All in `quench_desktop::plugins`.

* `pub struct Output { pub success: bool, pub stdout: String, pub stderr: String }` (`Debug, Clone, PartialEq, Eq, Default`): what a program printed and how it ended.
* `pub trait Runner: Send + Sync`: `fn run(&self, program: &str, args: &[&str], stdin: Option<&[u8]>) -> io::Result<Output>` (waits; writes `stdin` if given, ignoring a program that exits without reading it all) and `fn spawn(&self, program: &str, args: &[&str]) -> io::Result<()>` (does not wait).
* `pub struct System` (`Debug, Clone, Copy, Default`): the real `Runner` (`std::process::Command`).
* `pub struct Echo` (`Debug, Default, Clone, Copy`).
* `pub struct DeviceState`: `#[must_use] pub fn at(sys: &Path, every: Duration) -> Self` (a `/sys`-shaped tree and the watch interval), `#[must_use] pub fn state(&self) -> Value`; `Default` is `at("/sys", 5 s)`.
* `pub struct SecureStorage`: `#[must_use] pub fn new(app: &str, run: Arc<dyn Runner>) -> Self`.
* `pub struct Notifications`: `#[must_use] pub fn new(app: &str, run: Arc<dyn Runner>) -> Self`.
* `pub struct Share`: `#[must_use] pub fn new(run: Arc<dyn Runner>) -> Self`.
* `pub const CHUNK: usize = 256 * 1024;`
* `pub trait Chooser: Send + Sync`: `fn choose(&self, folder: bool, multiple: bool, done: Box<dyn FnOnce(Option<Vec<PathBuf>>) + Send>)`.
* `pub struct Files`: `#[must_use] pub fn new(chooser: Arc<dyn Chooser>) -> Self`.
* `#[must_use] pub fn uri_of(path: &Path) -> String`: `file://` plus the path, percent-encoding everything except `A-Za-z0-9-_.~/`.
* `#[must_use] pub fn path_of(uri: &str) -> Option<PathBuf>`: the path of a `file://` URI (optional `localhost`, `%XX` decoded); `None` for anything else, invalid escapes, non-UTF-8, a path that is not absolute or contains NUL.
