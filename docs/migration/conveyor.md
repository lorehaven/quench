# Conveyor: the migration checklist

**Where it lives:** `forge/services/conveyor`, Workbench's pattern (see [PATTERN.md](PATTERN.md), [workbench.md](workbench.md)) with polling, a live log stream and a few read-only report views.

```text
service/   conveyor-service: the REST API (kept, small reads added), the SSO routes, the page that starts the client
core/      conveyor-core: the domain records the API speaks (moved out of the service), wire models, drafts, formatting, the
           project tree, the job graph: no I/O, builds for the browser
app/       conveyor-app: every screen over `Request`/`Msg`
web/       conveyor-web: the wasm module: `fetch` + `EventSource` (the live log) with the session cookie
```

## Behaviour to match (read from the htmx pages)

- **Home** (`/ui/home`) and **a project's branch** (`/ui/projects/{id}`, same layout rooted at a node, with breadcrumb
  `Home / root / ... / project`, 404 text for an unknown project):
  - "Recent runs" panel: a table (status pill, repository `owner/name` linking to the run, ref, short sha, trigger, relative time),
    capped per repository and in total by the service config (`CONVEYOR_HOME_RECENT_RUNS`, `CONVEYOR_HOME_MAX_RUNS_PER_REPO`),
    **polled every 5 s**; "View all pipelines" link (keeps `?project=`).
  - "Repositories" panel: the registered projects as a tree (native `<details>`, open, a node links to `/ui/projects/{id}`; a leaf
    project's repositories fold into one table, an empty leaf is a muted link); "Manage repositories" link.
    Repo table: name (link to scan), provider, default branch, Enabled/Disabled, check chips L M A C T S (coloured by the latest scan:
    none / clean / warning / danger, with count or coverage metric, tooltip = check name), actions: **Run now** (only when enabled;
    refreshes the runs panel at once) and the "Run with parameters" link.
- **All pipelines** (`/ui/runs?project=&page=`): paged runs table (page size from config), breadcrumb when scoped, pager
  (Previous / Page n of m / Next, disabled ends keep their place).
- **Run** (`/ui/runs/{id}`): header (status pill, `owner/name` or "unknown repository", ref, sha, message), meta (trigger, stages when limited,
  queued, duration "so far" while running, attempt), the reason panel when the run has an error, **Restart** button when failed/cancelled
  (goes to the new run), the job graph (one row per dependency level, arrows between rows, one card per stage with its jobs), each job a
  disclosure with status pill, `stage/name`, "Reused from an earlier run", `exit N`, duration; opened: the log (loaded on first open only:
  Open raw log link, Copy log button, stdout/stderr lines, stderr distinguishable), a skipped/queued job shows its reason instead; the
  artifacts panel (name link to the public URI, short digest). **Polled every 2 s while the run is not terminal**, stops once it rests.
- **Repositories** (`/ui/repos`): list (`owner/name`, project path, provider, branch, Enabled/Disabled, Edit when writable, Follow /
  "Following - click to stop"), "Follow projects" panel (every readable project, toggle), "Add a repository" form (owner, name, clone URL,
  branch placeholder `master`, writable project select, provider GitHub/Generic) shown only when a project is writable; ok/error banners
  (`ui_repos_ok_*`, `ui_repos_err_*`, `ui_follow_*`); back to home.
- **Edit repository** (`/ui/repos/{owner}/{name}/edit`): fields owner, name, clone URL, branch, project select (writable ones), Enabled
  checkbox, Save; read-only copy with the "view only" hint without write access; "Danger zone" with Delete (list afterwards, banner);
  not-found text.
- **Run with parameters** (`/ui/repos/{owner}/{name}/run`): branch/tag (bare name becomes `refs/heads/...`), a checkbox per known stage
  (ticked when an ordinary run has it, "normally left out of a run" otherwise), "none known yet" hint, other stages (comma separated),
  disabled submit + hint without write access, errors (none selected, failed, forbidden); success opens the new run.
- **Scan overview** (`/ui/repos/{owner}/{name}/scan`): header with the launch link, run row (sha link to the run, ref, time), a card per
  check (count/metric, title, headline, "sha · trigger · time" origin), "no runs"/"no checks" texts.
- **Check detail** (`.../scan/{category}`): back link, title, "headline - from job (origin)", the stored report in a sandboxed iframe with
  open-in-new-tab and download links (tests, security), the findings (title, severity badge, id, date, location, extra), "Nothing found";
  security also lists the **scanner findings** with a severity filter (a box per level with its count, Apply; "never filtered" shows every
  level) and "N / total - time".
- **Credentials** (`/ui/credentials`): readable credentials only (name, `project: path` or `repo: owner/name`, kind, username, preview,
  relative time), empty text. Never the token.
- **Sidebar**: Pipelines, Credentials. Five locales (`i18n/*.ftl`, shared with the old pages), light/dark/palette from the shell.
- Auth unchanged: `/ui/login`, `/ui/auth/callback`, `/ui/logout` stay; `/api/v1` is behind `Auth`; per-project grants
  (`conveyor:project:<id>:read|write`) are enforced by the API.

## Choices

- **Logs:** the live log is `GET /api/v1/jobs/{id}/stream` (server-sent events) read through `quench_ui_web::sse::EventSource`
  (same origin, cookie); `stdout`/`stderr` events append lines, `done` closes it (no reconnect duplicates). Opened on first expand.
- **Polling** (`quench_ui::platform::after`, like Gauge): the home runs panel every 5 s, a running run every 2 s.
- **Copy log:** `quench-ui-web` has no clipboard call; `forge-shell`'s script gets a small generic handler (`[data-copy-target]`).
- The old HX-redirect/POST-redirect forms become API calls and a banner.

## What the API lacks for a client

Added to `/api/v1` (behind the same `Auth`, read access enforced like the existing lists):
- `GET /api/v1/access` -> `{ username, read: [project ids], write: [project ids] }` (what the pages computed per request).
- `GET /api/v1/projects?all=true` -> every project (the tree, paths, breadcrumbs).
- `GET /api/v1/runs/recent?project=` -> the front page's capped runs; `GET /api/v1/runs/page?project=&page=` -> `{ runs, page, pages, total }`.
- `GET /api/v1/scan` -> the check chips of every visible repository.
- `GET /api/v1/repos/{id}/scan` -> the latest scan (run, checks with findings, origins); `.../scan/findings` -> the scanner findings;
  `.../scan/{category}/report[?download=1]` -> the stored report page (the htmx page had it at `/ui/...`; same sandbox headers).
- `GET /api/v1/credentials` -> the credentials the caller may read (no tokens).
- Run detail artifacts carry `href` (the public warehouse URI).

## Checklist

### Phase 1: the API
- [x] 1.1 `access`, `projects?all`, `runs/recent`, `runs/page`, `scan`, `repos/{id}/scan[/findings|/{category}/report]`, `credentials`, artifact `href`, with integration tests.
- [x] 1.2 `stream.rs` no longer needs `quench-web` (its html frame is escaped by hand).

### Phase 2: `conveyor-core`
- [x] 2.1 Domain records moved in (the service re-exports them), wire models, request bodies.
- [x] 2.2 Drafts (repo create/edit, launch), formatting, the project tree, the job graph, scan levels, paging.

### Phase 3: `conveyor-app`
- [x] 3.1 Shell: top bar, language, sidebar, banner.
- [x] 3.2 Home / project, runs history.
- [x] 3.3 Run page with polling, log viewer, restart.
- [x] 3.4 Repositories, edit, launch.
- [x] 3.5 Scan overview/detail, credentials.
- [x] 3.6 Screen tests with sample data.

### Phase 4: `conveyor-web` and the service
- [x] 4.1 `fetch` for every request, `EventSource` for the log.
- [x] 4.2 The service serves the client at all the page routes behind `ActorOrRedirect`; htmx pages, `forge-ui`, `quench-web` leave the crate.
- [x] 4.3 `.anvil/docker.toml` (`RUNTIME_PACKAGES = "git"` kept, `RESOURCE_DIR`, `WASM_PACKAGE`), `web/.gitkeep`, `.gitignore`, `icon.png`.
- [x] 4.4 Run locally and click through it.

## Result

Verified in Chrome against the real `conveyor-service` and a throwaway Postgres (migrations by foundry, seeded runs/jobs/logs): the front page (runs panel polled, tree, chips, Run now up to the refused request), a project's branch with breadcrumb, the paged history (next/previous), a run (job graph, restart button, live log over `EventSource` with stderr marked and HTML escaped), repositories (add, edit/rename, delete, follow/unfollow), the launch form up to the refused request, scan overview and a check page, credentials (empty list).
Tests: core 26, app 25 screens, web 10, service unit 295 and integration 179 (new reads included, against `conveyor-test-pg`).
Not verified in a browser: restart's request (it would queue a real run), the security/tests report iframe and scanner findings filter (needs warehouse; covered by screen tests), copy button's clipboard, per-project grants with a real gatehouse login, the image build's wasm step.
