# Forklift (formerly Mathom): the migration checklist

**Where it lives:** `forge/services/forklift` (flat layout, a member of forge's workspace, quench crates from the registry). Package `dev.lorehaven.forklift`, icon `chest-lime`. The Flutter app `mobile-apps/mathom` stays untouched until the replacement reaches parity and you approve retiring it. **There is no server of its own:** Forklift is a client of gatehouse and warehouse, like Hoist.

**Behaviour to match** (read from the Flutter source, not the plan):

- Login is direct username/password against gatehouse; the password is used once; the rotating refresh token is kept in secure storage; the access token is refreshed and shared between the app and the background run (Hoist's `Session::keeping_tokens` / `following` already solve this).
- Uploads: `PUT {server}/warehouse/api/v1/files/{storage}/file?path=...`, whole file, one shot, bearer token, one refresh-and-retry on `401`. `413` is "file too large", `507` is "quota exceeded", `404` is "no such storage".
- Remote path: `{category}/{yyyy}/{mm}/{local_id}-{filename}`, each segment cleaned to `[A-Za-z0-9._-]` (**done**, `forklift-core::remote_path`).
- Manifest: per item `(local_id, size, mtime)`, states `discovered → pending → uploading → done | failed | skipped_*`, bounded concurrency, backoff, a capped retry count; a per-storage `/sync?since=` cursor for reconciliation.
- Sources: Photos, Videos (MediaStore) and custom folders (one category each, `custom:<slug>`, own Wi-Fi-only switch). Large files (> 200 MiB) upload only while charging.
- Triggers: "Back up now" (full rescan), new media, periodic catch-up (~15 min), app resume (throttled). Restore: list by prefix, download into a chosen folder / MediaStore.
- Always-on low-importance notification: "Last checked HH:MM" and how many items wait.

## What exists in the stack already

`files` (MediaStore listing, `pick_folder`, `pick_files`, `list_folder`, `stat`, `read`, streamed `upload`), `jobs`, `device`, `notifications`, `secure-storage`, `surface` + `quench-android-surface`.

## What the framework must grow

- A **manifest store** with no SQLite: an append-only log of item records, compacted on open, in the app's files directory (`forklift-core`).
- **Streaming download to a file** and **save into MediaStore** (restore).
- A **new-media observer** (ContentObserver feeding a job) and a **foreground service** for long uploads.
- A shared **gatehouse client** crate (login, refresh, session-following, bearer requests): **done**, `forge/libs/gatehouse-client` (10 tests); `hoist-net` and `forklift-net` wrap it and keep their own error type and domain calls (their existing tests passed unchanged).

## Checklist

### Phase 1: manual backup

- [x] 1.1 Project: `forge/services/forklift` made, workspace member, `.quench.toml`, icon.
- [x] 1.2 Remote path scheme (`remote_path`, `sanitize`), 3 tests.
- [x] 1.3 Settings and session: `forklift-core::Settings` (text file, folders, conditions), `forklift-net::Store` (session file mode 0600, atomic writes), login/refresh with `keeping_tokens` and `following` (as Hoist).
- [x] 1.4 Warehouse client in `forklift-net` (`storages`, `list` with Link paging, `changes`, `put`, `get_file`, `upload_url`, `fresh_token`, 413/507/404 mapped), 13 tests against a stand-in server. Android streams uploads through the `files` plugin with the token from `fresh_token`.
- [x] 1.5 Manifest (text file, `(size, modified)` change detection, states pending/uploading/done/failed/too_large, backoff 30 s to 1 h, 5 attempts) and `Engine` (concurrency, newest first, large files wait for the charger, Wi-Fi-only categories, quota/no-storage pause, 401 renew-once then sign-out). 35 tests.
- [x] 1.6 Android host: `android/` (plugins `surface`, `files`, `device`), `forklift-surface` with a testable `Controller` over a `Platform` trait (14 tests with a fake device) and the bridge-backed `Android` platform; `chest-lime` launcher icon.
- [x] 1.7 Photos and videos source over `files.media` (pages of 500), picked folders over `files.list_folder` (recursive, ids hashed), "Back up now", dashboard, settings. **Verified on the Pixel_8 emulator against a stand-in warehouse (never the real one): login, scan, 6 photos uploaded, stored bytes identical, session reused after a reinstall.** Not run on a phone.

### Phase 2: automatic backup

- [x] 2.1 Background run with no screen: `quench_surface --backup` (`forklift-surface::headless`), the same `Controller` over a JSON-lines protocol; ends when nothing is left to do or the host says stop. 4 end-to-end tests with a fake host and a stand-in gatehouse.
- [x] 2.2 Kotlin side (`Backup.kt`, `BackupJobService.kt`): a periodic job every 15 min (kept across reboots), a content-trigger job that wakes 10 s after a new photo or video appears (one-shot, re-armed after each run and at app start), a job started when the window goes away. The job does the device's side (media, folders, network/battery, streamed upload) and shows progress as the job's notification; a "Backed up N new files" notification at the end. Both ABIs (arm64-v8a, x86_64) are packaged.
- [x] 2.3 One program at a time: a lock file (`run.lock`) and a `hidden` marker the host writes while the window is stopped; the window's program then cancels what is in flight, writes the manifest and lets go, and takes the backup back (reloading the manifest) when shown again.
- [x] 2.4 Conditions: charging-only for large files and Wi-Fi-only categories apply in the background run too (same engine).
- [x] 2.5 Foreground service (`BackupService`, type `dataSync`): when the window leaves the screen with files still waiting, it takes the backup on with no job time limit and an ongoing notification ("Backing up: 41 of 44 files") with a **Stop** button. It waits (and asks again every 3 s) while another run (the window's program as it leaves, or a job) has the backup, and takes over when that one ends; it ends when nothing is left, when stopped, or at Android 15's data-sync limit (`onTimeout`). The jobs stay for what happens while the window is closed: the 15-minute catch-up and the new-photo trigger (a job cannot start a foreground service). A file that does not finish is still sent whole again (no resume).
- [x] 2.6 The quiet, always-on status notification of mathom (`Backup.updateStatus`, channel "Backup status", minimum importance): "Last checked 13:58 · 29 waiting" or "... everything is backed up". It is posted after every run and when the window leaves the screen, and removed when automatic backup is off or nobody is signed in (after signing out it goes the next time the window leaves the screen, not at once). It is as old as the last look; the live progress is the other notification.
- [x] 2.8 Dashboard card that does not jump: three fixed lines (what is sending, last checked and how much is left, storage use), each too short to wrap, long file names cut to fit with an ellipsis, a line with nothing to say holds its height. The status notification is put back when it is swiped away (`setDeleteIntent` → `BackupReceiver`), after a restart and after an update; the way to be rid of it for good is the channel switch in the system's notification settings.
- [x] 2.7 Choosing the storage: nothing is scanned or sent until the person has chosen where the backup goes. After the sign-in (and once, at start, for a session that has none chosen) the app asks the warehouse which storages the account may use (`GET /files`) and lists them with their use and quota; the device's guess is marked "suggested". Only storages someone made are listed (those with an owner); the ones the server is configured with are hidden. The same list opens from Settings → Storage → Change (choosing another storage sends everything to it again; what is stored stays). The choice is kept (`storage_confirmed=1`); the background run and the status notification do nothing without it. To carry on a backup made by another app (mathom wrote to `moto_edge_60_pro`), choose its storage in the list: the first scan recognises what it holds (same path, or same category, name and size). Photos and videos carry on cleanly; a picked folder is recognised only if its short name matches the one mathom used (mathom's SAF ids differ, so expect those files to be sent again once).

**Verified on the Pixel_8 emulator** (stand-in server): a photo added while the window was in the background reached the server through the job, with the notification posted; the window picked the state up when reopened. **Not run on a phone**, and not run across a reboot.

### Phase 3: restore and reconciliation

- [x] 3.1 Reconciliation: after each scan the program reads the storage's change feed from a per-storage cursor (`sync-<storage>` in the data directory) and marks files the server already holds (same path and size, or same category, name and size when the device renumbered them) done instead of sending them. Pure logic in `Manifest::reconcile`.
- [x] 3.2 Restore: `Settings` → "Restore from backup" reads the whole listing (`Session::list_all`), works out what this device lacks (`restore::plan`; a scan now marks what has gone from the device, `Manifest::mark_missing`), asks, then fetches two at a time (`Restorer`, three tries each) and shows progress, stop, and the result. A file saved by Kotlin's `RestorePlugin` streams straight into the media library: photos to Pictures/Forklift, videos to Movies/Forklift, a backed-up folder's files to Downloads/Forklift/<folder> (the app has read-only access to a picked folder). Needs Android 10 or newer.
- [x] 3.3 What came back is not sent again: after a restore the files are scanned and told from the stored ones by category, name and size.
- [x] 3.4 Quota line on the dashboard ("Storage 48.9 KB of 1.0 GB"), refreshed after a scan and when uploads finish.
- [x] 3.5 Per-folder "Wi-Fi only" / "Any network" switch in the settings.

**Verified on the Pixel_8 emulator** (stand-in server with listing, feed, quota and download): with the manifest and the cursor deleted, a relaunch recognised all 9 stored files and sent none; a photo deleted from the device was offered ("Restore 1 file (5.4 KB)? 8 already on this device."), restored byte for byte into Pictures/Forklift, and not sent again afterwards. **Not verified:** a restore of a video or a folder's files, a restore of a big library, Android 9 and older.

Limits: a restored file's name is the sanitised stored name (letters, digits, `. _ -`), not the original; the folder tree and original dates are not restored (the file gets today's date); a stopped restore starts over (what was fetched is recognised, so only the rest is fetched again).

### Phase 4: the Linux host

- [x] 4.1 `forklift-desktop` (`forge/services/forklift/desktop`): the same screens in a Wayland window (`forklift`), the backup's controller shared with Android (moved into `forklift-host`; the worker gained `spawn` for blocking work such as a folder dialog). A computer backs up **folders only**: the photo/video switches and the charging rule are hidden (`Setup.phone`), `Linux` answers the platform's questions (folders walked with `std::fs`, links not followed, `.DS_Store`/`Thumbs.db`/`desktop.ini` and `.part` files left out; always online and on power; the folder dialog is `zenity`, `kdialog` or `yad`).
- [x] 4.2 `forklift --backup` for a systemd user timer (`desktop/systemd/forklift-backup.{service,timer}`, every 15 minutes): one backup with no window, a desktop notification when something was sent; it yields to the window through the same lock file.
- [x] 4.3 Restore on Linux: a folder's files back into the folder (a name that is free: `name (2).ext`), everything else under `~/Forklift Restore/{photos,videos,<folder>}`; written through a `.part` file; the server's names are made safe first.
- [x] 4.4 Packaging and release: `quench build deb|rpm|arch|source` all build (checked), and `.anvil.toml` has the four Linux targets next to the APK; the version of every build is `desktop/Cargo.toml`'s.
- **Tests:** 7 (folder walk, names and destinations, send/fetch against a stand-in server, a whole background run: new files sent, nothing the second time, a changed file again, and the "off / not signed in / no storage chosen" cases). **Verified on this machine:** the window opens (class `dev.lorehaven.forklift`, title Forklift) and keeps running with its own data directory; `forklift --backup` with an empty store says what it did not do. **Not verified:** a login and backup from the window against a real server, the folder dialog, a restore from the window, the systemd timer.
- **Limits:** a file over 512 MiB is not sent or fetched (the HTTP client reads a whole body; streaming needs a change in `quench-client`); no status notification and no foreground service (a timer does the work); the restore does not recreate the original folder tree or dates.

## Security

Password never stored; refresh token in the Keystore; access token in memory; no network calls except `{server}/gatehouse` and `{server}/warehouse`; tests use stand-in servers only, never the real warehouse.

- [x] The manifest is a SQLite database (`manifest.db`, `quench-sqlite`): `forklift-net`'s `Store` writes only the changed records in one transaction; the old text file is imported once; `Backup.kt` reads `manifest.counts` (2026-10-31). Open: run it on the emulator/phone.
