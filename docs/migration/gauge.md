# Gauge: the migration checklist

**Where it lives:** `forge/services/gauge` (moved from `forge/docker/gauge-service`; package names unchanged). Gauge is the smallest service, so it goes first and settles the pattern Workbench and the rest follow.

```text
service/   gauge-service: the collector (PCP and node_exporter scrapers), the store, the JSON API (kept as it is), the SSO routes, and the page that starts the browser client
core/      gauge-core: the model the API speaks (Host, View, Point...), the number formatting, the chart geometry: no I/O, builds for the browser
app/       gauge-app: the screens (hosts, one host) over `Request`/`Msg`, like Daybook's
web/       gauge-web: the wasm module: `fetch` to the API with the session cookie, a poll every few seconds
```

No phone or desktop app: a dashboard is read on a screen with a browser. (`app/` stays host-independent, so one could be added.)

## Behaviour to match (read from the htmx pages)

- **Home** (`/ui/home`): one card per host: name, kind (`pcp` / `node_exporter`), core count and URL, an up/down badge, why a host is down and when it was last seen, CPU / memory / swap / disk / GPU meters (a bar and a value, coloured by level), network totals, a CPU sparkline of the history.
- **Host** (`/ui/hosts/{name}`): an overview of stats, CPU (every core), memory, GPU(s), disks (filesystems and devices), network (per interface), temperatures; line charts of the history; a link back.
- The page reloads itself every few seconds while it is visible (`auto_refresh`); the new client polls `GET /api/v1/hosts` and `/hosts/{name}` instead and re-renders in place.
- Five locales (`i18n/*.ftl`, 33 keys): the browser picks one the way gatehouse's account settings say; light and dark follow the system.
- Auth: unchanged. The realm's SSO delegates to gatehouse (`/ui/login`, `/ui/auth/callback`, `/ui/logout`, all kept); `/api/v1` is behind `Auth` (bearer or the `forge_session` cookie). With `SERVICE_AUTH_ENABLED` off, as in development, everyone is an administrator.
- The home card in gatehouse keeps its URL (`/gauge/ui/home`).

## What the stack has, and what Gauge needs

Has: `quench-ui` + `quench-ui-web` (`fetch`, timers, `browser` storage and zone), the kit and `quench-style`, `quench-fullstack`'s page and user (read from the `Auth` claims, a bearer token, or the **session cookie the SSO routes already set**), Daybook's way of serving a wasm module under a service's base path, the image build step for it (`WASM_PACKAGE`, `RESOURCE_DIR = "web"`).

Needs (small, and shared by every later service):
- **`libs/forge-web`**: the page, the assets and the `WEB_DIR` lookup that Daybook's `routers/ui.rs` has, extracted so Gauge, Workbench and the rest do not copy them; plus a page variant that sends a visitor with no session to `/ui/login`.
- **i18n in the app**: `quench-ui` has `i18n`; the five `.ftl` files are read into it (the keys do not change).
- **Charts**: today server-rendered SVG polylines with CSS variables. In the app: a `<img src="data:image/svg+xml,...">` built from the same geometry (core), with the colours given in the SVG (a data-URI image cannot read CSS variables), one per colour scheme.

Not needed, contrary to what I said earlier: a browser OAuth flow. The server-side SSO already does it, and the cookie it sets is what `quench-fullstack` reads.

## Checklist

### Phase 0: the move
- [x] 0.1 `forge/docker/*-service` → `forge/services/<name>/service` (all nine), the Dockerfiles into `images/`, `docker/` gone. Workspace, foreman, conveyor, anvil, forge-sectest and forge-bdd follow; every service's tests pass.

### Phase 1: the shared hosting (`libs/forge-web`)
- [x] 1.1 Extract Daybook's page/asset routes; Daybook uses it (its tests unchanged).
- [x] 1.2 Not needed in the lib: the service keeps its own `ActorOrRedirect` (no `forge_session` → redirect to `{base}/ui/login`) in front of the client page.

### Phase 2: `gauge-core`
- [x] 2.1 `View`, `Point`, `Host`... move out of `service/src/domain` (the API's JSON does not change); `Deserialize` added.
- [x] 2.2 `format` (percentages, bytes, rates, "x ago", the level of a meter) and the chart geometry move over with their tests.

### Phase 3: `gauge-app`
- [x] 3.1 Hosts screen: cards, meters, sparkline, outage text.
- [x] 3.2 Host screen: overview, CPU cores, memory, GPUs, disks, network, temperatures, charts.
- [x] 3.3 Locale and colour scheme; screen tests with sample hosts; offscreen PNGs to look at.

### Phase 4: `gauge-web` and the service
- [x] 4.1 `fetch` + poll; a signed-out answer (401) sends the page to `/ui/login`.
- [x] 4.2 The service serves the client at `/ui/home` and `/ui/hosts/{name}`; the htmx pages and `quench-web` leave the crate.
- [x] 4.3 `.anvil/docker.toml`: `WASM_PACKAGE`, `RESOURCE_DIR = "web"` (as Daybook).
- [x] 4.4 Run it locally (auth off, this machine's PCP and node_exporter if present) and click through it in a browser.

## After Gauge

**Workbench** next: the same pattern, but with forms and writes (projects, issues, comments, labels, boards), so it is where the kit's forms, dialogs and lists get a real workout; its REST API stays and the browser client replaces the htmx pages. Sage (streaming chat) and Switchboard (vLLM views) come last: they need a streamed response body in the browser client first.

## Result

Verified in Chrome against the real `gauge-service` running locally (auth off, this machine's PCP and node_exporter): the home cards with meters and chart, client-side navigation to a host with every section, the language switch (persisted in `localStorage`, kept after a reload), polling every 10 s, and the "no such host" screen. Not verified: the image build's wasm step (needs the registry token), the sign-in redirect against a real gatehouse.
