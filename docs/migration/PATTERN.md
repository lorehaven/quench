# Rewriting a forge service as a browser client: the pattern

Done and verified for **Gauge** (`forge/services/gauge`, plan: [gauge.md](gauge.md)) and **Workbench** (`forge/services/workbench`, plan: [workbench.md](workbench.md)); Daybook's web client follows it too. Read those two plans and their crates first: they are the reference, and everything below is what they settled.

The rule: the service **keeps its API, its data and its SSO routes**; the htmx pages (server-rendered `forge-ui` + `quench-web`) are replaced by a `quench-ui` client that runs in the browser as wasm and talks to the service's JSON API. **Every feature of the old pages stays** (read each old page and match it; a feature the API cannot serve yet gets a small API addition, with a test).

## Layout of `forge/services/<name>/`

```text
service/   <name>-service: the API (kept), the SSO routes (kept: /ui/login, /ui/auth/callback, /ui/logout, /ui/status, /ui/refresh), the page that starts the client
core/      <name>-core: the wire model, forms' rules (drafts), logic: no I/O, builds for wasm32, unit tests
app/       <name>-app: the screens over `Request`/`Msg` (no I/O), screen tests with a test renderer
web/       <name>-web: cdylib+rlib wasm: `fetch` to the API with the session cookie; `quench_ui_web::app!(app)`; tests with a stand-in transport
icon.png   the service's icon (see below)
```

Crate manifests: copy `services/workbench/{core,app,web}/Cargo.toml` (edition/authors/... from the workspace, `autotests = false`, custom harness `[[test]] harness = false`, `quench_test::main!()` at the end of each test file, tests under `tests/`). Add the three crates to `forge/Cargo.toml` `[workspace] members` and to the workspace dependency table (`<name>-core = { path = ... }` etc.). The service crate drops `forge-ui` and `quench-web` and gains `forge-web`, `<name>-core`, `<name>-web`.

## Hosting (already built, use it)

* `libs/forge-web` (`Client`): `static CLIENT: Client = Client::new("Name", <name>_web::stylesheet).framed().icon(include_bytes!("../../../../../icon.png"));` — in `service/src/routers/ui/pages/client.rs` route the pages (behind the service's own `ActorOrRedirect`: no session → redirect to `/ui/login`) and the public files `/ui/assets/{quench.js,app.css,app.wasm,shell.js,favicon.png}`. Copy `services/workbench/service/src/routers/ui/pages/client.rs` and `routers/ui/common/mod.rs` (it keeps `actor`, `ActorOrRedirect`, `ui_login_redirect`). Keep `pages/auth.rs`. Delete the htmx pages, `common/css.rs`, the UiShell code and the integration tests of the old pages (replace by a `tests/unit/client_tests.rs` like Workbench's: pages served or "not installed", redirect when signed out, files public).
* `service/web/.gitkeep` (committed) and `.gitignore` `/web/*.wasm`; the module is read from `WEB_DIR` (default `web`) as `app.wasm`.
* `.anvil/docker.toml`: under the service's module add `build_args = { RESOURCE_DIR = "web", WASM_PACKAGE = "<name>-web" }` (see Gauge's block).
* `forge-shell` (`libs/forge-shell`): the frame. In the app's `live.rs` build a `forge_shell::Bar { brand, image: Some("{base}/ui/assets/favicon.png"), home, spa: true, account: true, brand_in_bar: false, logout: "{base}/ui/logout", on_logout: None, languages, on_language }`, then `forge_shell::sidebar(&bar, nav_views)` (brand + whatever navigation the service has, as `fui-nav-item` / `fui-nav-sub` > `fui-nav-subitem` links), and `forge_shell::topbar(&bar, &i18n)` inside a `View::dynamic` so it follows the language. Layout: `div#<name>.fui-app.fui-client > [sidebar, div.fui-client-col > [topbar, main]]`. See `services/workbench/app/src/live.rs`.
* Style: the app's `style.css` only adds its own classes, written with the forge tokens (`var(--fui-bg)`, `--fui-surface`, `--fui-border`, `--fui-text`, `--fui-brand-500`, `--fui-error-soft-bg`, ...): dark by default, light and palettes follow the person's choice. Keep the look of the old pages (cards, badges, tables, forms: reuse the `fui-*` classes from `libs/forge-ui/assets/components.css` where they fit). No hard-coded colours. Do not set `display`/`height` on the `#<name>` root (the frame owns it).
* i18n: the service's `i18n/*.ftl` (five locales) are embedded with `include_str!("../../service/i18n/<loc>.ftl")` and read by `quench_ui::i18n::I18n`; keep the keys, add the few the client needs (loading, not found, remove, ...) to all five files; the shell keys (`ui_theme_*`, `ui_logout`, `locale_label`, `ui_profile_edit`, `ui_dashboard_button`, `ui_home_button`, `ui_user_menu`, `ui_palette_*`) must be in the files (copy from Gauge's). The language is stored in `quench_ui_web::browser::storage_*` under `language` (the shell script seeds it from the `qlocale` cookie).

## App structure (see Workbench)

* `Request` (what the app asks of its host) and `Msg` (what the host answers) are plain data enums in the app crate; compound reads (a page needs three calls) and writes followed by a re-read are the **web crate's `Client::execute`** (over a `Transport` trait, so tests use a stand-in); `Msg::Rejected(text)` shows a notice, `SignedOut` a sign-in link, `NotFound` a not-found screen.
* Lists that change: `View::keyed(items, key, render)` with a key that includes everything the row shows (carry changing data in the key, as Daybook's `Row::key` does), or the whole list rebuilds on every change. Read big signals with `.with(|v| ...)`, not `.get()`.
* Forms: `quench_ui::form::Field` (+ `field.bind(textarea())`, selects with `attr_bool("selected", ...)` options and `on_input`), checked by a `Draft` type in core that returns the request body or a `DraftError` with a message; show the message under the form.
* Charts: `quench-ui-web` has no SVG elements: draw as `<img src="data:image/svg+xml,...">` (see Gauge's `chart.rs`), colours that read on dark and light.
* Drag and drop: `forge-shell`'s script handles it (`data-drag-id`, `data-drag-select`, `data-drop-value`: a drop sets a `<select>` and fires its input event); see Workbench's board.
* Not available in the browser client: streamed response bodies of `fetch` (use `quench_ui_web::sse::Sse` for server-sent events, GET only), cookies (the shell script does theme/language), SVG elements.

## Tests (required)

* `core`: unit tests; `app`: screen tests with `TestRenderer` + `MemoryHistory` (Workbench's `tests/screens.rs`); `web`: `Client::execute` tests with a stand-in `Transport` (Workbench's `tests/unit.rs`); `service`: the existing API tests stay (the integration tests need `<NAME>_TEST_DATABASE_URL`: run them against a throwaway Postgres, see below) plus `client_tests.rs`.
* `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` (pedantic/nursery are denied: expect `semicolon_if_nothing_returned`, `many_single_char_names`, etc.; tests allow a few), `cargo check -p <name>-web --target wasm32-unknown-unknown`.
* After `cargo fmt` reflows code, a scripted text replace on it can silently not match: assert the match.

## Verifying in a browser (do it; never against real services)

1. A throwaway Postgres if the service needs one: `docker run --rm -d --name <name>-test-pg -e POSTGRES_PASSWORD=postgres -p <PORT>:5432 pgvector/pgvector:pg18`, then `cargo run -q -p foundry-service -- apply --catalog services/foundry/service/migrations --database-url postgres://postgres:postgres@localhost:<PORT>/postgres --install <name>`; seed users the service needs (see the Workbench run: `auth.users` rows `admin`, `dev`). Stop it (`docker stop`) when done.
2. `cargo build --target wasm32-unknown-unknown --profile wasm -p <name>-web`, copy `target/wasm32-unknown-unknown/wasm/<name>_web.wasm` to `$WEB_DIR/app.wasm`; `cargo build -p <name>-service`; run the binary from `services/<name>/service` with env: `DATABASE_URL=... GATEHOUSE_URL=http://127.0.0.1:9 WEB_DIR=... BASE_PATH=/<name> SERVER_ADDR=127.0.0.1:<PORT> SERVICE_AUTH_ENABLED=false REDIS_URL=redis://127.0.0.1:16379 SERVICE_NAME=<name>` (plus whatever the service itself needs; `GATEHOUSE_URL` must be set but is never called). The stylesheet is compiled into the service binary: rebuild and restart the service after CSS changes.
3. Click through every screen and action with the Chrome tools (open your own tab, close it after), including the language switch, the theme menu, and a write that round-trips. Use only local pages.
4. Stop what you started (kill specific PIDs found with `ss -ltnp | grep :<PORT>`, never by pattern).

## Icon

Each service has one icon from `/mnt/extdrive/projects/lorehaven/icons/` (`<name>-<colour>-256.png`): copy it to `forge/services/<name>/icon.png`; it is the tab icon and sits next to the name in the side panel.

## Hard constraints (the user's standing rules)

* **Never commit, push, publish, release, `git init`, or run any git command that changes anything** (no checkout/reset/revert/rm of files). Delete only files you created or that the rewrite replaces. Do not use `anvil release`/`warehouse` publishing.
* **Never read `.env` files, `homecloud/.env`, `.anvil/secret.toml` or other secrets.** Never contact the real services (warehouse, gatehouse, daybook server, the cluster, `ennor.ddns.net`); only local processes. Do not send credentials to anything but a local stand-in.
* Dependency policy: no third-party crates beyond std, libc, getrandom, tokio, rustls+ring+webpki-roots, proc-macro2/syn and quench crates. Avoid `unsafe`.
* Never kill processes by pattern (`pkill -f`); only specific PIDs. Do not touch `.idea/` or `.claude/`. Do not screenshot the user's desktop (only Chrome tabs of local pages).
* Keep each tool call under ~2 minutes (use background runs and polling for long builds).
