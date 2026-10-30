# Warehouse: the migration checklist

**Where it lives:** `forge/services/warehouse`, Workbench's pattern (see [workbench.md](workbench.md), [PATTERN.md](PATTERN.md)). Warehouse is also the cargo registry, docker registry, file store and artifact/rivet store that other tools talk to: **only the human pages under `/ui` were replaced**; `/v2`, `/token`, `/index`, `/api/v1/*`, `/admin/*` are untouched.

```text
service/   warehouse-service: the registry routes (kept), the SSO routes, the page that starts the client, the new /ui/api reads and changes
core/      warehouse-core: the model of /ui/api, the drafts (storage forms), the trees (repositories, files, artifacts, rivets): no I/O
app/       warehouse-app: home, docker, crates, files (storages + browser), artifacts, rivets over `Request`/`Msg`
web/       warehouse-web: the wasm module: `fetch` with the session cookie
```

## Behaviour to match (read from the htmx pages)

The old UI had no upload, search, token or pagination screens: it browsed and managed. Everything it did stays.

- **Home** (`/ui/home`): a card per enabled registry (docker, crates, files, artifacts, rivets) or "no services".
- **Docker** (`/ui/docker/catalog?repo=&tag=`): repositories as a tree split at `/`; the open repository lists its tags (newest first); a tag shows digest, media type, manifest size ("unknown" when absent); "Delete image" asks to confirm, removes the manifest and every tag that points at it, and lands on the repository. `/ui/docker/tags/{repo}` still redirects.
- **Crates** (`/ui/crates/catalog?repo=&tag=`): crates; the open crate's versions newest first (yanked marked); the open version (default: newest not yanked) shows status, checksum, rust version, links, features, dependencies grouped normal/build/dev; yank / unyank (any session, as before).
- **Files / storages** (`/ui/files/storages?storage=`): configured (static, with a badge) and database storages (with owner); a static one shows its root; a database one shows owner, quota bar, max file size, sync, created; with the write grant: edit form (quota GiB, max file MiB, reset max file, sync), "New storage" form (name, owner, quota, max file, sync), "Delete storage" with confirmation (removes its files). Link to the browser.
- **Files / browser** (`/ui/files/browse?storage=&path=`): tree of directories, root crumb, breadcrumbs, "Up", entries (directories first) with sizes; a file shows name, path, type, size, an in-place preview (image, video, audio, pdf/text in a frame, or "no preview"), Download, and (write grant) Delete. A database storage needs read on it (owner, wildcard, `warehouse:storage:<n>:read`), a static one any session. Capped at 50 000 files ("truncated" note).
- **Artifacts** (`/ui/artifacts/catalog?repo=&platform=&tag=`): program, platform, builds (newest first); details incl. SDKs, permissions; yank / unyank with the write grant.
- **Rivets** (`/ui/rivets/catalog?repo=&tag=`): packages and versions (semver, newest first); details incl. requirements, `[meta]`, install line; yank / unyank with the write grant.
- Errors the pages showed (`api_error_*`) are shown translated; five locales (`service/i18n/*.ftl`, six keys added); light/dark/palette follow the person's choice.
- Auth unchanged: SSO routes stay; the pages need a session; changes keep their gates.

## What the API lacked: `/ui/api` (session cookie; 401 without one, 403 `api_error_forbidden` without `warehouse:write` where the old page needed it)

`GET overview`, `docker/repositories`, `docker/tags?repo=`, `crates`, `crates/{n}/versions`, `artifacts`, `rivets`, `files/storages`, `files/browse?storage=`; `POST docker/delete-image`, `crates/(un)yank`, `artifacts/(un)yank`, `rivets/(un)yank`, `files/storages`, `files/storages/{n}/edit`, `files/storages/{n}/delete`, `files/delete-file`. Downloads and previews use the existing `/api/v1/files/{storage}/file`.

Fixed on the way: deleting a docker image stopped looking at tags at the first tag of another image; it now removes every tag of the deleted manifest.

## Checklist

- [x] 1. Service: `/ui/api` (7 modules), client page routes, `forge-ui`/`quench-web` removed, old page tests replaced (API unit tests + a database-backed one).
- [x] 2. Core: model, drafts, trees, formatting; unit tests.
- [x] 3. App: shell, home, docker, crates, storages, browser, artifacts, rivets, confirm dialog, language redraw; screen tests.
- [x] 4. Web: `Client::execute` over a `Transport`; stand-in tests.
- [x] 5. `.anvil/docker.toml` (`WASM_PACKAGE = "warehouse-web"`), `web/.gitkeep`, `.gitignore`, `icon.png`.
- [x] 6. Browser run against the local service and a throwaway Postgres.

## Result

Verified in Chrome (plain HTTP, `SERVER_CERT_PATH` pointed at nothing): docker tree, tag, delete with confirmation; crates versions, deps, yank/unyank; artifacts and rivets browse and yank/unyank; storages create, edit, delete; file browser (tree, listing, image preview, text frame, no-preview, download, delete with confirmation); language switch; light theme. Not verified: real sessions (auth off), the write-grant split (covered by unit tests), a denied storage in a browser, video/audio/pdf previews, the image build's wasm step.
