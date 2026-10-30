# Gantry: the migration checklist

**Where it lives:** `forge/services/gantry`, Workbench's pattern (see [workbench.md](workbench.md)): actions instead of forms.

```text
service/   gantry-service: the API (kept; small reads extended), the SSO routes, the page that starts the client
core/      gantry-core: the model the API speaks, the filters, the YAML draft, request bodies: no I/O, builds for the browser
app/       gantry-app: packages, package, target (versions), operations, operation, plan, YAML editor
web/       gantry-web: the wasm module: `fetch` with the session cookie
```

## Behaviour to match (read from the htmx pages)

- **Packages** (`/ui/home`): a card per package (name, sync badge, installed `→` newer, description, resource count, notes for missing/edited/extra/not ready); chips per sync state with counts and a name filter; those needing attention first; a banner naming the change in progress (link to it); cluster/registry errors shown, not hidden.
- **Package** (`/ui/apps/{name}`): header (sync, versions), Upgrade (confirm, `deploy`), Apply missing (`scale`), Refresh (no inventory), Versions link; counts that filter; declared deployments with Start / Stop (stop asks first, `scale`); filter (name, kind, state, clear, `n / total`); resource table with Apply (missing), Edit (editable, `deploy`), Delete (asks first, `scale`).
- **Target** (`/ui/targets/{name}`): status, installed `→` offered, version picker (newest default, yanked disabled) + Sync, running units, the published versions.
- **Operations** (`/ui/operations`): history table. **Operation** (`/ui/operations/{id}`): title, state, who/when, error, steps, log (live while running), Cancel (`deploy`), re-read every 3 s until over.
- **Plan** (`/ui/plans/{id}`): summary, steps, Run (when `can_confirm`), link to the operation once run, "not allowed" text.
- **Editor** (`/ui/resource?package&kind&name&namespace&api_version`): live YAML, read-only without `deploy`, Apply; a refusal keeps the text and says why.
- Sidebar: Packages, History. Five locales, light/dark follow the person.
- Old redirects with `?error=` become a notice on the screen; the old browser `confirm()` becomes an in-page question naming the thing.
- Start/Stop/Sync ran at once (no plan page): the client plans (`POST .../plan`) and confirms (`POST /plans/{id}/confirm`) in one go, so permissions are checked as before.

## What the API lacked

- `GET /resources`: each package now carries `summary` and `can` (read/deploy/scale held for it).
- `GET /resources/yaml`: `editable`.
- `GET /operations/{id}` and `GET /plans/{id}`: `step_lines` (steps in words); plan also `can_confirm`.
All with tests in `service/tests/unit`.

## Checklist
- [x] API additions + tests
- [x] `gantry-core` (model, filters, draft) + tests
- [x] `gantry-app` screens + 30 screen tests (polling with a manual clock)
- [x] `gantry-web` client + 16 stand-in tests
- [x] service rewired (forge-ui, quench-web out; forge-web in), `client_tests.rs`, old UI tests replaced by API/client-contract tests
- [x] `.anvil/docker.toml` build args, `web/.gitkeep`, `.gitignore`, icon
- [x] Browser run against `GANTRY_CLUSTER=none GANTRY_RUNNER=simulate` with `local/seed.json` and `local/packages`

## Result
Verified in Chrome: packages, filters, package page, start (swap) with live log, upgrade and stop confirmations, apply/delete/cancel, sync from the target page, YAML edit (saved, blank refused, wrong resource refused with the reason), plan page confirm, not-found, language switch.
Not verified: a real cluster/registry (errors are covered by unit tests only), a scoped-grant login (needs gatehouse), the image build.
