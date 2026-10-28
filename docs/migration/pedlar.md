# Pedlar: the migration checklist

**Where it lives:** `forge/apps/pedlar` (a project made by `quench new pedlar --template web`, its own workspace, quench crates from the registry). The Flutter scaffold in `mobile-apps/pedlar` stays untouched until the replacement reaches parity and you approve retiring it. **There is no server of its own:** pedlar is a client of gatehouse and warehouse. The project's `server` crate only renders the page at build time and serves the dev loop; nothing from it ships.

**The spec** is `mobile-apps/pedlar/plans/PEDLAR.html` (blueprint) and `ROADMAP.html` (28 tasks). The Flutter app is an empty scaffold, so there is no behaviour to match beyond those two documents. Two things have changed since they were written, and this checklist follows the code, not the plan:

1. **The registry is multi-platform now.** The blueprint calls `/api/v1/apk` (still there as an alias); the real API is `GET /api/v1/artifacts?platform=<android|linux|windows|macos>`, `GET /api/v1/artifacts/{program}[/{platform}[/{version_code}]]`, `.../download`. A row (`ArtifactView`) has `program`, `platform`, `arch`, `format`, `version_code`, `version_name`, `filename`, `size_bytes`, `sha256` (bare hex), `label`, `min_sdk_version`, `target_sdk_version`, `permissions`, `uploaded_by`, `yanked`, `created_at`. Order by `version_code`, never by `version_name`.
2. **Desktop is in scope** (the blueprint deferred it). It lists `platform=linux` artifacts for the machine's arch.

## One app, two hosts

The wasm bundle is the same; the host decides what the native side can do. The app asks the host who it is (`device.info`: platform and arch) and requests that platform's artifacts.

| | Android host | Linux desktop host |
|---|---|---|
| Catalog | `artifacts?platform=android` | `artifacts?platform=linux`, filtered to the machine's `arch` (or `universal`) |
| What "installed" means | the system's package list (`packages.installed`) | pedlar's own ledger of what it installed (a file in the app's data directory: program, version code, name, where) |
| Install | `packages.install_from_url` (PackageInstaller session; the system confirms; the result is an event) | download and verify, then by `format`: `deb` is handed to the system installer (`xdg-open`; the desktop confirms), anything else is saved to a chosen folder and revealed |
| Update / uninstall | the same install path; `packages.uninstall` (the system confirms) | the same; uninstall for what the ledger knows (a `deb`: handed to the system) |
| Background check | `jobs.schedule` (JobScheduler, every 6 h, Wi-Fi only if chosen) | **not offered**: nothing wakes a closed desktop app (the setting is hidden) |
| Notification | `notifications.show` | `notifications.show` |

## What the framework must grow (found by reading the plan against Stage 9 and 10)

- **`http`, a native request plugin, on both hosts.** The page cannot call gatehouse and warehouse itself: the bundle's CSP is `connect-src 'self'` on purpose, the services send no CORS headers (and the plan says not to change them), and a token must not sit in page storage. `http.request {method, url, headers, body}` answers `{status, headers, body}`; `http.download {url, headers, sha256, destination}` streams to a file under the app's cache, reports progress as events, **checks the SHA-256 natively** and answers the path (a mismatch deletes the file and answers `hash_mismatch`). https only (plain http to loopback in debug builds).
- **`device.info` on both hosts:** `{platform, arch, model, sdk}`.
- **Android `packages`:** `install_from_url` takes `headers` already (the typed client sends only the url), but not `sha256` (a downloaded APK must be verified before it is committed), and there is no `uninstall` or `open`. The typed client is extended to match.
- **Linux `packages`** (new, the desktop plugin of the same name): `installed {program}` from the ledger, `install {path, format}`, `uninstall {program}`, `open {program}`; events as on Android.
- **The headless update check (phase 3)** runs in the `native` Rust program on Android, so it needs an HTTP client there (`quench-client` for the NDK targets) and the refresh token, which lives in the Keystore (`secure_storage`) and cannot be read by a separate process: the job's input will carry it, supplied by Kotlin from the same storage.

## Checklist

Phase 1 is usable before phase 2 starts, as in the roadmap. A box is ticked when it is demonstrably done.

### Phase 1: browse and install

- [ ] 1.1 Project: `forge/apps/pedlar` made; dark theme by default (quench-style); `desktop/` and `android/` added with `quench linux init` and `quench android init --package dev.lorehaven.pedlar`. (Project and core crate exist; the hosts are not added yet.)
- [ ] 1.2 Settings: the server URL (trailing slash stripped), kept in `localStorage`-equivalent app storage; no secrets.
- [ ] 1.3 Gatehouse auth: `POST /gatehouse/api/v1/auth/login {username, password}`, `/refresh {refresh_token}` (rotates: persist the new token **before** the next request), `/logout`. The password is used once and never stored; the refresh token goes in `secure_storage`; the access token stays in memory and is refreshed at about 80% of `expires_in` and on a `401` (one in-flight refresh, one retry). `mfa_required` is shown as "turn MFA off for this account".
- [ ] 1.4 First-run setup screen: server URL, username, password; shown whenever there is no valid session.
- [ ] 1.5 Registry client over `http`: catalog, versions of a program, one version, with the bearer header and the refresh-and-retry. A `404` on the catalog is "registry not enabled", shown as a hint and not as an error.
- [x] 1.6 The `Artifact` model (`pedlar-core`, tested against a captured row; optional fields are `None`, a row with no version is an error).
- [ ] 1.7 Installed list: Android `packages.installed` per catalogued program; Linux the ledger.
- [x] 1.8 Install-state logic: `state_for`, `build_entries` (updates, then current, then not installed; alphabetical within a group; the running app marked; a yanked row is never the latest; one row per program), `format_bytes`. 8 tests.
- [ ] 1.9 Download and verify through `http.download` with the expected `sha256`; progress per program; a mismatch removes the file and says "the download was corrupted, try again".
- [ ] 1.10 Install: Android `packages.install_from_url` with the bearer header and the hash; `can_install` is checked first and routes to `open_install_settings` with an explanation when false; Linux as in the table above.
- [ ] 1.11 Catalog screen: tiles with an icon by state, title, program and version, and a button (Install / Update / Open) bound to the progress; search; pull to refresh; the actions Updates, Settings, Log out; state re-derived when the app regains focus (the install result also arrives as an event on Android).
- [ ] 1.12 App detail: label, uploader, size, version name and code, min/target SDK (Android), created date, permission chips (Android), primary action, version history from `artifacts/{program}/{platform}` with a "yanked" tag.

**Done when** (live, on a device and on this machine, against the real deployment): from a clean install a person sets the server, logs in, sees the catalog, opens an app, installs it through the system's confirmation and sees the tile become Open.

### Phase 2: updates

- [ ] 2.1 The update count and badges, from `build_entries`.
- [ ] 2.2 Updates screen: the updatable entries, the running app first, each with its own progress.
- [ ] 2.3 "Update all": one at a time (the system shows one dialog at a time), advancing when the previous install's result arrives; the running app last.
- [ ] 2.4 Self-update banner: the entry with `is_self` and an update available; the running version code comes from the host.
- [ ] 2.5 Uninstall (the system confirms).
- [ ] 2.6 Install a specific version from the version history (including an older one).

**Done when:** a higher version of an installed program is published (to a stand-in registry, never the real one without your say-so), refresh shows Update everywhere, "Update all" walks the system dialog once per app, and a higher version of pedlar itself reinstalls pedlar over itself.

### Phase 3: background checks (Android)

- [ ] 3.1 Scheduler: `jobs.schedule` every 6 h, Wi-Fi only optionally, replaced when settings change; a settings switch with the notification permission requested when it is turned on.
- [ ] 3.2 Headless check: refresh the token, fetch the catalog, list what is installed, `build_entries`, diff against `notified_versions`, notify only what is new, store the set (so nothing is announced twice); never notify about a failed check.
- [ ] 3.3 Notification: one low-importance channel, "N updates available" with up to three names; tapping opens the app.
- [ ] 3.4 Settings screen: read-only server URL, the background switch, "only on Wi-Fi", "Check now" (the same check, in the page), the running version.

### Phase 4: polish

- [ ] 4.1 Install-permission gating flow and its explanation (Android).
- [ ] 4.2 Search and filter; empty states; error surfaces (`401` twice ends the session with "session expired"; a network failure retries from zero, there is no resume).
- [ ] 4.3 Launcher icon and desktop menu entry; the `.deb`.
- [ ] 4.4 Release APK signed; `quench publish --dry-run` shows what would be sent (**never** a real publish without your approval).

## Security (carried over, unchanged)

Password never stored; refresh token in the Keystore or keyring; access token in memory only; every download verified against the registry's `sha256` before it is installed (Android verifies the signing block as well); no analytics, no crash reporting, no network calls except `{server}/gatehouse` and `{server}/warehouse`; the broad permissions (`REQUEST_INSTALL_PACKAGES`, `QUERY_ALL_PACKAGES`) declared plainly, since the app is distributed through warehouse and not through a store with a policy on them.

## Tests

Unit tests for the pure logic first (done for 1.6 and 1.8); the registry client and the update check against a stand-in warehouse and gatehouse (the `quench-test` HTTP mock); a browser test for the screens; the live, on-device pass is yours.

## Status on the native renderer (2026-10-28)

On hosts that run Rust (the desktop window and, next, the Android surface), pedlar calls gatehouse and warehouse **from Rust**, so the `http` plugin of the web hosts is not needed there: `forge/apps/pedlar/net` (`pedlar-net`) logs in, rotates the refresh token, lists the catalog (a `404` is "registry not enabled"), and downloads with the SHA-256 checked before the file is kept; `native` (`pedlar-native`) keeps the session in a file only the user can read, runs the worker thread, **installs** (downloads, hands the file to the desktop with `xdg-open`, records it on a ledger of installed programs so the catalog shows Open or Update) and updates everything that has an update (this app last). `app` has the screens and the live state machine (`Request` out, `Msg` in), tested with no host. Tests use stand-in servers only. Not built: a detail screen, progress while a file arrives (it arrives whole), the background check, the installed ledger from the system's package manager (the ledger records what pedlar handed over), and the Android native host (the surface plugin and `packages` are there to do it with).

### Android (2026-10-28)

`forge/apps/pedlar` is also an Android project now: `android/` (made by `quench android init`, the `surface` and `packages` plugins, launcher on `.SurfaceActivity`) and `surface/` (`pedlar-surface`): the same screens and worker as the desktop, with the program calling the app's **`packages`** plugin through the surface bridge. A download goes to the app's files directory, `packages.can_install` is checked (`open_install_settings` when not allowed), `packages.install {uri: file://...}` hands the APK to the system installer, and its `install` event ends the install (success records the ledger; failure or cancel is shown). What is installed comes from the package manager: after each catalog the program asks `packages.installed {package}` for every program and the app merges the answers (`Msg::Known`). Verified on the Pixel_8 emulator against a stand-in registry (`10.0.2.2`): sign-in, the catalog, a program's page, and an install that **reached the system installer's confirmation**; the emulator's package verifier then refused the APK (`INSTALL_FAILED_VERIFICATION_FAILURE`) and the refusal came back into the app's message line. A successful install and the ledger it writes were not seen on a device.

The app also has a **page for each program** (version, size, platform, publisher and date, the hash in two halves so it wraps, what is installed, the Android API range and permissions when the registry has them, the same button as the row, Back) and looks for updates **in the background** every six hours while signed in (the "Check for updates automatically" setting switches it off): a look that finds more updates than the last says "N updates available". Not built: progress while a file arrives (it arrives whole), a notification for the background check when the app is closed, and the install of the app by itself on Android.
