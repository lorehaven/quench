# Network and data crates: gap report

---

## Area: http

## Gap report: quench-http, quench-http-macros

Docs written: `docs/crates/quench-http/` (index, usage, api, api-request-response, api-extractors, api-routing-server, api-di, configuration, errors, internals, testing) and `docs/crates/quench-http-macros/` (index, api, errors, internals, testing).

Old page `docs/quench-http.md` still exists: the sandbox denied deleting it. It is fully covered and should be removed by the caller. Links to it remain in `docs/quench-auth.md`, `docs/quench-fullstack.md`, `docs/quench-starter.md`, `docs/quench-proto.md` (other authors' pages; the anchors `#routing` and `#middleware` are used). Fixed: `quench-http/README.md`, `README.md`, `docs/README.md`, `docs/examples/http_demo.md`.

### 1. Stale or wrong statements in the old docs

* "`ExtractError` (... a missing DI component) converts into one automatically via `?`, always as `400`": an `Inject<T>` failure is a `500` (`extract.rs` `Inject::from_request`), only `ExtractError` is 400. `BodyTooLarge` is also 400, not 413 (tests assert 400).
* "Scoping middleware": the old page shows a hand-written `OnPathPrefix { prefix, inner }`; the crate ships `middleware::OnPathPrefix` with private fields and `new(prefix, inner)`.
* Old text says handlers' `Extension` etc. are all "in the prelude" via "Everything public is reachable through `prelude::*`": false. Not in prelude: `WebSocketUpgrade` and all of `ws`, `InboundBody`, `DEFAULT_BODY_LIMIT`, `DEFAULT_HEADER_READ_TIMEOUT`, `stream::*`, `IntoHttpError`, `ROUTES`, `COMPONENTS`.
* Old text: "Both [serve fns] run until SIGTERM/SIGINT"; only `serve_http`/`serve_https` do; the `_on` variants use a caller future.
* Old text lists `IntoResponse` impls without noting that `ExtractError`/`DiError` have none.
* Workspace `Cargo.toml` declares `quench-http` with `default-features = false`, so the documented "tls (default)" does not apply to workspace members unless they enable it.
* Old text calls the mount/path-prefix behaviour general ("one inside it is dispatched with the prefix stripped"); it is wrong for nested mounts and trailing-slash prefixes (see 3).
* Old page refers to "BodyLimit (#dependency-injection)" anchor and "Quench Proto" cookie helper `cookie_value`; not verified here (proto is another crate).

### 2. Existed in code, undocumented in the old page

* `IntoHttpError` trait (dead), `ExtractError::MissingPathParam` / `MissingState` (never constructed), `DiError::not_found`.
* `DEFAULT_HEADER_READ_TIMEOUT` (30 s) and `DEFAULT_BODY_LIMIT` as public constants; `Form` extractor (old page's table omitted it); `Response::html`, `Response::from_bytes`, `Response::not_found`, `Response::ok`, `Response::headers`, `Request::path_param`, `header`, `skip_prefix`, `container`.
* `ComponentFactory` fields, `COMPONENTS`, `BoxedAny`, `ConstructFuture`, `ROUTES`, `RouterBuilder::route`/`finish`, `Router::dispatch`.
* `ws::upgrade_with`, `WebSocketUpgrade::config`, `stream::receiver_stream`/`reader_stream` (partly covered).
* `Path<T>` deserialisation rules (percent-decoding once, shape probing), 405 without `Allow`, HEAD not derived from GET.
* Connection limits inherited from `quench-proto` `ServerConfig::default()`.

### 3. Open questions and suspected bugs (verified with a scratch probe unless noted)

1. `quench-http/src/router.rs:77-92` (`Mount::call`): matches against `req.uri().path()` (full path) not the routed path. `mount("/a", mount("/b", r))` returns 404 for `/a/b/x` (probe confirmed). Should use `routed_path()`.
2. `router.rs:83-86`: a prefix with trailing slash (`mount("/api/", r)`) makes `/api/x` 404, because the remainder `x` does not start with `/` (probe confirmed). Probably should normalise the prefix.
3. `di/mod.rs:174-202` (`build`): two `#[injectable]` functions returning the same type collapse in `by_id`, `constructed (1) != factories.len() (2)`, so `build()` fails with `DiError::Cycle([])` ("dependency cycle detected among: " with no names). Probe confirmed. Misleading; should be a duplicate-component error.
4. `di/mod.rs`: an `#[injectable]` whose type was also `provide`d is still constructed and overwrites the provided value (provided values only satisfy dependencies). May be intended (test overrides do not work then).
5. `error.rs:50-58`: all `ExtractError`, including `BodyTooLarge`, map to 400; 413 would be conventional. Tests assert 400.
6. `router.rs:44-57`: `HEAD` (and `OPTIONS`) on a `GET` route answers 405 and the 405 has no `Allow` header (probe confirmed for HEAD).
7. `middleware.rs:55-62`: `OnPathPrefix` tests `uri().path()` (original path, not mount-stripped) and is a raw string prefix (`/api` matches `/apix`).
8. `error.rs:33`: `IntoHttpError` is public and unused anywhere in the workspace.
9. `server.rs` `serve_loop`: if the SIGTERM handler cannot be installed only Ctrl+C works (silently). The 25 s and 30 s timeouts are constants, not configurable.
10. `extract.rs:169-175`: `Extension<T>` missing is 500 `HttpError::Status`; `Inject<T>` likewise (intentional per tests, but a server-side wiring error shares the code of a client error elsewhere).
11. `quench-http-macros/src/expand.rs`: `#[injectable(args)]` silently ignores its arguments; route patterns are not validated at compile time (panic at `discover_routes()`); the generated struct name `__quench_route_<fn>` is unhygienic; generic handlers unverified; `arc_inner`/`result_inner` match any type named `Arc`/`Result` by last path segment.
12. `response.rs:header`: invalid header names/values are silently dropped (documented as intended in the doc comment, but a typo in a header name disappears without trace).
13. Doc comments in `quench-http/src/extract.rs:186` (`Limited`) show the pattern `Limited::<Json<Report>, N>(Json(report)): Limited<...>`; the shorter `Limited(Json(r)): Limited<Json<R>, N>` is what old docs and I use; both compile in principle (not compile-checked here).
14. Not verified here: details of `quench_proto` behaviour (WebSocket, SSE headers, `Expect: 100-continue`, smuggling rejection); documented only as references to that crate.

---

## Area: web

## Report: quench-web, quench-web-components, quench-client, quench-tls

Pages: `docs/crates/{quench-web,quench-web-components,quench-client,quench-tls}/`. Old flat pages deleted: `docs/quench-{web,web-components,client,tls}.md`. Links fixed in `README.md`, `docs/README.md` (link lines only), `docs/quench-proto.md`, the three crate READMEs (`quench-tls` has no README). Left for the coordinator: the tree listing in `docs/README.md` lines 21, 49, 50 still names the deleted files. `docs/quench-redis.md` / `quench-pg.md` / `quench-auth.md` etc. no longer referenced the old pages when I looked (already changed by others, or they use other link forms): re-grep at the end.

nav_order values I used: quench-client 10, quench-tls 20, quench-web 30, quench-web-components 31 (arbitrary; adjust to the global scheme).

### 1. Stale or wrong statements in the old docs

quench-web.md
* "`available_locales()` scans an `i18n/*.ftl` directory next to the running binary": it is `./i18n`, relative to the process working directory (`quench-style/src/i18n.rs`, `I18N_DIR`).
* "`create_asset_files(default_theme, resources_prefix)` ... write ... to disk" implies both arguments matter. `create_asset_files_with_options` ignores `default_theme` and `resources_prefix` (parameters are `_default_theme`, `_resources_prefix`, `quench-style/src/assets.rs`); output is always `dist/assets` in the working directory.
* Dependents claim ("`quench-starter`, `quench-web-components`, switchboard/conveyor/sage/warehouse/gatehouse services, `examples/basic`"): in this repo only `quench-starter` and `quench-web-components` depend on it; the rest cannot be verified here.
* "Reachable via `quench_web::prelude::*`: ... `HeaderBuilder`, `FooterBuilder`, `NavPanelBuilder`, `nav_button()`": the prelude has only `locale_switch`, `AppBuilder`, `AppShell`, `AppShellBuilder`, `Theme`, `html::*`, `js`, `CssRule`.
* "(`framework::app::i18n`)" module path for the i18n functions does not exist; they are glob re-exports in `framework::app`.
* Test comment in `tests/unit/framework_app_shell_tests.rs` says `Element` attributes are a `HashMap`; they are a `BTreeMap` (sorted).
* Says `PageBuilder` assembles "a full `<html>` document"; the output has no `<!DOCTYPE html>` (see 3).

quench-web-components.md
* "used by the forge services conveyor, gatehouse, switchboard, warehouse": not verifiable in this repo.
* "`quench_web::dom`": exists only as `quench_web::dom` through the root glob; real path is `quench_web::framework::dom`.
* Did not mention that CSS for the widgets does not exist (see 3).

quench-client.md
* "based on the single source file `libs/quench-client/src/lib.rs`": the crate has `lib.rs`, `transport.rs`, `blocking.rs`, `multipart.rs` and lives at `quench-client/`.
* "the blocking client has all of the above too": it lacks `max_response_bytes`, `send_streaming`, `bytes_stream`.
* "Non-2xx responses become an `anyhow::Error`": the error type is `quench_core::Error` (no `anyhow`).
* `ClientError` is described as "defined but ... surface errors as `anyhow::Error`": correct that it is unused, but the replacement type is `quench_core::Error`.
* Crate description in Cargo.toml says "with auth and retry support": only the transport has one automatic retry (dead pooled connection, idempotent methods); the typed clients do not retry.
* "`BearerAuthClient` ... `get`/`post`" correct; nothing said about the missing builder/put/delete.
* Cargo.toml comment for `blocking` says "a private tokio runtime per client": clones share one runtime (`Arc<Runtime>`).

quench-tls.md
* "(`ClientOptions`) trust anchors from webpki-roots, the OS bundle ..." fine, but it did not state that `ClientOptions::default()` has `use_system_roots: false`.
* Fixtures README says the material is for "the `quench-proto` TLS tests"; the fixtures are now also used by `quench-tls` and `quench-client` tests.

### 2. Existing in code, undocumented in the old pages

quench-web
* The exact `AppBuilder::build` head/body output (CDN Font Awesome 6.7.2, htmx 2.0.10, htmx-ext-sse 2.2.4 loaded from CDNs; preload links for non-default themes; scripts order).
* `session_script` / `locale_script` / `theme_script` (pub fns): the session watcher expects `GET {prefix}/status` -> `{"authenticated": bool}`, `POST {prefix}/refresh`, and `{prefix}/login`; 60 s interval constant; cookies `qlocale` / `qtheme`; browser globals (`qT`, `setLocale`, `getLocale`, `updateTheme`, `qUpdateI18n`, events `localeChanged` / `themeChanged`); `data-i18n`, `data-i18n-placeholder`, `data-i18n-title`, `data-i18n-args`.
* `AppShellBuilder::with_header`, `try_build` (returned error `missing locale file: ...`), `header`, `footer`, `links`, `scripts`, `header_label`, `footer_label`; `AppShellBuilder::build` side effect of writing `dist/assets`.
* `Element::raw/defer`, `Script` ordering/`defer` defaults (`new` defers, `inline` does not), `head_link_static_attr(s)`, `pretty_print_html` public.
* Translation keys the shell requires: `header_label`, `footer_label`, `locale_label`, `theme_label`.
* Full re-export map incl. `Color`, `Palette`, `GRAY_STEPS`, `SUCCESS_STEPS`, `ThemeSpec`, `write_assets`, `FAVICON`, `*_in` variants, `Theme::ALL/palette`.

quench-web-components
* `empty_state` exact output; `StatusBadge` icon only with a level; `AlertBox` class names `alert-status-*`; `FormSelect::options` replaces; `Default` vs `new` differences; `prelude` contents.

quench-client
* `transport` module in full: `Redirect`, `ClientBuilder::{user_agent, redirect, max_response_bytes, client_identity_pem}`, `RequestBuilder::{query, headers, cookies, multipart, send_streaming}`, `Response::{cookies, cookie, bytes_stream}`, `transport::Error` variants and texts, `multipart::{Form, Part}` body layout, constants (pool 8 / 60 s, redirect drain 64 KiB).
* Error message strings of the typed clients.
* Token cache details (30 s margin, no refresh on 401, lock not held across the exchange).

quench-tls
* `ClientOptions` fields and defaults, the system bundle search order and `SSL_CERT_FILE`, `AcceptAnyCert` behaviour (no roots loaded), stream EOF/`close_notify` semantics, `root_store`, `pem_*`, `server_name` bracket handling, `TlsStream::{alpn_protocol, get_ref}`, no server-side client-auth helper.

### 3. Open questions and suspected bugs

quench-web
1. `quench-web/src/html/builder/page.rs` (`PageBuilder::build`, `format!` with `<!DOCTYPE html>`) followed by `pretty_print_html`: the pretty printer drops `<!...>` declarations (`html/builder/pretty.rs`, `parse`, branch `rest.starts_with("<!")`), so every generated page has no doctype (confirmed in `tests/fixtures/baseline/pages/basic.html`, which starts with `<html>`). Browsers render such pages in quirks mode. The golden files lock this in.
2. `page.rs` `PageBuilder::build`: title is interpolated unescaped (`<title>{}</title>`); a title containing `</title><script>` is parsed by the pretty printer as markup and injected. `render_head_link` and `Script::render` also write `href` / `src` / `crossorigin` unescaped. Safe only with trusted values.
3. `page.rs` `PageBuilder::build`: `self.content.unwrap()` panics without content; an `Err`/default would be friendlier (tested as a panic: `build_panics_without_content`).
4. `html/builder/elements.rs` `content()` creates a non-standard `<content>` element.
5. `html/builder/pretty.rs` `write_element`: void elements are written with a closing tag (`<meta ...>\n</meta>`), valueless attributes as `defer=""`; invalid HTML5 style though browsers tolerate it. Text nodes are trimmed, which can drop spaces between inline text and inline elements.
6. `framework/app/shell.rs` `try_build` -> `create_asset_files_with_options`: writes into `./dist/assets` on every shell build (a side effect in a "build" call); failures are only printed to stderr. `resources_prefix` is ignored for writing, which is consistent only if the service serves `dist/assets` under the prefix.
7. `quench-style/src/i18n.rs` `generate_translations_js`: `unwrap_or_default()` on a parse error yields `window.qTranslations = ;` (JS syntax error in the browser, all translations lost) instead of failing.
8. `framework/components/nav.rs` `NavPanelBuilder::select_theme` / `build_locale_select`: `.attr("value", ..)` on a `<select>` has no effect in HTML (selection is done by script); `select_theme` clones `element` per option (`element.clone().child(..)`), harmless but quadratic. Theme option text is the raw theme id (not localized).
9. `framework/app/scripts.rs`: locale and theme watchers poll the cookie every 500 ms for the lifetime of the page; `theme_script` requires `getCookie` / `setCookie` from `locale_script` (silent ordering dependency).
10. `framework/app/builder.rs`: `AppBuilder::new()` and `AppBuilder::default()` differ (`supported_themes` all vs empty); `AppBuilder::default_theme` is not checked against `supported_themes` (only `AppShellBuilder` does).
11. `framework/components/header.rs`: `HeaderBuilder::build` lacks `#[must_use]` (others have it); `Script::inline` likewise.
12. `framework/dom/modal.rs` `toggle_modal`: `overlay.length === 1` is required, panels only `> 0`; declares `const overlay` / `const panel` in inline handler scope. DOM helper arguments are interpolated unescaped.
13. Session watcher URLs are `{resources_prefix}/status|refresh|login`; the `resources_prefix` doubles as both the asset prefix and the UI base path. A service that wants assets and auth endpoints under different prefixes cannot express it.

quench-web-components
14. `status/mod.rs` and `containers/mod.rs`: the CSS classes `btn*`, `card*`, `panel*`, `container*`, `form-group`, `status-*`, `alert*` are not defined in the shared stylesheet generated by `quench-style` (only `.empty` is; checked in `quench-web/dist/assets/css/style.css` and `quench-style/src/styles`). The widgets render unstyled. Either the stylesheet is missing rules or services are expected to bring their own.
15. `status/mod.rs` `AlertBox::build`: `closeable` renders a close icon with no handler anywhere in the codebase.
16. `status/mod.rs` `StatusBadge::build`: `with_icon(true)` does nothing without a level; the icon is appended after the text (Element writes text before children).
17. `buttons/mod.rs`, `forms/mod.rs`: `#[derive(Default)]` gives empty `button_type` / `input_type` (renders `type=""`); `new` is the only correct constructor. `new` fns lack `#[must_use]`.
18. `forms/mod.rs` `FormSelect::build`: no `selected` for unmatched value and no multi-select support (`multiple` not available).

quench-client
19. `transport.rs` `exchange` (redirect loop): request headers, including `Authorization` and `Cookie`, are forwarded unchanged to the redirect target, also across hosts and from https to http. Potential credential leak; browsers/reqwest strip `Authorization` on cross-origin redirects.
20. `lib.rs` `ClientCredentialsClient::access_token`/`refresh_token`: the cache mutex is released before the exchange, so concurrent callers stampede the token endpoint; and a 401 from the target does not invalidate the cached token. With `expires_in <= 30` the TTL is 0 (every call exchanges).
21. `lib.rs`: `ClientError` / `ClientResult` are public but unused.
22. `lib.rs` typed clients: no timeout is configurable (`HttpClientBuilder` has only `tls_verify`), no `Accept` header, and base URL + path is plain concatenation (a trailing slash on the base gives `//`).
23. `lib.rs`: `BearerAuthClient` has no `put`/`delete`/builder; `ClientCredentialsClient` has no `put`/typed `delete`; `BasicAuthClient::delete` vs `delete_expect_success` duplication of the status check code.
24. `transport.rs` `Response::text` decodes lossily and ignores the response charset.
25. `transport.rs`: `max_response_bytes` is not applied to chunks from `bytes_stream`; `send_streaming` connections are never pooled.
26. `transport.rs` `RequestBuilder::header`: appends rather than replaces (except `user-agent`), so `.json(..)` after a manual `content-type` produces two `Content-Type` headers.
27. `blocking.rs`: current-thread runtime shared by clones; behaviour when used concurrently from several threads is not covered by a test (I assume `block_on` calls serialise; unverified).

quench-tls
29. `lib.rs` `ClientOptions::default()` has `use_system_roots: false` (derived `Default`), unlike what most callers want; `quench-client` sets it true explicitly.
30. `lib.rs` `load_server_config`: all failures collapse to `None` (cause lost); `server_config` has no client-auth variant, so mTLS servers (used by tests) must build rustls configs by hand.
31. `lib.rs` `root_store`: reads only the first existing system bundle; a bad or partially invalid bundle is silently skipped, and a set-but-nonexistent `SSL_CERT_FILE` silently falls back to the fixed list. No `SSL_CERT_DIR`.
32. `lib.rs` `TlsStream::poll_write`: ignores `Pending` from pushing records after buffering (data may sit in rustls' buffer until the next write/flush); callers must `flush`. Standard for this adapter style but worth noting; no test asserts it directly.
33. `lib.rs` `poll_shutdown` does not wait for the peer's `close_notify` (intended, unverified against all HTTP/1.1 keep-alive uses).
34. `tests/fixtures/README.md` says the fixtures are for `quench-proto` tests, but the files live in `quench-tls`.

---

## Area: db

## Gap report: quench-db, quench-pg

Pages: `docs/crates/quench-db/` (12 pages) and `docs/crates/quench-pg/` (11 pages). Old flat pages `docs/quench-db.md` and `docs/quench-pg.md` replaced and deleted. No `src/`, tests or manifests changed.

### 1. Stale or wrong statements in the old docs

quench-db (`docs/quench-db.md`):

* The usage example `#[derive(Debug, Serialize, Deserialize)] struct User` does not compile: `Model` requires `Clone` (`quench-db/src/lib.rs:25`).
* "That build has 9 third-party crates ... instead of 25": the current offline `cargo tree -e normal` gives 10 package versions without default features (two `getrandom` versions) and 26 with defaults. `docs/README.md:133` repeats the "9".
* Test paths given as `libs/quench-db/tests/...`; the crate is at `quench-db/` in this repository. The old page names only `lib_tests.rs` and `runner_tests.rs` under `tests/unit/`; it has six files (catalog, backends_in_memory, error, migrations, lib, runner).
* `ModuleManifest` field list omitted `description`.
* The migration example used `anyhow::Result`, which is not a dependency of the crate.
* "`Db::repository::<T>()`" and CRUD described without the backend differences (create/update/find_by semantics differ; see list 3).
* `Database::migrate` described as "the migration ledger table ... named by `DB_MIGRATION_TABLE`" without stating that it ignores `since` and does not render `${var}` (it uses `ChangeSet::to_sql()`, `backends/postgres.rs:86`).
* "9 third-party crates ... `ring` for the plan hashes": correct in spirit; `quench-crypto` pulls `ring`.

quench-pg (`docs/quench-pg.md`):

* "verify-full ... verified against the public roots, the system bundle and sslrootcert": correct, but `Prefer`/`Require` also do not verify; old text did not say `prefer` can be downgraded (added).
* `Row::get` examples elsewhere use bare integers; real index types are `&str`, `&String`, `&usize` (sealed `RowIndex`).
* Nothing else contradicted the code.

### 2. In the code, undocumented in the old pages

quench-db:

* `Repository`, `InMemoryDb`/`InMemoryRepository` details and their divergences from Postgres (silent overwrite on `create`, upsert on `update`, string-only `find_by`, `i64`-only numeric keys).
* `PostgresDb::pool()`; `MigrationRunner::resettable_schemas`, `installed_modules`, `module_table`, the ledger table DDL, the `DEFAULT_*` constants, `validate_identifier` and `advisory_lock_key` (`#[doc(hidden)]` but `pub`), `ApplyReport::count`/`with_outcome`.
* The exact SQL generated by `PostgresRepository`; `jsonb_populate_record` mapping; `ChangeSet` TOML spellings (`sql`, `createSchema`, `createExtension`, `createTable`) and `ColumnDef`.
* `render` rules (trimmed names, no escape for `${`, no rescanning), `RenderError`; built-in variables `schema`, `module`, `version`.
* Plan algorithm details: instance keys (`module@schema`, `module@*`), checksum definition, conflict/replace rules for repeated requests, tie-break order.
* `CatalogError`, `PlanError` full variant lists and texts; `MigrationLoader` rules (duplicate prefix per file, missing directory is empty, non-recursive).
* `quench_db::prelude` contents (excludes the in-memory types).
* `DB_POOL_MAX_SIZE` silently falls back to 5 on bad text and `0` becomes pool size 1.

quench-pg:

* Everything under `message` (all frame builders, `Backend`, `FieldDescription`, `decode`, `rows_affected`) and `scram` (`MECHANISM`, `Scram` API, limits) was only described in prose.
* `Config` fields and defaults in full (`max_message_size` 256 MiB, `ssl_root_cert_pem`, `options`), the URL grammar details (last `@`, `%2F` socket hosts, empty parts keep defaults, `sslrootcert` read at parse time and repeatable).
* `types::oid` constants, `Column`, `decode_text` table, array element OID table, `decode_bytea`/`encode_bytea`/`parse_timestamp`/`parse_array`, full `ToSql`/`FromSql` impl lists (and what is missing: no `FromSql` for `i8`/`u8`/`u16`/`usize`/`PgArray`).
* `Connection`: `from_stream`, `startup`, `looks_dead`, `is_closed`, `transaction_status`, `parameter`, `created_at`, `last_used`, `close`, the `Stream` trait, `QueryResult`.
* `Pool`: `available`, `idle_count`, `close`, exact `get` algorithm, `PooledConnection::begin`, drop rules.
* `Row`: `get_raw`, `is_null`, `value`, `to_value`, `to_array`, `columns`, `len`, `is_empty`.

### 3. Open questions and suspected bugs

1. `quench-db/src/runner.rs:335` `MigrationRunner::reset` ignores `dry_run`: a runner built with `.dry_run(true)` still takes the lock and executes `DROP SCHEMA ... CASCADE`. Probably unintended; at minimum surprising.
2. `quench-pg/src/error.rs:36` `ServerError::parse`: `b'V' | b'S' if out.severity.is_empty()` takes whichever of `S`/`V` comes first. PostgreSQL sends `S` (localised) before `V` (non-localised), so the severity is the localised text on servers with non-English `lc_messages`. The test `malformed_field_lists_do_not_panic` asserts exactly this (`SFEHLER\0VERROR` gives `FEHLER`) while its comment says the localised one is used "only when V is absent". Comment and behaviour disagree.
3. `quench-pg/src/pool.rs:230-231` doc of `Pool::close`: "Checked-out ones are closed as they come back if the pool is dropped". `give_back` does not check whether the pool was closed or dropped, and `close` does not mark the pool closed, so checked-out connections are returned to the idle queue normally (the `Arc<Inner>` keeps the queue alive). The documentation does not match the code.
4. `quench-pg/src/pool.rs:301` `fn age` is `#[allow(dead_code)]` and unused.
5. `quench-pg/src/scram.rs` (`Scram::new` doc): "random 24-byte nonce"; the code encodes 18 random bytes, giving 24 base64 characters (`scram.rs:32`). Wording only.
6. `quench-db/src/backends/postgres.rs:34-37` `DB_MIGRATION_TABLE` is read per call, not validated and interpolated into DDL/DML (the runner validates its own names with `validate_identifier`, this path does not). Environment-controlled, so low risk; still unvalidated SQL text. Likewise `Model::table_name`/`primary_key_name`/`columns` are interpolated unquoted (by design, but undocumented in source).
7. `quench-db/src/backends/postgres.rs:86` `Database::migrate` ignores `Migration::since` and does not substitute `${var}`; a migration file written for the catalog and passed to `migrate` runs with literal `${...}` text. Intended? The in-memory `migrate` is a no-op.
8. `quench-db/src/backends/postgres.rs:132-` `create` uses `jsonb_populate_record(NULL::table, $1)`, so model fields absent from the JSON become NULL rather than the column default (`SELECT *` from the record supplies every column). Models with `#[serde(skip_serializing_if ...)]` lose DB defaults. Not tested.
9. `quench-db/src/error.rs` mapping loses the SQLSTATE: unique violations become `QueryError(text)`; callers must match the text. A statement timeout becomes `ConnectionError`. A single-row query returning several rows is `QueryError`, zero rows is `NotFound` (also for `create`'s `RETURNING *`, which cannot happen).
10. `quench-db/src/backends/in_memory.rs` (`create`): `update` on a missing id inserts. Behaviour differs from Postgres (`NotFound`); tests encode it (`update_overwrites_the_existing_row` covers only the existing case). Also `u64` keys above `i64::MAX` are rejected.
11. `quench-db/src/catalog.rs:81` custom `Deserialize for Requirement`: in the table form a bad `version` yields a `quench_serde::Error` (surfaced as `BadManifest`), in the short form `BadRequirement`. Inconsistent error variants for the same mistake.
12. `quench-db/src/migrations.rs`: `ChangeSet::CreateTable { schema: Option<String> }`, `ColumnDef.primary_key`, `.nullable` have no `#[serde(default)]`. Whether an omitted key deserialises as `None` was not verified (no test); marked *Unverified* in the docs.
13. `quench-db/src/lib.rs:21-25` `#[async_trait]` on `Model` (no async methods): harmless but pointless.
14. `quench-db/src/catalog.rs` `Catalog::load` skips a directory without `module.toml` silently (tested: `directories_without_a_manifest_are_ignored`), so a mistyped manifest filename makes a module vanish; the later `UnknownModule` error is the only symptom.
15. `quench-pg/src/types.rs` `bytea`: only the hex format is decoded; `bytea_output = 'escape'` is never set by the client, so a server configured that way would produce strings. Not tested. Marked as a note in internals.
16. `quench-pg/src/conn.rs:320` `looks_dead` uses a noop-waker poll on the stream: a TLS stream that has buffered application data or a close_notify may report "ready" (dead) spuriously, or a half-read TLS record may hide EOF. Not verified; live tests cover plain TCP kill only (`idle_connections_that_were_killed_are_replaced`).
17. Not verified at runtime: nothing was executed against a database (instructions: no DB except a throwaway one; none was started). All behaviour statements come from reading the source and tests.

### Public item counts (documented)

* quench-db: 3 traits (`Database`, `Model`, `Crud`), 3 root enums (`Db`, `Repository`, `DbError`), 1 module of 12 items (`migrations`: `Migration`, `render`, `RenderError`, `MigrationFile`, `MigrationLoader`, `ChangeSet`, `ColumnDef` + methods), `catalog` 8 types/consts/fns (+ 12 methods), `plan` 6 types (+ 6 methods), `runner` 3 consts + 6 types + 2 hidden fns (+ 14 methods), 4 backend types (+ constructors), `prelude`. About 95 items counting methods and variants as listed on the pages.
* quench-pg: root re-exports 17 types, `types` module 25 items (+ `oid` module with 23 consts), `message` 12 builders + `decode`, `rows_affected`, `Backend`, `FieldDescription`, `scram` 1 const + 1 type + 5 methods. About 130 items counting methods.

---

## Area: auth

## Report: quench-redis, quench-cache, quench-auth

Pages: `docs/crates/quench-redis/` (9 pages), `docs/crates/quench-cache/` (8), `docs/crates/quench-auth/` (10). Old flat pages `docs/quench-redis.md`, `quench-cache.md`, `quench-auth.md` deleted; links fixed in `docs/README.md` (lines 62, 104, 105), `docs/quench-starter.md`, `docs/examples/forge_service_demo.md`, `quench-auth/README.md`, `quench-cache/README.md`. Not touched (outside the allowed area): the root `README.md` lines 9, 15, 63 still point at the deleted flat pages; `docs/README.md` lines 18-19 (the tree listing) and 58 still mention `quench-auth.md`/`quench-cache.md`. `quench-redis` has no README.

### 1. Stale or wrong statements in the old docs

quench-redis
* Nothing materially wrong. Minor: the old page shows `.arg(&5)` correctly, but the doc comment in `quench-redis/src/cmd.rs:6` says `cmd("SET").arg("key").arg(5)`, which does not compile (`arg` takes `&T`).

quench-cache
* "a `DashMap<String, CacheEntry>`": `DataCache` uses `quench_core::map::ShardedMap` (`data_cache.rs`).
* "`default = ["request-cache", "data-cache"]`": the default also includes `tls`.
* "`CacheError`... (`thiserror`-derived)": it derives `quench_core::Error`.
* "`libs/quench-cache/tests/` has `unit.rs` ... and `redis_store.rs`": wrong path (`quench-cache/tests/`); there is also `tests/from_env.rs`.
* Did not mention that `RedisStore` is reachable only as `quench_cache::store::RedisStore` (not in the root or prelude), that `RequestCache::has` ignores expiry, or that `CacheError::Miss` is never produced.
* "All three are re-exported from `quench_cache::prelude`": true for CacheStore, DataCache, RequestCache, but `RedisStore` and `CacheError` are not in the prelude.

quench-auth
* "Read from the environment via `envmnt`": the code uses `quench_core::env` (`get_or`, `var`).
* "`Access` and `Claims::can` are here": there is no `Access` type; the types are `Actions` and `Permissions` (`domain/auth.rs`).
* Example `HttpResponse::Forbidden().finish()` is the old actix API; the stack is `quench_http`.
* Session-watcher paragraph: the watcher no longer navigates to login straight away; on `authenticated:false` it first POSTs `<prefix>/refresh` and only navigates if that fails (`quench-web/src/framework/app/scripts.rs:164-215`). It also said "polls once a minute"; the interval is the `interval_secs` argument of `session_script`.
* "Relying parties that depend on it include gatehouse, sage-service, switchboard-service, warehouse-service, conveyor-service" and the gatehouse file paths (`services/gatehouse/service/src/...`): not verifiable in this repository; dropped from the new pages.
* `JwksVerifier` description omitted the 3600 s / 30 s cache TTLs and the `jwks` cache prefix.

### 2. Existing in the code but undocumented in the old pages

quench-redis: `Connection` methods (`from_stream`, `handshake`, `looks_dead`, `created_at`, `last_used`, `ping`, `close`, `request_value`), `Client::{lazy, get, address, idle_count, close, exec_value}`, `PooledConnection` (Deref), `Cmd::{args, name, get, key}`, `ToArg` impl list, `SlotMap` API, `crc16`/`key_slot`/`parse_primary`/`SLOTS`, `ClusterClient::{refresh, nodes, connect_with, exec_value}`, `resp::{encode_command, decode}` signatures and limits (512 MiB bulk, 16 Mi elements, depth 32), `ServerError::parse`, exact error texts, URL parsing rules (percent-decoding, IPv6, empty parts), `PoolOptions` defaults.

quench-cache: `CacheEntry`, both `CacheStats` types, `DataCache` set semantics and expiry rules, `RedisStore` command mapping, `parse_primary` hidden forwarder, `CacheStore` Clone/Debug, exact `from_env` precedence (first *set* variable wins even when blank), `generate_key` algorithm.

quench-auth: `KeyResolver`/`KeySigner`, `JwtConfig` fields and constructors' env defaults, `narrow_audiences`, `issue_access_token_for`, `Claims::{roles, permissions, for_audiences}`, `domain::signing`, `domain::percent`, full `User` model (19 columns) and lockout methods, `Role` API, session key layout and the 30 s reuse grace window, `sessions_for`, `refresh_delegation`, `LoginQuery`, `AuthStatus`, `allowed_redirect_hosts`, `cookies::cookie_value`, all realm cookie attributes and the cleared-cookie functions, the authorize/callback wire details, `RefreshedTokens`, `AUTH_REDIRECT_HOSTS`.

### 3. Open questions and suspected bugs

1. `quench-auth/src/http/routers/ui/pages/auth.rs:62-67`: `auth_status` splits `scope` on commas only, while `Claims::roles` (`domain/jwt.rs`) splits on spaces and commas and every doc/test elsewhere uses space-separated scopes (`"user sage:read"`). A real space-separated scope would come back as one role string. The test (`tests/http_ui.rs:257`) uses a comma scope, so it does not catch this. Also `roles` includes `service:action` permission entries, not only roles.
2. `quench-auth/src/http/middleware/auth.rs:126` vs `src/http/routers/ui/mod.rs:47`: with no `SessionDb` in the container, `Auth` rejects session-bound tokens while `is_ui_authenticated`/`get_user_from_req` trust the token. Inconsistent (maybe intentional, documented as is).
3. `quench-auth/src/http/domain/sso_client.rs:32`: `authorize_redirect` always returns to `ui_path("/home")`; `?redirect=` (`redirect_target`) is never consulted by `login_delegation`. `redirect_target` is therefore unused inside the crate.
4. `quench-auth/src/http/domain/sso_client.rs:78`: `state` compared with `!=` (not constant time); low risk, cookie-bound.
5. `quench-auth/src/domain/session.rs:227` (`rotate_from_reuse`) and `:253` (`revoke_by_refresh_token`): revoking does not remove `reuse:{old_hash}`, so for up to 30 s a previously consumed token still returns a token pair for a revoked session (the returned refresh token is dead, because its `refresh:` key is removed, but the answer is `Some`).
6. `quench-auth/src/domain/session.rs` `rotate`: the sequence take/get/set/set/add/set is not atomic; a crash in between loses the session's refresh capability (the old token is consumed, the new one may not be stored).
7. `quench-auth/src/domain/auth.rs`: derived `Debug` for `User` prints the password hash and `mfa_secret`.
8. `quench-cache/src/store.rs:113-119`: in-process `take` is `get` then `delete`, not atomic; the doc comment says "exactly one gets the value", true only for Redis (`GETDEL`).
9. `quench-cache/src/store.rs:396`: `RedisStore::clear` with an empty prefix uses pattern `*` and deletes the whole database, contradicting the doc comment "never the whole database". Glob characters in the prefix are not escaped either.
10. `quench-cache/src/store.rs` `set` with `Some(0)`: Redis rejects `EX 0` (error), the in-process cache expires immediately. Behaviour differs between backends.
11. `quench-cache/src/request_cache.rs:41`: `generate_key` uses `DefaultHasher` (algorithm not guaranteed stable across Rust releases) and joins parts with `-` so `["a-b","c"]` and `["a","b-c"]` collide. `has` (`:76`) ignores expiry.
12. `quench-cache/src/lib.rs:6`: `CacheError::Miss` is never constructed.
13. `quench-cache` has two distinct, identical `CacheStats` structs (`data_cache.rs`, `request_cache.rs`).
14. `quench-redis/src/cluster.rs:381`: after a `CLUSTERDOWN` refresh the command is retried against the same address (no re-route), unlike the I/O-error path which re-routes.
15. `quench-redis/src/cluster.rs:245`: cluster nodes force `db: 0`; the `/db` of the seed URL is silently ignored (documented in the new pages).
16. `quench-redis/src/config.rs`: `redis://user@host` (user without password) parses but sends no `AUTH`; the username is silently unused.
17. `quench-redis/Cargo.toml:21`: `quench-core` is a regular dependency but is not used under `src/` (only by tests).
18. Root `Cargo.toml` pins workspace dependency versions that differ from the crates' own (`quench-cache` 0.1.7 vs 0.1.20 at line 86, `quench-redis` 0.1.0 vs 0.1.8 at line 138, `quench-auth` 0.4.0 vs 0.4.16 at line 85); harmless for path dependencies, may matter for publishing.
19. Pushes (RESP3) are delivered as ordinary values and would be mistaken for command replies; the pool should not be used with client tracking or Pub/Sub (design limit, documented in Internals).
20. Not verified (no live Redis used): all `tests/live.rs` behaviour, and Redis's rejection of `EX 0`. No tests were run for any of the three crates.

---

## Area: misc

## Gap report: quench-mail, quench-k8s, quench-fullstack, quench-fullstack-macros, quench-starter

Pages are in `docs/crates/<crate>/`. Old flat pages `docs/quench-{mail,k8s,fullstack,fullstack-macros,starter}.md` were deleted after being covered. No source file was changed.

### 1. Stale or wrong statements found in the old docs / source comments

#### quench-mail
* `src/smtp.rs:432` doc comment of `Response::message` says it returns text such as `2.0.0 Message queued as 1234`. The code strips the leading enhanced status (`Reply` parsing, `smtp.rs` `Connection::reply`), the test asserts `"queued as TEST1"` (`tests/unit/smtp_tests.rs:55`). The old page repeated "the server's `250` text, e.g. its queue id" without the detail.
* Old page: "`Message::formatted()` ... Bcc appears in no header" is right; but it did not list the reserved header names, the header order, or that custom headers pass through `fold_header` (runs of spaces collapse).
* Old page says a single default 30 s "per step"; matches code (every wait, connect, handshake, write).

#### quench-k8s
* `src/lib.rs` / `Config::infer` doc: "the kubeconfig named by `KUBECONFIG` (first path of the list)". The code (`config.rs:default_kubeconfig`) takes the first path in the list that **is an existing file**, falling back to `~/.kube/config` when none exists. The old page said "first existing file", which is right.
* Old page: "Client certificates use mutual TLS from `quench-tls`". The client only passes the identity to `quench_client::transport` (`client.rs:46-48`); `quench-tls` is not a direct dependency of this crate.
* Old page claim of a read-only run against a real k3s cluster (104 resources) cannot be reproduced from the repository; marked *Unverified* in the new testing page.

#### quench-fullstack
* Old page: "64 unit tests (96% line coverage)". The test binary now has 76 `#[quench_test::test]` functions. Coverage figure not verified.
* Old page: "The golden test `tests/fixtures/page.html`" fine; the file is a single line without trailing newline (661 bytes), so `wc -l` reports 0.
* `src/lib.rs:23` still points at `docs/quench-fullstack.md` (deleted). `docs/quench-ui-web.md`, `docs/quench-tool.md` pointed there too; their links were repointed to `docs/crates/quench-fullstack/...`. The `src/lib.rs` comment was not changed (docs-only rule).
* Old page lists `ServerError` statuses but not that `Network`/`BadResponse` report 502 from `status()` (`error.rs:status`).
* Old page: "with `SERVICE_AUTH_ENABLED` off, as in development, everyone is an administrator." Accurate, but incomplete: an invalid or missing bearer token also ends up as the admin user in that mode, because `User::from_request` falls through to `get_user_from_req`, which returns `admin` first (`server.rs` `from_request`; `quench-auth/src/http/routers/ui/mod.rs:57-65`).

#### quench-fullstack-macros
* Old page lists the same options as the code; it omitted that `ok_type` takes the **first type argument** of any path ending in `Result`, so the error type is never checked and aliases with one argument pass.
* Version: macros crate is `0.1.0`, runtime `0.1.7`; the old page did not state versions.

#### quench-starter
* Old page: `CorrelationId` "reads or generates `X-Correlation-ID`, stores it on the request". The code does **not** store it on the request (comment in `correlation.rs` says so); it records it on the tracing span and echoes it on the response.
* Old page: `logging::init()` "loads `.env` (root, then `docker/<CARGO_BIN_NAME>/.env`)". Code loads the first `.env` found in the working directory **or the nearest ancestor**, then `docker/<CARGO_BIN_NAME>/.env` relative to the working directory, never overriding existing variables.
* Old page: swagger "redirects `/swagger-ui` and `/swagger-ui/` to `/swagger-ui/index.html`". Actually two `308` redirects: `/swagger-ui` to `/swagger-ui/`, and `/swagger-ui/` to `/swagger-ui/index.html` (`swagger.rs`).
* Old page: "Read via `envmnt`/`std::env`". The code uses `quench_core::env`.
* Old page lists `SERVER_CERT_PATH`/`SERVER_KEY_PATH` without defaults; the defaults are `cert.pem` and `key.pem` (`http/mod.rs:serve_app`).
* Old page: "`discover_and_mount` ... wraps it in the correlation-id/logger middleware". Code also wraps `SecurityHeaders` (innermost of the three).
* Old page: dependents "under `docker/`": those paths are stale after the services moved (not checked further).
* Old page: "`HealthState` ... `live()`": `HealthState::default()` (derived) is **not** live; only `live()` is.

### 2. Things in the code that the old docs did not mention

* quench-mail: public `mime` module (8 functions), `Mailbox::new`, `Address::new/local/domain`, `MessageBuilder::date`, `Message::message_id`, `Envelope` public fields, `dot_stuff`, the reserved header list, header folding at 78 columns, `Reply::message`, `Error::reply`, the fixed limits (4096-byte line, 100 reply lines, 2 s QUIT wait).
* quench-k8s: exact paths and query strings per call, `ListParams::page_size`, `PostParams`/`PatchParams` fields, `Propagation`, `DeleteOptions` body, `Capabilities::supports_operation`, `ApiGroup::recommended_resources`, `Discovery::groups`, `Object::kind/labels/annotations`, `Config::new/in_cluster_at/from_kubeconfig_str`.
* quench-fullstack: `Ui::export`, `Ui::with_script/render/render_with_nonce/paths/assets`, `Page::redirect/routed`, `server::{collect, dispatch, with_user, guard, ServerFn, FUNCTIONS}`, `ErrorBody`, `FieldError`, `FormExt::finish`, `INTERNAL_MESSAGE`, `User::has_role`, `Bearer::{signed_in, access_token, refresh, logout}`, `client::reset`, `StubGatehouse` methods (`set_access_ttl`, `revoke_all`, `login_count`, `refresh_count`) and its endpoint contract, `content_hash`, `Asset` accessors, `wire` constants, `render_document`, `content_security_policy`, `new_nonce`, `escape_json`, the full guard check order.
* quench-fullstack-macros: the exact compile-error messages and check order, the generated item names (`__QuenchArgs`, `__QuenchEndpoint`, `__user`, `__args`).
* quench-starter: `SecurityHeaders`, `http::domain::error` (`{"errors":[...]}` shape, 3 constants), `ApiError`/`json_error`, `human_bytes`, `inject_correlation_id_header`, `CircuitState`/`get_state`, `serve_assets`, `server_root_redirect`, `css` rule functions, `RedirectToHttps` behaviour (308, Host header), `.env` parser syntax, the glob re-export caveat at the crate root (`quench_starter::normalize_base_path` does not exist).

### 3. Open questions and suspected bugs (file:line approximate)

#### quench-mail
* `src/message.rs:195,340` `MessageBuilder.message_id` is a private field that is never set (no setter), so a caller cannot choose a Message-ID. Dead field or missing setter.
* `src/smtp.rs:500-516` `add_root_certificate_pem` detects "no certificate in the PEM" by comparing root-store sizes; a certificate that is already in the built-in roots (or a duplicate) may give a misleading `Config("no certificate found in the PEM")` if the store does not grow. Unverified.
* `src/error.rs:90-96` `is_transient` is false for `Error::Auth` even when the reply is 4xx (for example 454 "temporary authentication failure"); arguably retryable.
* `src/smtp.rs:1015-1036` `deliver`: the first rejected `RCPT TO` aborts the whole send; no partial-delivery handling and no `RSET` before `QUIT`.
* `src/smtp.rs:916-925` any non-250 answer to `EHLO` (including 421/5xx) triggers a `HELO` fallback.
* `src/smtp.rs` capability parsing ignores the legacy `AUTH=PLAIN` form.
* No limit on subject or header length beyond folding (a single token longer than 78 columns is left whole; RFC line limit 998 is only checked for bodies).

#### quench-k8s
* `src/client.rs:513-529` `Api::create` on a namespaced resource with an `Api` that has no namespace posts to the all-namespaces collection path instead of failing like the per-object calls do (`client.rs:435`).
* `src/client.rs:290` `Patch::Apply` is sent as `application/apply-patch+yaml` with a JSON body (valid YAML, works, but is a convention); the doc says it "needs a `field_manager`" yet nothing enforces it client side.
* `src/client.rs:109` `get_json` only decodes JSON, so endpoints like `/healthz` (plain `ok`) return `Error::Decode`.
* `src/config.rs:601-627` `infer`: once a kubeconfig file is found, a failure to parse it does not fall back to the in-cluster configuration (by design?). `infer_with_context` ignores the context in-cluster.
* `src/config.rs:658` `Auth::BearerFile` is read with blocking `std::fs::read_to_string` on every request inside async code (`client.rs:134`).
* `src/discovery.rs:216` the core group is hard-coded to `v1`; `find_kind` returns the first kind match across groups (e.g. `Event` exists in `""` and `events.k8s.io`).
* `Config` ignores `proxy-url`, `tls-server-name`, multi-file `KUBECONFIG` merging.

#### quench-fullstack
* `src/client.rs` `renew`/`State.failed`: a failed refresh stores `(generation, error)` and it is only cleared by a generation bump or `reset()`. After a failed refresh followed by a fresh `Bearer::login` + `use_bearer`, a later 401 in the same generation returns the stale error without trying a refresh. Unverified; needs a test.
* `src/server.rs:guard` the origin check compares only the authority of `Origin` and `Host` (scheme stripped), so `http://host` and `https://host` are treated as the same origin. Low impact (the custom header and `Sec-Fetch-Site` still apply).
* `src/app.rs` `UiApp::call`: a `#[server]` function path is matched before pages and for any HTTP method, so a page registered at the same path is unreachable; also an unmatched `/_quench/dev*` path with dev reload off is a 404 from `Assets::serve` (fine).
* `src/app.rs` `Ui::export` does not apply `dev_reload`'s script to the exported page (probably intended).
* `src/server.rs:with_user` / `user()` use a `tokio::task_local!`: calling `user()` from a task spawned inside a server function returns `Unauthorized` (task-locals do not cross `tokio::spawn`).
* `src/server.rs:from_request` (bearer with auth disabled): see section 1; with `SERVICE_AUTH_ENABLED` off any request, including one with a garbage bearer, becomes `admin`.
* `Cargo.toml` `[lib] harness = false` with `#[cfg(test)] quench_test::main!();` in `lib.rs:114`: lib unit tests exist only through that macro; there are none in the lib besides it.

#### quench-fullstack-macros
* `src/expand.rs` (`options`, `server`): lifetime-only generics are rejected as "cannot be generic" (any entry in `generics.params`); `impl Trait` parameters are accepted by the macro and then fail inside the generated struct with a confusing error; `Pat::Ident` also accepts `mut x` / `ref x`.
* `ok_type` accepts any path ending in `Result` with at least one type argument (error type never checked); a return type such as `Result<T, MyError>` compiles in the macro but fails to link into the runtime's `Result<T, ServerError>` expectations at the `dispatch` bound.

#### quench-starter
* `src/common/wait.rs:13` `wait_for_services` builds its client with `danger_accept_invalid_certs(true)` and polls forever with no overall timeout and no error return.
* `src/http/mod.rs:RedirectToHttps` builds `Location` from the request's `Host` header without validation; with no Host header the result is `https://` plus an empty authority. Redirect-only server, low impact.
* `src/metrics.rs:record_error`, `reset`, and all `CircuitBreaker` methods `unwrap()` poisoned mutexes (panic).
* `src/metrics.rs:snapshot`: `avg_latency_ms` divides the sum of successful latencies by the total request count including errors, lowering the average when errors occur.
* `src/common/routes.rs` `BASE_PATH` is cached in a `LazyLock` on first use, while `serve` reads `BASE_PATH` again; a program that changes the variable between the two would mount and redirect under different bases.
* `src/resilience.rs` `CircuitBreaker`: `failure_count` is not reset when entering `HalfOpen`, so any failure while half-open re-opens at once (probably intended); `CircuitState` is only reachable through `get_state`.
* `src/common/format.rs` `human_bytes` takes `i64`; negatives print as `"-5 B"`.
* `Cargo.toml` lists `quench-auth` features `redis`, `postgres`, `tls` although only `quench_auth::http::...` and `JwtConfig` are used in this crate; not investigated.
* The root README (`/mnt/extdrive/projects/lorehaven/quench/README.md`) and `docs/README.md` (the directory tree text lines 26-30) still mention the old flat page names; the link lines in `docs/README.md` were repointed, the tree text and the root README were left (outside the allowed scope).

### Counts of documented public items

| Crate | Types/consts/free fns | Methods and trait impls | Notes |
|---|---|---|---|
| quench-mail | 12 types, 9 free fns (`dot_stuff` + 8 in `mime`) | about 40 methods | all re-exports at crate root plus `mime` |
| quench-k8s | 22 types/aliases, 2 free fns | about 62 methods | |
| quench-fullstack | about 45 top-level items (consts, types, traits, fns, statics) | about 90 methods/variants | server-only vs all-target marked |
| quench-fullstack-macros | 1 attribute (`server`) with 2 arguments (`path`, `auth`) | 9 compile-error cases | |
| quench-starter | about 55 top-level items | about 30 methods | |
