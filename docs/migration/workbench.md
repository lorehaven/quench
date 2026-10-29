# Workbench: the migration checklist

**Where it lives:** `forge/services/workbench`, Gauge's pattern (see [gauge.md](gauge.md)) with forms and writes.

```text
service/   workbench-service: the REST API (kept; three small reads added), the SSO routes, the page that starts the client
core/      workbench-core: the model the API speaks, the forms' rules (drafts), the board grouping, issue keys: no I/O, builds for the browser
app/       workbench-app: home, board, issue screens over `Request`/`Msg`
web/       workbench-web: the wasm module: `fetch` (GET, POST, PUT, DELETE) with the session cookie
```

## Behaviour to match (read from the htmx pages)

- **Home** (`/ui/home`): a card per project (`KEY — name`, description) linking to its board; "New project" (key ≤ 10 chars, name, description) that opens the new board; empty state.
- **Board** (`/ui/projects/{id}/board`): one column per status (blocked, todo, in-progress, done, rejected); a card per issue (`#seq`, title link, kind and priority badges, estimate, a status `<select>` that moves it); "New issue" (title, kind, priority, assignee).
  Dragging a card to a column works through `forge-shell`'s script (native drag and drop sets the card's select and fires its input event), since `quench-ui` has no drag events.
- **Issue** (`/ui/issues/{id}`): breadcrumbs, details form (title, description, status, kind, priority, assignee with "assign to me", estimate; a changed status is a transition), dependencies (blocks, blocked by, relates to; remove; add by key `WB-4` and type), comments (list, add).
- **Sidebar**: the projects under "Projects", the current one marked.
- Errors the pages showed as `?error=code` become a message on the screen.
- Five locales (`i18n/*.ftl`, shared with the old pages), light and dark follow the system.
- Auth unchanged: `/ui/login`, `/ui/auth/callback`, `/ui/logout` stay; `/api/v1` is behind `Auth`; per-project grants (`workbench:project:<id>:read|write`) are enforced by the API as they are today. A 403 on a write says so on the screen.

## What the API lacks for a client

The pages read three things straight from the database. Added to `/api/v1` (behind the same `Auth`):
- `GET /api/v1/me` → `{ "username": ... }` ("Assign to me", "reporter").
- `GET /api/v1/users` → `[{ "username", "display_name" }]` (the assignee picker).
- `GET /api/v1/issues?key=WB-4` → the issue, or 404 (adding a link by key).

## Checklist

### Phase 1: the API
- [x] 1.1 `me`, `users`, `issues?key=` with integration tests (they need a database).

### Phase 2: `workbench-core`
- [x] 2.1 The model (timestamps as RFC 3339 text), request bodies, statuses/kinds/priorities/link kinds.
- [x] 2.2 Drafts: project, issue (estimate parsing), comment; `split_issue_key`; board grouping and the optimistic move.

### Phase 3: `workbench-app`
- [x] 3.1 Shell: top bar, language, sidebar of projects, banner (signed out / failed / rejected).
- [x] 3.2 Home and new project.
- [x] 3.3 Board and new issue; moving a card (optimistic, undone when refused).
- [x] 3.4 Issue: details, dependencies, comments.
- [x] 3.5 Screen tests with sample data.

### Phase 4: `workbench-web` and the service
- [x] 4.1 `fetch` for every request; the answers become messages (401 signed out, 403, 404, 409, 400 with the API's text).
- [x] 4.2 The service serves the client at `/ui/home`, `/ui/projects/{id}/board`, `/ui/issues/{id}` behind `ActorOrRedirect`; the htmx pages, `forge-ui` and `quench-web` leave the crate.
- [x] 4.3 `.anvil/docker.toml`: `WASM_PACKAGE = "workbench-web"`, `RESOURCE_DIR = "web"`; `web/.gitkeep`.
- [x] 4.4 Run locally and click through it (needs a database for the service: see how the integration tests start one).

## Result

Verified in Chrome against the real `workbench-service` and a throwaway Postgres (migrations applied by foundry): creating a project opens its board, the sidebar marks it, a new issue appears as a card, moving a card persisted (`in-progress`), saving the details (estimate) and adding a comment round-tripped through the API. The service's integration suite (32 tests, including the three new routes) passes against that database.

Not verified: the add-link and remove-link flows in a browser (client and screens are tested, the API is covered by integration tests), the language switch (same code as Gauge), per-project grants with a real gatehouse login, the image build's wasm step. Dragging a card to a column is done in `forge-shell`'s script (HTML5 drag and drop sets the card's status select and fires its input event).
