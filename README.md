# Quench

A modular Rust workspace of shared library crates: auth/session verification, an ORM/migration layer, a lightweight HTTP framework and its service bootstrap, a dependency-light HTML/CSS/JS page builder, caching, an authenticated HTTP client, and config loading. Split out of [Forge](https://github.com/lorehaven/forge), which remains the primary consumer alongside other services.

See [docs/](./docs/) for per-crate reference.

## Crates

- [Quench Auth](./docs/crates/quench-auth/index.md) — relying-party token/session verification
- [Quench Http](./docs/crates/quench-http/index.md) — lightweight, `poem`-shaped HTTP framework: annotation-discovered routing, dependency injection, plain-async middleware
- [Quench Starter](./docs/crates/quench-starter/index.md) — service bootstrap (TLS, base-path scoping, health, DB wiring)
- [Quench Web](./docs/crates/quench-web/index.md) — dependency-light server-rendered HTML/CSS/JS page builder
- [Quench Web Components](./docs/crates/quench-web-components/index.md) — higher-level UI builders on top of Quench Web
- [Quench DB](./docs/crates/quench-db/index.md) — ORM/CRUD abstraction plus a migration catalog engine
- [Quench Cache](./docs/crates/quench-cache/index.md) — shared in-process/Redis caching layer
- [Quench Client](./docs/crates/quench-client/index.md) — shared authenticated HTTP client wrappers
- [Quench Config](./docs/crates/quench-config/index.md) — typed config/env loading helper
- [Quench Serde](./docs/crates/quench-serde/index.md) — JSON/TOML/urlencoded codecs and a dynamic `Value` model, without serde
- [Quench Crypto](./docs/crates/quench-crypto/index.md) — hashing, HMAC, Ed25519, RSA verify, PBKDF2, Argon2id and JWT
- [Quench Serde YAML](./docs/crates/quench-serde/api-formats.md) — the `yaml` feature of Quench Serde: YAML 1.2 on `Value`
- [Quench Core Macros](./docs/crates/quench-core-macros/index.md) — `#[async_trait]` and `#[derive(Error)]` for Quench Core
- [Quench Serde Macros](./docs/crates/quench-serde-macros/index.md) — `#[derive(Serialize, Deserialize)]` for Quench Serde
- [Quench Core](./docs/crates/quench-core/index.md) — dependency-free foundations: time, uuid, base64/hex, errors, env, sharded map, versions, bytes
- [Quench Proto](./docs/crates/quench-proto/index.md) — HTTP types, an HTTP/1.1 codec for servers and clients, multipart, cookies, WebSocket and SSE
- [Quench QR](./docs/crates/quench-qr/index.md) — QR code encoder (ISO 18004, versions 1-40) with SVG output, no dependencies
- [Quench desktop Wayland](./docs/crates/quench-desktop-wayland/index.md) — a Wayland client on the wire protocol: window, shared-memory frames, pointer input
- [Quench CSS](./docs/crates/quench-css/index.md) — a CSS engine: parser with nesting, selectors, the cascade, custom properties, calc() and media queries
- [Quench Layout](./docs/crates/quench-layout/index.md) — box layout: block and inline flow, flexbox, positioning, margin collapsing
- [Quench Paint](./docs/crates/quench-paint/index.md) — paints a laid out box tree: backgrounds, borders, radii, shadows, clipping, opacity, text
- [Quench Image](./docs/crates/quench-image/index.md) — PNG decoding (every colour type, Adam7) and encoding, no dependencies
- [Quench SVG](./docs/crates/quench-svg/index.md) — an SVG renderer: shapes, paths with arcs, transforms, fills and strokes into RGBA
- [Quench UI Native](./docs/crates/quench-ui-native/index.md) — a quench-ui renderer that draws itself: DOM, style, layout and paint with no WebView
- [Quench Desktop Native](./docs/crates/quench-desktop-native/index.md) — runs a quench-ui-native renderer in a Wayland window: input, repeat, caret, input method, accessibility
- [Quench Desktop AT-SPI](./docs/crates/quench-desktop-atspi/index.md) — screen-reader support on Linux: the D-Bus protocol by hand and the AT-SPI objects
- [Quench Android Surface](./docs/crates/quench-android-surface/index.md) — an Android app drawn by the native renderer: the program side, the protocol, the Kotlin host plugin
- [Quench Text](./docs/crates/quench-text/index.md) — a TrueType parser, kerning, line breaking and Noto Sans embedded, drawn through quench-raster
- [Quench Raster](./docs/crates/quench-raster/index.md) — software rasteriser: anti-aliased paths, rounded rectangles, premultiplied BGRA canvas, no dependencies
- [Quench HTML](./docs/crates/quench-html/index.md) — HTML parser with CSS selectors, and CommonMark to HTML
- [Quench Test](./docs/crates/quench-test/index.md) — test harness with JSON/HTML result pages, command runner, predicates, mock HTTP server
- [Quench Test BDD](./docs/crates/quench-test-bdd/index.md) — Gherkin runner with cucumber expressions, async steps, concurrent scenarios, JSON/HTML pages
- [Quench Convert](./docs/crates/quench-convert/index.md) — CSV, XML, Android binary XML and GGUF as quench-serde values
- [Quench PDF](./docs/crates/quench-pdf/index.md) — PDF text extraction (xref tables and streams, object streams, fonts, encodings)
- [Quench Template](./docs/crates/quench-template/index.md) — Jinja-compatible templates over quench-serde values (filters, tests, macros, imports)
- [Quench Archive](./docs/crates/quench-archive/index.md) — tar, zip, deflate, gzip and zstd (decoder and encoder), no dependencies
- [Quench Args](./docs/crates/quench-args/index.md) — command-line parsing with clap-compatible derives, no clap
- [Quench K8s](./docs/crates/quench-k8s/index.md) — generic Kubernetes API client (kubeconfig, dynamic resources, discovery, logs, watch)
- [Quench Schema](./docs/crates/quench-schema/index.md) — JSON Schema validation over quench-serde values
- [Quench Regex](./docs/crates/quench-regex/index.md) — regular expressions with Unicode classes, no dependencies
- [Quench Style](./docs/crates/quench-style/index.md) — shared design tokens, themes, base CSS and i18n for web, mobile and desktop
- [Quench TZ](./docs/crates/quench-tz/index.md) — IANA time zones with the database built in
- [Quench Android host](./docs/crates/quench-android/index.md) — the WebView app that runs a Quench UI bundle on Android, with a plugin bridge
- [Quench desktop host](./docs/crates/quench-desktop/index.md) — a GTK window around webkit2gtk that runs a Quench UI bundle on Linux, with the same bridge
- [Quench Tool](./docs/crates/quench-tool/index.md) — the `quench` command: new projects, a dev loop with live reload, release builds, doctor
- [Quench UI](./docs/crates/quench-ui/index.md) — UI core: signals, views, events and the renderer interface
- [Quench Fullstack](./docs/crates/quench-fullstack/index.md) — serves Quench UI apps from Quench HTTP: server rendering, hashed assets, strict CSP, typed server functions, sessions
- [Quench Fullstack Macros](./docs/crates/quench-fullstack-macros/index.md) — the `#[server]` attribute
- [Quench UI Web Macros](./docs/crates/quench-ui-web-macros/index.md) — the `#[plugin]` attribute: a typed client for a native plugin of the host app
- [Quench UI Web](./docs/crates/quench-ui-web/index.md) — runs a Quench UI app in the browser from WebAssembly, with a small JS bridge
- [Quench TLS](./docs/crates/quench-tls/index.md) — async glue between tokio streams and rustls, with shared trust-store helpers
- [Quench Log](./docs/crates/quench-log/index.md) — leveled events, spans, console and rolling-file output
- [Quench Log Macros](./docs/crates/quench-log-macros/index.md) — `#[instrument]` for Quench Log
- [Quench PG](./docs/crates/quench-pg/index.md) — PostgreSQL client: wire protocol, SCRAM-SHA-256, TLS, typed rows, pool
- [Quench Redis](./docs/crates/quench-redis/index.md) — Redis client: RESP2/3, pooled connections, cluster routing
- [Quench Mail](./docs/crates/quench-mail/index.md) — MIME message builder and async SMTP submission client
- [Quench CLI](./docs/crates/quench-cli/index.md) — shared terminal UI styling for CLI tools

Most crates have cargo features for the heavier optional parts (TLS, the Postgres backend, the HTTP layer of `quench-auth`, the REPL editor, ...); see each crate's feature table in the [crate reference](./docs/crates/index.md).

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

## License
MIT
