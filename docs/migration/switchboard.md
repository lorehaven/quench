# Switchboard: the migration checklist

**Where it lives:** `forge/services/switchboard`, Gauge's pattern (see [PATTERN.md](PATTERN.md)), with live polling, filters and two confirm/launch windows. No database of its own to seed (the model store is in-memory; the service only needs the usual `DATABASE_URL`).

```text
service/   switchboard-service: the API (kept; one small read added), the SSO routes, the page that starts the client
core/      switchboard-core: the wire model, the filters' and launch form's drafts, the fit arithmetic: no I/O, builds for the browser
app/       switchboard-app: home, models registry and vLLM screens over `Request`/`Msg`
web/       switchboard-web: the wasm module: `fetch` (GET, POST, DELETE) with the session cookie
```

## Behaviour to match (read from the htmx pages)

- **Home** (`/ui/home`): a card per switched-on screen (models registry, vLLM management; `FEATURE_MODELS_DASHBOARD_ENABLED`, `FEATURE_VLLM_MANAGEMENT_ENABLED`), "No services are currently enabled." when neither. The nav items are the same two.
- **GPU box** (both screens): `GPU: <name>`, `Total: N GB`, `Free: N GB`, `n/a` without a card. The old page took it from a server-sent stream every second; the client polls `GET /api/v1/gpu/status` every second.
- **Models** (`/ui/models/dashboard`): filters (source HF/GGUF, sort name/params/vram asc/desc, search, quantisation by source, context, "vLLM only") that re-query; a card per model (vLLM badge, name, params/quant/context/layers/hidden, the fit line: `Fits: YES | Best | VRAM | Margin` coloured ok/tight(<=2 GB), `Fits: NO | Minimum`, `No estimates`; path); the fit line opens the **estimates window** (fit / context / quant filters over the what-if rows); the delete button (only with `delete-model`) opens a **confirm window**, deleting refreshes the list.
- **vLLM** (`/ui/vllm/manage`): GPU box, **Launch** button (only with `launch`), a card per instance (model, GPU/CPU badge, stop button while stoppable and with `stop`, id/namespace/endpoint/status/started, quant/dtype/multimodal limit/GPU util or Device: CPU, error and log path), "No running instances"; instances polled every 2 s (the old stream). Stop opens a **confirm window**.
- **Launch window**: model (vLLM-capable ones), host, port, namespace (hidden for the native engine), quantisation, dtype, device (GPU/CPU), max model len, multimodal limit, GPU utilisation (disabled on CPU), prefix caching, task, tool calling; a fit note (`Should fit` / `Tight fit` / `Won't fit right now` / `Won't fit: ... utilization allows only` / select a model / no estimate / CPU) that follows every edit; choosing model, quantisation, length or device sizes the utilisation to the least that holds the model (0.05 steps, 0.20..1.0); Launch is disabled when it cannot fit.
- Refusals (403, 500 with `api_error_*` keys) become a message; a 401 offers to sign in; five locales shared with the old pages.
- Auth unchanged: `/ui/login`, `/ui/auth/callback`, `/ui/logout`, `/ui/status`, `/ui/refresh` stay; `/api/v1/{gpu,models,vllm}` are behind `Auth`, the per-action checks (`launch`, `stop`, `delete-model`) are the API's.

## API: what changed

Kept unchanged (other services and apps call them): `GET /api/v1/gpu/status`, `POST /api/v1/models/list`, `POST /api/v1/models/delete`, `POST /api/v1/models/delete-form`, `GET /api/v1/models/running`, `GET /api/v1/vllm/{list,instances}`, `POST /api/v1/vllm/instances`, `POST /api/v1/vllm/launch` (form), `DELETE /api/v1/vllm/instances/{id}`.

- Added `GET /api/v1/vllm/capabilities` -> `{models_dashboard, vllm_management, can_launch, can_stop, can_delete_model, namespace}` (the pages decided these on the server, per request).
- `POST /api/v1/models/list` accepts `"source": "all"` (every source: the launch window lists vLLM-capable models of both).
- Removed (HTML fragments for htmx, no JSON consumers): `GET /api/v1/models/{grid,estimates-modal,estimates-modal/empty,delete-modal,delete-modal/empty}`, `GET /api/v1/vllm/{grid,launch-modal,launch-modal/empty,stop-modal,stop-modal/empty}`, the two HTML server-sent streams `GET /api/v1/gpu/status/sse`, `GET /api/v1/vllm/{sse,instances/sse}`. Their logic (fit lines, estimates, launch fit note, utilisation sizing) moved to `switchboard-core::fit` with tests.

## Checklist

### Phase 1: the API
- [x] 1.1 `capabilities` and `source: all` with tests; HTML fragments and streams removed with their tests.

### Phase 2: `switchboard-core`
- [x] 2.1 Wire model (GPU, capabilities, model/estimates, instance, bodies).
- [x] 2.2 Fit arithmetic (card fit, estimate rows, launch estimate, utilisation sizing, launch note).
- [x] 2.3 Drafts: filters, launch form.

### Phase 3: `switchboard-app`
- [x] 3.1 Shell: top bar, language, sidebar, banner (signed out / failed / refused).
- [x] 3.2 Home; models registry with filters, estimates and delete windows; vLLM screen with launch and stop windows.
- [x] 3.3 Polling (GPU 1 s, instances 2 s), stale answers ignored.
- [x] 3.4 Screen tests.

### Phase 4: `switchboard-web` and the service
- [x] 4.1 `fetch` for every request; answers become messages.
- [x] 4.2 The service serves the client at `/ui/home`, `/ui/models/dashboard`, `/ui/vllm/manage` (and trailing-slash forms) behind `ActorOrRedirect`; htmx pages, `forge-ui`, `quench-web`, `quench-web-components` leave the crate's UI.
- [x] 4.3 `.anvil/docker.toml`: `RESOURCE_DIR = "web", WASM_PACKAGE = "switchboard-web"` (kept `RUN_AS`); `web/.gitkeep`; `.gitignore`.
- [x] 4.4 Run locally and click through it (see Result).
