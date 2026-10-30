# Gatehouse: the migration checklist

**Where it lives:** `forge/services/gatehouse`, the [pattern](PATTERN.md) with one difference: gatehouse is the sign-in everybody else redirects to, so its pages open **without** a session, and its writes stay form posts.

```text
service/   gatehouse-service: every API and SSO route (kept byte for byte), the page API (new), the pages that start the client
core/      gatehouse-core: the page API's model, the notices a redirect carries back, the MFA draft
app/       gatehouse-app: sign-in and the public pages, the dashboard, the account, the user administration
web/       gatehouse-web: the wasm module: `fetch` with the session cookie
```

## Design

- **Writes stay what they were.** The old pages posted urlencoded/multipart forms to `/ui/...` and the service answered with a redirect (and, for sign-in, the `forge_session`/refresh cookies). The client draws the same `<form method=post action=...>`; the browser posts it, the service answers as before, and the page that comes back shows the `?ok=`, `?err=` or `?registered=1` of its query (only keys from fixed lists in `gatehouse-core::notice`). So sign-in, the redirect target (`?redirect=`, validated as before), rate-limit and lockout messages, cookies, avatar upload (multipart), password and address change, notifications and all admin actions work exactly as before and needed no programmatic navigation (no `forge-shell` change).
- **Reads are JSON**, under `/api/v1/ui/...` (new, `service/src/api/pages.rs`, same session: cookie or bearer; with `SERVICE_AUTH_ENABLED=false` the pages' stand-in `admin`): `dashboard`, `account`, `mfa-enroll` (GET a fresh secret + QR, POST the first code: the one write the client makes itself, so a wrong code keeps the secret and the secret never travels in a URL), `admin/users`, `admin/users/{name}` (the catalog action `read-users` as before), and two public ones where the emailed link is the credential: `confirm-email?token=`, `unsubscribe?token=`.
- `POST /ui/unsubscribe` now answers JSON (`{"done":true}` / `{"error":key}` with 200/404/500) instead of an HTML page; a mail provider's one-click POST only needs the 2xx. `POST /ui/account/mfa/enroll` is replaced by the API above.
- **Public pages** (no `ActorOrRedirect`): `/ui/login` (a good refresh cookie still renews the session server side and redirects), `/ui/login/mfa`, `/ui/register`, `/ui/forgot-password`, `/ui/reset-password`, `/ui/resend-verification`, `/ui/accept-invite`, `/ui/confirm-email` (an invalid link still redirects to the login error), `/ui/unsubscribe`. **Signed in:** `/ui/home`, `/ui/account`, `/ui/account/mfa/enroll`, `/ui/admin/users[/{name}]`; `/ui/account/avatar`, `/ui/logout`, `/ui/status`, `/ui/refresh` untouched.
- The dashboard keeps every card (catalog order, `<NAME>_UI_URL`/`_URL`, `FEATURE_<NAME>_ENABLED`), its translated title/description (else the service's own words), the realm section for an administrator.

## Checklist
- [x] page API + tests (`api_pages_tests`, notice keys cross-checked with `RealmError`)
- [x] `gatehouse-core` + tests
- [x] `gatehouse-app`: 30 screen tests (every screen in five languages, no raw keys)
- [x] `gatehouse-web`: stand-in transport tests
- [x] service rewired (forge-ui, quench-web out), old render tests replaced by API/client-contract tests (`ui_client_tests`)
- [x] `.anvil/docker.toml` build args, `web/.gitkeep`, `.gitignore`, icon, i18n keys (five locales)
- [x] Chrome run with auth enabled

## Result
Verified in Chrome (auth on, BASE_PATH=/gatehouse, Postgres + Redis db 9): signed-out redirect to login, wrong password (`?err=1` notice), real sign-in with `?redirect=` (cookies set, back on the account page), dashboard cards, admin create user and grant change, forbidden page for a non-admin, MFA enrol with a wrong code, language switch in place, logout (cookies cleared, API 401).
Not verified: real SMTP links, TOTP success, avatar upload in the browser (server multipart tests unchanged), lockout in the browser (server tests unchanged), the image build.
