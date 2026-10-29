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
- [ ] 3.3 The web build (see below).

The web build needs a decision on how the service serves the wasm bundle (look at how other forge services ship theirs) and on the browser's session (token in memory + refresh).

### Phase 4: Android (`daybook-surface`, `android/`)

Reminders, the widget, the background refresh, the icon (`notebook-indigo`).

### Phase 5: release

`.anvil/daybook.toml` (apk, deb, rpm, arch, source), keystore, `anvil artifact release daybook`; the Flutter app retired when you say.
