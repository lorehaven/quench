# Quench Documentation

Quench is a modular Rust workspace of shared library crates split out of [Forge](https://github.com/lorehaven/forge), which remains the primary consumer alongside other services. Jump straight to a crate's page below.

## Layout

```text
docs/
├── README.md                 # this file
├── quench-archive.md
├── quench-template.md
├── quench-pdf.md
├── quench-convert.md
├── quench-html.md
├── quench-test.md
├── quench-test-bdd.md
├── quench-args.md
├── quench-auth.md
├── quench-cache.md
├── quench-cli.md
├── quench-client.md
├── quench-config.md
├── quench-db.md
├── quench-http.md
├── quench-log.md
├── quench-mail.md
├── quench-starter.md
├── quench-style.md
├── quench-fullstack.md
├── quench-ui.md
├── quench-ui-web.md
├── quench-web.md
├── quench-web-components.md
└── examples/                 # examples/* runnable references
    ├── basic.md
    ├── db_example.md
    ├── forge_service_demo.md
    └── http_demo.md
```

Each page lives at the same path as the crate it documents (e.g. `quench-auth` → `docs/quench-auth.md`), so the doc to update is always predictable from the code you're touching. Every crate's own `README.md` is a short pointer into its page here — this tree is the actual documentation: descriptions, features, configuration, and API surface.

## Crates

- [Quench Auth](./quench-auth.md) — relying-party token/session verification
- [Quench Http](./quench-http.md) — lightweight, `poem`-shaped HTTP framework: annotation-discovered routing, dependency injection, plain-async middleware
- [Quench Starter](./quench-starter.md) — service bootstrap (TLS, base-path scoping, health, DB wiring)
- [Quench Web](./quench-web.md) — dependency-light server-rendered HTML/CSS/JS page builder
- [Quench Web Components](./quench-web-components.md) — higher-level UI builders on top of Quench Web
- [Quench DB](./quench-db.md) — ORM/CRUD abstraction plus a migration catalog engine
- [Quench Log](./quench-log.md) — leveled events, spans, console and rolling-file output
- [Quench QR](./quench-qr.md) — QR code encoder (ISO 18004, versions 1-40) with SVG output, no dependencies
- [Quench Archive](./quench-archive.md) — tar, zip, deflate, gzip and zstd (decoder and encoder), no dependencies
- [Quench Template](./quench-template.md) — Jinja-compatible templates over quench-serde values (filters, tests, macros, imports)
- [Quench PDF](./quench-pdf.md) — PDF text extraction (xref tables and streams, object streams, fonts, encodings)
- [Quench Convert](./quench-convert.md) — CSV, XML, Android binary XML and GGUF as quench-serde values
- [Quench HTML](./quench-html.md) — HTML parser with CSS selectors, and CommonMark to HTML
- [Quench Test](./quench-test.md) — test harness with JSON/HTML result pages, command runner, predicates, mock HTTP server
- [Quench Test BDD](./quench-test-bdd.md) — Gherkin runner: cucumber expressions, async steps, concurrent scenarios, JSON/HTML pages
- [Quench Args](./quench-args.md) — command-line parsing with clap-compatible derives, no clap
- [Quench K8s](./quench-k8s.md) — generic Kubernetes API client (kubeconfig, dynamic resources, discovery, logs, watch)
- [Quench Schema](./quench-schema.md) — JSON Schema validation over quench-serde values
- [Quench Regex](./quench-regex.md) — regular expressions with Unicode classes, no dependencies
- [Quench Style](./quench-style.md) — shared design tokens, themes, base CSS and i18n for web, mobile and desktop
- [Quench TZ](./quench-tz.md) — IANA time zones with the database built in
- [Quench UI](./quench-ui.md) — UI core: signals, views, events and the renderer interface
- [Quench Fullstack](./quench-fullstack.md) — serves Quench UI apps from Quench HTTP: server rendering, hashed assets, strict CSP
- [Quench UI Web](./quench-ui-web.md) — runs a Quench UI app in the browser from WebAssembly, with a small JS bridge
- [Quench PG](./quench-pg.md) — PostgreSQL wire-protocol client with SCRAM, TLS, pool
- [Quench Redis](./quench-redis.md) — Redis client: RESP2/3, pool, cluster routing
- [Quench Cache](./quench-cache.md) — shared in-process/Redis caching layer
- [Quench Client](./quench-client.md) — shared authenticated HTTP client wrappers
- [Quench Config](./quench-config.md) — typed config/env loading helper
- [Quench Mail](./quench-mail.md) — MIME message builder and async SMTP submission client
- [Quench CLI](./quench-cli.md) — shared terminal UI styling for CLI tools

The `docker/*` services and several `cli/*` tools in the sibling [Forge](https://github.com/lorehaven/forge) repository depend on these crates, pulled in from the `ennor` cargo registry like any other dependency — see that repo's own docs for how they're used in practice.

## Cargo features

Features are additive and, unless noted, on by default. Turn them off with `default-features = false` and name the ones you want. Internally the crates depend on each other with `default-features = false` and list what they use, so a feature you disable stays disabled all the way down.

| Crate | Features |
| --- | --- |
| `quench-proto` | `server`, `client`, `ws`, `multipart`, `sse`, `tls` |
| `quench-http` | `tls` (`serve_https`, `load_tls`) |
| `quench-client` | `tls` (`https://`), `blocking` (off by default: `quench_client::blocking`) |
| `quench-pg` | `tls` (`sslmode=require`, `verify-full`) |
| `quench-redis` | `tls` (`rediss://`) |
| `quench-cache` | `request-cache`, `data-cache`, `tls`; `redis` is off by default |
| `quench-db` | `postgres`, `tls` |
| `quench-auth` | `http`, `redis`, `postgres`, `tls` |
| `quench-crypto` | `argon2`, `ed25519`, `rsa`, `jwt`, `totp`, `aead`, `x25519`, `age` |
| `quench-cli` | `tui`, `repl` |
| `quench-serde` | `toml` (default), `yaml` (off by default) |
| `quench-html` | `parse`, `markdown` (both default) |
| `quench-convert` | `csv`, `xml`, `axml`, `gguf` (all default) |

Dropping every `tls` feature removes `rustls`, `ring` (unless `quench-crypto` still needs it for hashing) and `webpki-roots` from a build: `quench-http` goes from 25 to 17 third-party crates, `quench-client` from 25 to 15, and `quench-db` without `postgres` has 9. `scripts/features.fish` lints every crate with its features off and on one at a time (`-t` also runs the tests).

## Examples

- [Example: Basic](./examples/basic.md) — smallest possible Quench web app, a smoke test for the UI framework
- [Example: Db](./examples/db_example.md) — `quench-db` migration runner and CRUD repository against Postgres
- [Example: Http Demo](./examples/http_demo.md) — `quench-http` feature tour: routing, DI, middleware, extractors, multipart, body limits, graceful shutdown
- [Example: Forge Service Demo](./examples/forge_service_demo.md) — `quench-starter` + `quench-auth` on `quench-http`, the template for migrating a real service

## Build

```bash
cargo build --release
```

## Tests

```bash
cargo test --workspace
```

The Redis-backed cache tests are skipped unless `CACHE_TEST_REDIS_URL` is set;
the cluster ones want `CACHE_TEST_REDIS_CLUSTER_URL` with comma-separated seeds.

## Project Structure

```text
.
├── quench-archive/         # tar, zip, deflate, gzip, zstd
├── quench-template/        # Jinja-compatible template engine
├── quench-pdf/             # PDF text extraction
├── quench-convert/         # CSV, XML, AXML, GGUF readers
├── quench-html/            # HTML parser, selectors, Markdown renderer
├── quench-test/            # test harness, reports, command runner, mock server
├── quench-test-macros/     # #[quench_test::test]
├── quench-test-bdd/        # Gherkin runner
├── quench-test-bdd-macros/ # #[given]/#[when]/#[then], #[derive(World)]
├── quench-args/            # command-line argument parsing
├── quench-args-macros/     # its derive macros
├── quench-auth/            # relying-party token/session verification
├── quench-cache/           # in-process/Redis caching layer
├── quench-cli/             # shared terminal UI styling and REPL line editor
├── quench-core/            # time, uuid, base64, errors, env, bytes, streams, registry
├── quench-core-macros/     # #[async_trait] and #[derive(Error)]
├── quench-crypto/          # hashes, HMAC, Ed25519, RSA verify, PBKDF2, Argon2id, JWT
├── quench-client/          # authenticated HTTP client wrappers
├── quench-config/          # typed config/env loading
├── quench-db/              # ORM/CRUD abstraction plus migrations
├── quench-http/            # lightweight poem-shaped HTTP framework
├── quench-http-macros/     # #[get]/#[post]/#[injectable]/... proc macros for quench-http
├── quench-log/             # logging: events, spans, console + rolling file
├── quench-log-macros/      # #[instrument]
├── quench-mail/            # MIME message builder + async SMTP submission client
├── quench-pg/              # PostgreSQL client
├── quench-proto/           # HTTP types, HTTP/1.1, multipart, cookies, WebSocket, SSE
├── quench-redis/
├── quench-regex/           # regular expressions
├── quench-k8s/             # Kubernetes API client
├── quench-schema/          # JSON Schema validation
           # Redis client
├── quench-serde/           # JSON, urlencoded, Value, Serialize/Deserialize; features toml, yaml
├── quench-serde-macros/    # derives for quench-serde
├── quench-starter/         # service bootstrap
├── quench-qr/              # QR code encoder
├── quench-tls/             # rustls glue and trust stores
├── quench-style/           # shared tokens, themes, base CSS, i18n
├── quench-fullstack/       # serves quench-ui apps from quench-http: SSR, hashed assets, CSP
├── quench-ui/              # UI core: signals, views, events, renderer interface
├── quench-ui-web/          # runs quench-ui in the browser: wasm glue, quench.js, DOM renderer
├── quench-web/             # server-rendered HTML/CSS/JS page builder
├── quench-web-components/  # UI components built on quench-web
├── examples/               # runnable references: basic, db_example, http_demo, forge_service_demo
└── docs/                   # this documentation tree
```
