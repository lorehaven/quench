---
title: Android plugins
parent: quench-tool
grand_parent: Crates
nav_order: 4
---

`quench plugin add <name>` copies a plugin's Kotlin (and sometimes Rust) files into the project, edits the manifest and `Plugins.kt`, and adds the crates it needs. This page documents each of the eleven plugins from `src/plugins.rs` and the templates in `quench-tool/templates/android/plugins/`. The typed Rust clients are in `quench_ui_web::plugins`; from a native-renderer program use `quench_android_surface::Bridge`.

## The bridge contract (host side)

* `Plugin` (`Plugin.kt`): `abstract class Plugin(val name: String)` with `emit(event: String, data: JSONObject = JSONObject())`, `open fun disconnected()`, `open fun onActivityResult(requestCode, resultCode, data)`, `open fun onIntent(intent)` and `abstract fun call(method: String, args: JSONObject, call: Call)`.
* `Call` (in `Bridge.kt`): `resolve(value: Any? = null)`, `reject(code: String, message: String)`, `permissions(vararg names: String, then: (Boolean) -> Unit)`. Answers once; later calls are ignored.
* `Bridge`: a request is the JSON `{"id","plugin","method","args"}`; answers `{"id","ok"}` or `{"id","err":{"code","message"}}`; events `{"event","plugin","data"}`. Messages over `1 shl 20` bytes are dropped; a message without `id` is dropped. Unknown plugin: `unknown_plugin` (message ``no plugin `<name>` ``). A `JSONException` (a missing or mistyped argument): `invalid_args`. Any other exception: `internal`. Every plugin answers an unknown method with `unknown_method` (``<plugin> has no method `<m>` ``).
* The page gets a `MessagePort` only if its origin is the bundle's (or, in a debug build, the dev server's); a native-renderer `SurfaceActivity` creates `Bridge(this, null, plugins(this), sink) { false }`.
* Registration: `fun plugins(activity: Activity): List<Plugin>` in `Plugins.kt`; a new project has `EchoPlugin()` and the marker `// quench:plugins`.
* Manifest markers: `<!-- quench:permissions -->` (permissions, and the top-level `<queries>` snippet), `<!-- quench:components -->` (inside `<application>`), `<!-- quench:activity -->` (inside `.MainActivity`). Missing marker: ``the marker `<marker>` is gone from the file; add it back where plugins go``.

### `EchoPlugin` (always present; name `echo`)

`say {text}` returns the text; `device` returns `{sdk, model}`; `add {a, b}` returns the integer sum; `fail {code}` rejects with `code` (default `failed`) and message `asked to fail`; `notifications` asks for `POST_NOTIFICATIONS` and resolves `true` or rejects `permission_denied` (`notifications are not allowed`). Meant to be removed from `plugins()`. The debug manifest (`app/src/debug/AndroidManifest.xml`) declares `POST_NOTIFICATIONS` for it.

## Summary

| CLI name | Bridge plugin name(s) | Needs | Permissions added | Manifest elements | Registered |
|---|---|---|---|---|---|
| `native` | (none; `dev.quench.Native` object) | | | | |
| `surface` | (none; `SurfaceActivity`) | | | `<activity .SurfaceActivity>` | |
| `jobs` | `jobs` | `native` | `RECEIVE_BOOT_COMPLETED` | `<service .QuenchJobService>` | `JobsPlugin(activity)` |
| `device` | `device` | | `ACCESS_NETWORK_STATE` | | `DevicePlugin(activity)` |
| `secure-storage` | `secure_storage` | | | | `SecureStoragePlugin(activity)` |
| `notifications` | `notifications` | `native` | `POST_NOTIFICATIONS`, `RECEIVE_BOOT_COMPLETED`, `USE_EXACT_ALARM`, `SCHEDULE_EXACT_ALARM` (`maxSdkVersion="32"`) | `ReminderReceiver`, `BootReceiver` | `NotificationsPlugin(activity)` |
| `files` | `files` | | `READ_MEDIA_IMAGES`, `READ_MEDIA_VIDEO`, `READ_MEDIA_AUDIO`, `READ_EXTERNAL_STORAGE` (`maxSdkVersion="32"`) | | `FilesPlugin(activity)` |
| `packages` | `packages` | | `REQUEST_INSTALL_PACKAGES`, `QUERY_ALL_PACKAGES` | `<receiver .InstallReceiver>` | `PackagesPlugin(activity)` |
| `media` | `tts`, `audio` | | | `<queries>` for `TTS_SERVICE` | `TtsPlugin(activity)`, `AudioPlugin(activity)` |
| `share` | `share`, `intents` | | | `SEND text/plain` intent filter on `.MainActivity` | `SharePlugin(activity)`, `IntentsPlugin()` |
| `widget` | `widget` | | | `<receiver .QuenchWidgetProvider>` | `WidgetPlugin(activity)` |

Note that the CLI name `secure-storage` has a hyphen but the bridge name is `secure_storage` (the desktop host uses the same bridge name).

Files are written to `android/app/src/main/kotlin/<package path>/` unless stated. The base manifest already has `INTERNET`. A permission is only inserted when `"<permission>"` is not in the manifest yet.

## `native`

* Files: `android/app/src/main/kotlin/dev/quench/Native.kt` (package `dev.quench`), `native/Cargo.toml`, `native/src/main.rs`. Workspace: member `native`; dependencies `quench-android`, `quench-core`, `quench-serde`.
* Needs the NDK; run `quench android build-native`.
* `dev.quench.Native`: `fun available(context: Context): Boolean`; `fun invoke(context: Context, op: String, input: ByteArray = ByteArray(0), timeoutSeconds: Long = 60): ByteArray`; `class NativeException(message: String) : Exception`. It runs `libquench_native.so` from `applicationInfo.nativeLibraryDir` as a process, writes two frames and reads one, destroys it after `timeoutSeconds` (a hang ends the read), and returns the result bytes; failure raises `NativeException` with the program's message, `the native program is not in this build`, `cannot start the native program: <e>`, or `the native program ended without an answer: <stderr, at most 4096 bytes, or exit <code>>`. Protocol: see [quench-android internals](../quench-android/internals.md).
* `native/src/main.rs` registers one sample operation, `job.heartbeat` (input JSON `{"dir": "<files dir>"}`, appends a line `<rfc3339> heartbeat <n>` to `heartbeat.log` there, answers `{"lines": n, "at": "<rfc3339>"}`), and runs `quench_android::run(register_ops)`.

## `surface`

* Files: `SurfaceHost.kt`, `QuenchSurfaceView.kt`, `SurfaceActivity.kt`, `surface/Cargo.toml`, `surface/src/main.rs`. Workspace: member `surface`; dependencies `quench-android-surface`, `quench-style`, `quench-ui`, `quench-ui-native`.
* Manifest: `<activity android:name=".SurfaceActivity" android:exported="true" android:windowSoftInputMode="adjustResize" android:configChanges="orientation|screenSize|smallestScreenSize|screenLayout|keyboard|keyboardHidden|uiMode|density|fontScale|layoutDirection|locale" />`. Start it with `adb shell am start -n <package>/.SurfaceActivity`, or make it the launcher: `quench android launcher surface`.
* `SurfaceHost(context, listener)`: runs `libquench_surface.so` with the app's files directory as working directory and `HOME`, `TZ` = the device's time zone id, and (debug builds) `QUENCH_FRAME_STATS=1`; its standard error goes to logcat (tag `quench-surface`, level warning). Methods: `available()`, `start()`, `stop()`, `resize`, `touch`, `key`, `text`, `compose`, `delete`, `access`, `scheme`, `back`, `visible`, `clipboard`, `reply`. Listener: `onFrame`, `onKeyboard`, `onIme`, `onTree`, `onBackHandled`, `onClipboard`, `onCall`, `onExit`, `onClosed`. Messages are those of [quench-android-surface](../quench-android-surface/internals.md#protocol).
* `QuenchSurfaceView : SurfaceView`: draws frames (`lockCanvas`), forwards touches and hardware keys, provides an `InputConnection` for the soft keyboard, an `AccessibilityNodeProvider` built from the program's tree, and syncs the clipboard.
* `SurfaceActivity : Activity`: hosts the view; Back goes to the program (`surface.back()`); forwards activity results, permission results and new intents to the bridge; reports dark mode and visibility.
* `surface/src/main.rs` is a demo app (counter, text field, checkbox, "Ask the host" button calling `echo.device`).

## `jobs`

Requires `native`. Files: `JobsPlugin.kt`, `QuenchJobService.kt`. Permission `RECEIVE_BOOT_COMPLETED`; `<service android:name=".QuenchJobService" android:permission="android.permission.BIND_JOB_SERVICE" android:exported="false" />`.

Bridge plugin `jobs`. A task name `<task>` runs the native operation `job.<task>` with the input JSON `{"dir": "<filesDir>", "task": "<task>"}`; the result string is stored. If the stored result starts with `error:` the job is rescheduled. Job ids: `hash(task) & 0x3fffffff`; one-off ids add `0x40000000`.

| Method | Arguments | Result | Notes |
|---|---|---|---|
| `schedule` | `task` (string, required), `interval_minutes` (default 15, minimum 15), `network`, `charging` (bool, default false), `battery_not_low` (bool, default false) | `{"scheduled": bool}` | Periodic, persisted across reboots. Replaces an earlier job of the same task. |
| `run_now` | `task`, plus the same constraint arguments | `{"scheduled": bool}` | One-off, override deadline 0. |
| `cancel` | `task` | null | Cancels both ids. |
| `pending` | | array of `{"task", "periodic", "interval_minutes"}` | |
| `last_run` | `task` | `{"runs": int, "at": ms, "result": string}` | From preferences `quench-jobs`. |

`network` values: `"none"` (no requirement), `"connected"` (any network), `"unmetered"`; any other value, including the default `"any"` used when the argument is absent, maps to no requirement (`NETWORK_TYPE_NONE`).

## `device`

File `DevicePlugin.kt`. Permission `ACCESS_NETWORK_STATE`. Bridge plugin `device`.

| Method | Result |
|---|---|
| `state` | `{"online": bool, "transport": "none"/"wifi"/"cellular"/"ethernet"/"other", "metered": bool, "battery_percent": int (-1 unknown), "charging": bool, "battery_low": bool (0..=15 and not charging), "power_save": bool}`. `online` means the active network has the `VALIDATED` capability. |
| `watch` | null; then the event `state` (same object) on every change (default network callbacks and battery / power-save broadcasts; only a different state is pushed). |
| `unwatch` | null; also done when the page disconnects. |

## `secure-storage`

File `SecureStoragePlugin.kt`: classes `SecureStore(context)` (usable anywhere in the app, also from a job) and `SecureStoragePlugin`. Bridge name **`secure_storage`**. AES-256-GCM with a key (`quench-secure-storage` alias) in the Android Keystore; ciphertext (12-byte IV + sealed bytes, Base64) in the preferences `quench-secure`, entries `v.<key>`; the key name is used as additional authenticated data, so a value cannot be moved to another name.

| Method | Arguments | Result |
|---|---|---|
| `set` | `key`, `value` (strings) | null |
| `get` | `key` | the string, or null when absent |
| `remove` | `key` | null |
| `keys` | | sorted array of key names |
| `clear` | | null; clears **all** preferences of the file `quench-secure` |

Error `corrupt` (``the value cannot be decrypted: <message>``) on a `GeneralSecurityException`.

## `notifications`

Requires `native`. File `NotificationsPlugin.kt` (also `Reminders`, `ReminderReceiver`, `BootReceiver`). Permissions as in the summary. Manifest: `<receiver android:name=".ReminderReceiver" android:exported="false" />` and `<receiver android:name=".BootReceiver" android:exported="true">` with actions `BOOT_COMPLETED`, `TIMEZONE_CHANGED`, `TIME_SET`, `MY_PACKAGE_REPLACED`. Bridge plugin `notifications`.

| Method | Arguments | Result |
|---|---|---|
| `channel` | `id`, `name` (required), `importance` (default 3), `description` | null |
| `show` | `id` (int), `title`, `body`, `channel` (default `reminders`, created on demand as "Reminders") | `{"posted": bool}` (false if notifications are disabled) |
| `schedule` | `id` (int), `at_ms`, `title`, `body`, `channel`, `repeat_zone`, `repeat_time` | `{"exact": bool, "at_ms": ms}` |
| `cancel` | `id` | null |
| `pending` | | array of the stored reminder objects, as sent |
| `permission` | | `{"granted": bool}` after asking for `POST_NOTIFICATIONS` |

A reminder repeats when both `repeat_zone` (an IANA name) and `repeat_time` (`HH:MM`) are non-null. For a repeating reminder without `at_ms` (or 0) the first time is worked out by the native operation `tz.next`; without `at_ms` and without repetition: `invalid_args` (``at_ms is needed unless the reminder repeats``). A repeating reminder without the native program: `native_unavailable`. Reminders are stored in the preferences `quench-reminders`, armed with `AlarmManager.setExactAndAllowWhileIdle` when allowed (API < 31 or `canScheduleExactAlarms()`), otherwise `setAndAllowWhileIdle` (`exact` false). When due: posted (small icon `ic_dialog_info`; tapping opens the launch activity), then dropped, or re-armed for the next occurrence. After boot, a time or zone change, or an app update, all are re-armed; one missed while off is fired at once.

## `files`

File `FilesPlugin.kt`. Permissions as in the summary. Bridge plugin `files`. Constants: chunk 256 KiB, request codes `0x5101` (folder), `0x5102` (files).

| Method | Arguments | Result |
|---|---|---|
| `media` | `kind` (`images` default, `video`, `audio`), `limit` (default 50, clamped 1..500), `offset` (default 0) | array of `{"uri","name","size","mime","modified_ms"}`, newest first (MediaStore). Asks for `READ_MEDIA_*` (API 33+) or `READ_EXTERNAL_STORAGE`; rejects `permission_denied` (`the media library is not allowed`); other kind: `invalid_args`. |
| `pick_folder` | | `{"uris": [..]}` (system tree picker; read grants are persisted) |
| `pick_files` | `multiple` (default true) | `{"uris": [..]}` |
| `list_folder` | `uri`, `limit` (default 500) | array of `{"uri","name","mime","is_dir","size" (-1 unknown),"modified_ms"}` |
| `stat` | `uri` | `{"name","size" (-1 unknown),"mime"}` |
| `read` | `uri`, `offset` (default 0), `length` (default and maximum 262144, minimum 1) | `{"data": base64, "size", "eof"}` |
| `upload` | `id`, `uri`, `url`, `method` (default `PUT`), `headers` (object) | `{"id","total"}` at once; then events `progress {id, sent, total}` (at most every 200 ms, and at the end), `done {id, status, body}` (first 4096 bytes of the response) or `failed {id, message}`. Streams from native code; fixed-length when the size is known, chunked (64 KiB) otherwise. |
| `cancel_upload` | `id` | null |

Pickers reject `cancelled` (`nothing was picked`) when dismissed. `SecurityException` becomes `permission_denied`; `FileNotFoundException` becomes `not_found`.

## `packages`

File `PackagesPlugin.kt` (also `Packages` and `InstallReceiver`). Permissions `REQUEST_INSTALL_PACKAGES`, `QUERY_ALL_PACKAGES`; `<receiver android:name=".InstallReceiver" android:exported="false">` with the action `<package>.INSTALL_RESULT`. Bridge plugin `packages`. Google Play restricts both permissions to apps whose core purpose is installing or updating apps; an app using this plugin cannot be distributed through Play.

| Method | Arguments | Result |
|---|---|---|
| `can_install` | | `{"allowed": bool}` |
| `open_install_settings` | | null (opens "install unknown apps" for the app) |
| `installed` | `package` | `{"installed": true, "version_name", "version_code"}` or `{"installed": false}` |
| `list` | | `{"apps": [{"package","label","version_name","version_code","system"}]}`: apps with a launch intent; `system` is true for system apps that were not updated |
| `launch` | `package` | `{"launched": bool}` |
| `install_from_url` | `url`, `headers` | null at once (rejects `permission_denied`, ``this app may not install apps: open_install_settings`` if not allowed); events `download {sent, total}` then an `install` event |
| `install` | `uri` | null at once; the `install` event follows |

Event `install {status, package?, message}`: `status` is `success`, `failure`, `cancelled` or `pending` (the system's confirmation is being shown). The system always asks the person to confirm; nothing installs silently. An update must be signed with the same key and restarts the app.

## `media`

Files `TtsPlugin.kt`, `AudioPlugin.kt`; a `<queries>` entry for `android.intent.action.TTS_SERVICE`. Two bridge plugins.

`tts`:

| Method | Arguments | Result |
|---|---|---|
| `speak` | `text` (required), `lang` (default: device locale tag), `rate` (1.0), `pitch` (1.0) | `{"id": "u<n>"}` (queued); rejects `failed` (``the engine refused the text``) |
| `stop` | | null |
| `voices` | | sorted array of language tags |

Events `start {id}`, `done {id}`, `error {id}`.

`audio` (one track at a time; stops when the page disconnects):

| Method | Arguments | Result |
|---|---|---|
| `play` | `uri` (content, file or http(s)) | `{"duration_ms"}` once prepared; rejects `failed` |
| `pause`, `resume`, `stop` | | null |
| `seek` | `ms` (int) | null |
| `state` | | `{"playing","position_ms","duration_ms"}` |

Events `ended`, `error {message}` (`media error <what>/<extra>`).

## `share`

Files `SharePlugin.kt`, `IntentsPlugin.kt`. The main activity gets an intent filter for `android.intent.action.SEND` with category `DEFAULT` and `text/plain`. Two bridge plugins.

`share`:

| Method | Arguments | Result |
|---|---|---|
| `text` | `text` (required), `title` | null (chooser) |
| `file` | `uri` (content URI), `mime` (default `*/*`), `title` | null |
| `open` | `uri`, `mime` (default `*/*`) | null; rejects `not_found` (`no app can open this`) |

`intents`: `take` returns the array of intents received since the last `take` and clears it; each is `{"action","type","uri","text"}`; the event `intent` carries the same object while a page is connected.

## `widget`

Files `WidgetPlugin.kt`, `android/app/src/main/res/layout/quench_widget.xml`, `android/app/src/main/res/xml/quench_widget_info.xml`. Manifest: `<receiver android:name=".QuenchWidgetProvider" android:exported="true">` with `APPWIDGET_UPDATE` and the `@xml/quench_widget_info` provider metadata. Bridge plugin `widget`; the widget shows a title and a body line and opens the launch activity when tapped.

| Method | Arguments | Result |
|---|---|---|
| `set` | `title`, `body` | null (stored in preferences `quench-widget`; every placed widget is updated) |
| `pin` | | `{"requested": bool}` |
| `count` | | `{"count": n}` |

## Desktop counterparts

`quench-desktop` offers `echo`, `device`, `secure_storage`, `notifications`, `share` and `files` with the same method names; what a desktop cannot do answers `unsupported`. See [quench-desktop plugins](../quench-desktop/plugins.md).
