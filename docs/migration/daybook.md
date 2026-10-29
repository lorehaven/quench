# Daybook: the migration checklist

**Where it lives:** `forge/services/daybook`, a flat-layout project in forge's workspace (quench crates from the registry), made of the app **and** its service:

```text
service/   daybook-service, moved from forge/docker/daybook-service (the API: tasks, lists, labels, repeats)
core/      the logic with no I/O: tasks, lists, labels, repeat rules, due dates and their colours, the order, reminders, the widget's snapshot, the calendar
net/       the API over gatehouse-client; the saved session and settings
app/       the screens (a calendar and a time picker of their own: the native renderer has no pickers)
host/      the controller both hosts share (optimistic changes, reminders, the widget) over a `Platform`
surface/   the Android program (and `--widget`, the refresh with no window)
desktop/   the Linux window
android/   the Gradle project (reminders by the `notifications` plugin, the home-screen widget in Kotlin)
```

**Targets: mobile (Android), desktop (Linux) and web.** `app/` is built as Hoist's is (`crate-type = ["cdylib", "rlib"]`, `quench-ui` + `quench-ui-web`): one set of screens that the native renderer draws on Android and Linux and the browser runs as wasm, served by the daybook service. The web client talks to the same `/daybook/api/v1` with a gatehouse login in the page; reminders on the web are the browser's notifications while the page is open, and there is no home-screen widget. The Flutter app had no web version, so this is new.

The Flutter app `mobile-apps/daybook` stays until the replacement reaches parity and you approve retiring it. Package id and program stay `dev.lorehaven.daybook`, **but** the Flutter app's release builds are signed with the Android *debug* key, which a proper release key cannot replace in place: uninstall the Flutter app once before installing the new one (nothing is lost: the tasks are on the server).

## Behaviour to match (read from the Flutter source)

- Inbox, custom lists (projects), labels; a task with no project is in the inbox. `PUT /tasks/{id}` replaces the whole task, so an edit sends everything. Deleting a list deletes its tasks (asked first).
- Due date with a time, or **all day** (a date: midnight UTC, read back as that UTC date in any zone, only overdue once its day has passed). Due colours: red overdue, green today, orange tomorrow, yellow within 3 days, blue within a week, none beyond. Order: open tasks by due (an all-day task before the timed ones of its day), undated last; completed by last change.
- Reminders: a preset relative to the due time or any date and time; the server stores `remind_at` only, the app schedules a local notification for every open task with a future reminder (replacing the set after any change and at every start). Android: with the app closed, re-armed after a reboot. Linux: only while the app runs.
- Labels (per user): tag a task with any number, a label's view lists every task with it, edit/delete from that view.
- Repeats: every N day/week/month/year, weekdays for a weekly rule, on schedule or after completion; the **server** rolls a completed repeating task to its next date ("Done — next: Mon 2 Nov 09:00"); *Mark occurrence done* and *Stop repeating* (with Undo). The rule's zone is the device's.
- **No "Show completed"** (the Flutter app had it; dropped on purpose): a checked-off task leaves the list and completed tasks are never fetched or shown. A repeating task comes back at its next date as the service rolls it.
- Checkbox completes/reopens; swipe left deletes (confirmed); changes are optimistic and roll back with a message; pull to refresh; a sidebar from 840 px, a drawer below.
- Android home-screen widget: full width and resizable, a list picker (inbox, a project, a label; remembered per widget), a **+** that opens the add-task popup for that list, the open tasks sorted and coloured as in the app, refreshed every 30 minutes with the app closed (needs the saved login), redrawn every 30 minutes so "Today" stays right; signing out clears it.

## What the stack already has, and what it lacks

Has: `notifications` (scheduled reminders that survive a reboot), the surface program with a bridge to plugins, the headless-job pattern (Hoist's `--check`, Forklift's `--backup`), `quench-tz` (zones, `civil_at`, `resolve`), the shared gatehouse client.

Lacks: a **date picker and a time picker** on the native renderer (an `<input type="date">` is drawn but cannot be edited by touch): built in `app/` for this app. The home-screen widget with a task list is Daybook's own Kotlin.

## Checklist

### Phase 0: the service moves

- [x] 0.1 `forge/docker/daybook-service` → `forge/services/daybook/service`: workspace member, `foreman.toml` (`workdir`), `forge-bdd` (finds a service under `services/<app>/service`), docs; the image build already works from any directory (anvil passes the package's own path). 21 unit + 10 recurrence tests pass; the integration suite needs a database.
- [x] 0.2 The Dockerfile's list of workspace members it does not build (`WORKSPACE_EXCLUDES`) is one `ENV` now, and has Forklift's windowed host and Android program in it (they were missing).

### Phase 1: the logic (`daybook-core`)

- [x] 1.1 `forge/services/daybook/core`, a workspace member: the service's JSON (tasks, lists, labels, repeat rules; the replacing update sends every field), due days (an all-day date is its UTC date in any zone), urgency and its colours, written due dates, the order, views (inbox / list / label), reminder presets and the notifications to schedule (stable FNV-1a ids, at most 200), the task form's rules as a `Draft` (a preset follows the due date, is dropped for the other kind of date, clearing the date clears the repeat), the date picker's month grid. 22 tests, clippy clean.
- [ ] 1.2 The widget's snapshot (read `widget_snapshot.dart` first).

### Phase 2: the API and the controller (`daybook-net`, `daybook-host`)

- [x] 2.1 `daybook-net`: `Session` over `gatehouse-client` (open/completed tasks, create/replace/delete task, lists, labels; statuses mapped: 400 invalid, 403, 404 gone, 409 name taken), `Store` (session 0600, settings: server, user, open view, show completed). 7 tests against a stand-in server, clippy clean.
- [x] 2.2 What the app and its host say to each other is plain data in `daybook-core` (`Request`, `Msg`, `Data`), so the host does not depend on the screens and the web client can use the same. `daybook-host`: the `Controller` over a `Platform` (loads everything; checking off and deleting show at once and are undone with a message if refused; a repeating task waits for the service and says "Done — next: …"; saving reports back to the open form; reminders are replaced 2 s after the last change and at every load; a session that ended signs out and cancels them), `Remote` (the calls against a real session) and a `Worker` that runs calls one after another on one thread. 11 controller tests with a fake platform, clippy clean. `Remote::execute` is not covered by a test yet (it is by the first run against a stand-in server).

### Phase 3: the screens, the Linux window and the web client (`daybook-app`, `daybook-desktop`, wasm build served by the service)

- [x] 3.1 `daybook-app`: sign-in; the inbox, lists and labels (a sidebar from 840 px by CSS, a sheet below); tasks with due text in the urgency colours, the repeat and the labels; check off, swipe to delete (asked first), pull to refresh, "Show completed"; the task form (title, notes, due, repeat, reminder, list, labels, *Mark occurrence done*, *Stop repeating*, Delete) whose pickers (a month calendar, a time picker with +/- and usual times, the reminder and repeat menus, a custom repeat, a new label) replace the dialog's content instead of stacking modals; naming/renaming lists and labels. 11 screen tests, clippy clean. Drawn offscreen with sample data (`cargo run -p daybook-desktop --example shots -- <dir>`): the phone and the wide layout look right.
- [x] 3.2 `daybook-desktop` (`daybook`, class `dev.lorehaven.daybook`): the Linux `Platform` (calls on the worker thread, reminders as timers shown with `notify-send` while the window is open: 3 tests). **Verified against a stand-in server** (no real service): a stored session refreshed and the token rotated into the 0600 session file, then the lists, labels and open tasks loaded; the window opened with the right class and title. Not driven by hand (typing a password, creating a task from the window).
- [x] 3.3 The web client. `quench-ui-web` 0.1.7 gained `quench_ui_web::browser` (the wall clock, the browser's time zone, a small `localStorage` store; `quench.js` is still under 400 lines). `daybook-net` and `daybook-host` build for wasm without the blocking client (`client` / `native` features); the paths, bodies and sign-in errors both clients share are in `daybook_net::api`. `daybook-web` (`cdylib` + `rlib`): `Client` over an abstract transport (sign-in, one renewal on a refused token, the refresh token kept in the page's storage), `Web` platform (calls one after another, no scheduled reminders), `app()` and `stylesheet()`. The service serves it: `/ui` (the page; a bare `/daybook` already redirects there), `/ui/assets/{quench.js,app.css,app.wasm}`, all public; the module is read from `WEB_DIR` (default `web`) as `app.wasm`, and without it the page says the client is not installed. 9 client tests. **Verified in Chrome against a stand-in server** (not the real services): the page loads and hydrates, no server field in the sign-in, sign-in, the lists and tasks with times in the browser's zone, the session survives a reload (a refresh, no password), checking a task sent `PUT /tasks/<id>`. The release module is 678 KB.
- [ ] 3.4 Shipping the module: the image has no wasm step (`cargo build --target wasm32-unknown-unknown --profile wasm -p daybook-web`, then `app.wasm` into `WEB_DIR`). Not done: the Dockerfile is shared by every service. Reminders in the browser (the `Notification` API while the page is open) are not done either.

The web build needs a decision on how the service serves the wasm bundle (look at how other forge services ship theirs) and on the browser's session (token in memory + refresh).

### Phase 4: Android (`daybook-surface`, `android/`)

- [x] 4.1 `android/` (`quench android init`, plugins `surface` and `notifications`; the Flutter app's `MainActivity` is not in the manifest, `SurfaceActivity` is the launcher), the `notebook-indigo` launcher icon (adaptive and round, made from `icon.png`), `ACCESS_NETWORK_STATE`. `daybook-surface` (`quench_surface`): the shared controller over `Android` (calls on the ordered worker, reminders through `notifications`: what is armed and no longer wanted is cancelled, the rest scheduled again, the same id replacing the alarm). The Kotlin host passes the device's zone as `TZ` (also in quench's template and docs). Both ABIs build.
- [x] 4.2 The home-screen widget, ported from the Flutter app's Kotlin: header with the list picker and a **+**, the open tasks of the chosen list (sorted and coloured as in the app, text worked out when it draws), a list picker per widget. The program writes `widget.json` (`daybook_core::widget`, the Flutter snapshot's format) after every change and tells the `daybook` plugin to redraw; a JobService (30 minutes, needs a network, kept across reboots) runs `quench_surface --widget` (the saved login, no window) and redraws; signing out empties the widget. **+** and a task write a `launch` file that the program reads (`Msg::Show`): the add form for that list, or the task.
- **Verified on the Pixel_8 emulator against a stand-in server (never the real services):** sign-in, the lists, a due time in the device's zone, the permission prompt, **a reminder fired on time and posted "Call the dentist / Due Thu 29 Oct 16:00"**, the widget (added from the launcher) listing the inbox with the colours, its **+** opening the add form, the refresh job scheduled. A bug found on the way and fixed: scheduling the job without `ACCESS_NETWORK_STATE` crashed the activity (and now scheduling can no longer throw).
- **Not verified:** a phone; the widget's list picker, a tap on a task row, the 30-minute refresh actually running (it is scheduled), a reminder surviving a reboot (the plugin re-arms them), a task opened from a reminder's notification (it only opens the app), landscape/tablet layouts, TalkBack.

### Phase 5: release

- [x] 5.1 `.anvil/daybook.toml` (apk, deb, rpm, arch, source; the version is `desktop/Cargo.toml`'s), `anvil config check` valid, Dockerfile excludes for `daybook-desktop` and `daybook-surface`.
- [ ] 5.2 The release keystore: `.quench.toml` names `release.keystore` in `services/daybook` (ignored by git). Copy a keystore there (any one signs any number of apps; the password is the one in `.anvil/secret.toml`) or make a new one with `quench android keystore`. Then `anvil artifact release daybook`.
- [x] 5.3 The Flutter app (`mobile-apps/daybook`) is retired: its README says so and points here (the code stays for reference). Uninstall it once before installing the new one (it was signed with the debug key).
