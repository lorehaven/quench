# Quench

Quench is a Rust framework built from small crates that each do one job with as few dependencies as possible: serialization, regular expressions, HTTP, TLS-aware clients, Postgres, Redis and SQLite (database files, with an SQL engine) clients, templates, PDF and image handling, a UI toolkit and the renderers and hosts that run it. Most of what a service or an app usually pulls in from the wider ecosystem is written here instead, so what you build on is readable, uniform and under one roof.

**Documentation: <https://lorehaven.github.io/quench/>** (guides, per-crate reference, examples)

## Purpose

* Replace large, fast-moving dependency trees with a workspace you can read, audit and version together (see the [dependency policy](https://lorehaven.github.io/quench/guides/dependency-policy/)).
* Give services, command-line tools and apps the same conventions: one config loader, one logger, one error style, one test harness.
* Run one UI codebase in the browser (WebAssembly), in a WebView on Android and Linux, or drawn by a native renderer with no WebView at all.

## General use cases

* **HTTP services**: `quench-http` for routing and middleware, `quench-starter` for TLS, health and database wiring, `quench-auth` for session verification, `quench-db` and `quench-pg` for data.
* **Full-stack web apps**: `quench-ui` views and signals served by `quench-fullstack` with server rendering and typed server functions, or compiled to WebAssembly with `quench-ui-web`.
* **Mobile and desktop apps**: the same UI in `quench-android` or `quench-desktop`, or rendered natively with `quench-ui-native`.
* **Command-line tools**: `quench-args` for parsing, `quench-cli` for styling, `quench-tool` to scaffold, run and release projects.
* **Libraries for everyday data work**: JSON/TOML/YAML (`quench-serde`), regex, templates, archives, PDF text, QR codes, PNG and SVG.
* **Testing**: `quench-test` and `quench-test-bdd` produce JSON and HTML result pages; `quench-security` runs the security gate.

New to Quench? Start with the [getting started guide](https://lorehaven.github.io/quench/guides/getting-started/).

## Crates

Every crate has a README in its directory. The book icon (📖) links to the crate's documentation.

### Foundations

| Crate | Description | Docs |
| --- | --- | --- |
| [`quench-core`](./quench-core/) | Dependency-free foundations: time, uuid, base64/hex, errors, env, sharded map, versions, bytes | [📖](https://lorehaven.github.io/quench/crates/quench-core/) |
| [`quench-core-macros`](./quench-core-macros/) | `#[async_trait]` and `#[derive(Error)]` for Quench Core | [📖](https://lorehaven.github.io/quench/crates/quench-core-macros/) |
| [`quench-serde`](./quench-serde/) | JSON/TOML/urlencoded codecs and a dynamic `Value` model, without serde | [📖](https://lorehaven.github.io/quench/crates/quench-serde/) |
| [`quench-serde-macros`](./quench-serde-macros/) | `#[derive(Serialize, Deserialize)]` for Quench Serde | [📖](https://lorehaven.github.io/quench/crates/quench-serde-macros/) |
| [`quench-config`](./quench-config/) | Typed config/env loading helper | [📖](https://lorehaven.github.io/quench/crates/quench-config/) |
| [`quench-log`](./quench-log/) | Leveled events, spans, console and rolling-file output | [📖](https://lorehaven.github.io/quench/crates/quench-log/) |
| [`quench-log-macros`](./quench-log-macros/) | `#[instrument]` for Quench Log | [📖](https://lorehaven.github.io/quench/crates/quench-log-macros/) |
| [`quench-regex`](./quench-regex/) | Regular expressions with Unicode classes, no dependencies | [📖](https://lorehaven.github.io/quench/crates/quench-regex/) |
| [`quench-tz`](./quench-tz/) | IANA time zones with the database built in | [📖](https://lorehaven.github.io/quench/crates/quench-tz/) |
| [`quench-template`](./quench-template/) | Jinja-compatible templates over quench-serde values (filters, tests, macros, imports) | [📖](https://lorehaven.github.io/quench/crates/quench-template/) |
| [`quench-schema`](./quench-schema/) | JSON Schema validation over quench-serde values | [📖](https://lorehaven.github.io/quench/crates/quench-schema/) |

### Web services

| Crate | Description | Docs |
| --- | --- | --- |
| [`quench-proto`](./quench-proto/) | HTTP types, an HTTP/1.1 codec for servers and clients, multipart, cookies, WebSocket and SSE | [📖](https://lorehaven.github.io/quench/crates/quench-proto/) |
| [`quench-http`](./quench-http/) | Lightweight, `poem`-shaped HTTP framework: annotation-discovered routing, dependency injection, plain-async middleware | [📖](https://lorehaven.github.io/quench/crates/quench-http/) |
| [`quench-http-macros`](./quench-http-macros/) | Route and injectable discovery macros for Quench Http | [📖](https://lorehaven.github.io/quench/crates/quench-http-macros/) |
| [`quench-starter`](./quench-starter/) | Service bootstrap (TLS, base-path scoping, health, DB wiring) | [📖](https://lorehaven.github.io/quench/crates/quench-starter/) |
| [`quench-auth`](./quench-auth/) | Relying-party token/session verification | [📖](https://lorehaven.github.io/quench/crates/quench-auth/) |
| [`quench-client`](./quench-client/) | Shared authenticated HTTP client wrappers | [📖](https://lorehaven.github.io/quench/crates/quench-client/) |
| [`quench-tls`](./quench-tls/) | Async glue between tokio streams and rustls, with shared trust-store helpers | [📖](https://lorehaven.github.io/quench/crates/quench-tls/) |
| [`quench-cache`](./quench-cache/) | Shared in-process/Redis caching layer | [📖](https://lorehaven.github.io/quench/crates/quench-cache/) |

### Data and I/O

| Crate | Description | Docs |
| --- | --- | --- |
| [`quench-sqlite`](./quench-sqlite/) | SQLite database files read and written by our own code: pager, b-trees, rollback journal, an SQL engine; no dependencies | [📖](https://lorehaven.github.io/quench/crates/quench-sqlite/) |
| [`quench-db`](./quench-db/) | ORM/CRUD abstraction plus a migration catalog engine | [📖](https://lorehaven.github.io/quench/crates/quench-db/) |
| [`quench-pg`](./quench-pg/) | PostgreSQL client: wire protocol, SCRAM-SHA-256, TLS, typed rows, pool | [📖](https://lorehaven.github.io/quench/crates/quench-pg/) |
| [`quench-redis`](./quench-redis/) | Redis client: RESP2/3, pooled connections, cluster routing | [📖](https://lorehaven.github.io/quench/crates/quench-redis/) |
| [`quench-mail`](./quench-mail/) | MIME message builder and async SMTP submission client | [📖](https://lorehaven.github.io/quench/crates/quench-mail/) |
| [`quench-k8s`](./quench-k8s/) | Generic Kubernetes API client (kubeconfig, dynamic resources, discovery, logs, watch) | [📖](https://lorehaven.github.io/quench/crates/quench-k8s/) |
| [`quench-archive`](./quench-archive/) | Tar, zip, deflate, gzip and zstd (decoder and encoder), no dependencies | [📖](https://lorehaven.github.io/quench/crates/quench-archive/) |
| [`quench-convert`](./quench-convert/) | CSV, XML, Android binary XML and GGUF as quench-serde values | [📖](https://lorehaven.github.io/quench/crates/quench-convert/) |
| [`quench-crypto`](./quench-crypto/) | Hashing, HMAC, Ed25519, RSA verify, PBKDF2, Argon2id and JWT | [📖](https://lorehaven.github.io/quench/crates/quench-crypto/) |

### Documents, images and text

| Crate | Description | Docs |
| --- | --- | --- |
| [`quench-html`](./quench-html/) | HTML parser with CSS selectors, and CommonMark to HTML | [📖](https://lorehaven.github.io/quench/crates/quench-html/) |
| [`quench-pdf`](./quench-pdf/) | PDF text extraction (xref tables and streams, object streams, fonts, encodings) | [📖](https://lorehaven.github.io/quench/crates/quench-pdf/) |
| [`quench-qr`](./quench-qr/) | QR code encoder (ISO 18004, versions 1-40) with SVG output, no dependencies | [📖](https://lorehaven.github.io/quench/crates/quench-qr/) |
| [`quench-image`](./quench-image/) | PNG decoding (every colour type, Adam7) and encoding, no dependencies | [📖](https://lorehaven.github.io/quench/crates/quench-image/) |
| [`quench-svg`](./quench-svg/) | An SVG renderer: shapes, paths with arcs, transforms, fills and strokes into RGBA | [📖](https://lorehaven.github.io/quench/crates/quench-svg/) |
| [`quench-raster`](./quench-raster/) | Software rasteriser: anti-aliased paths, rounded rectangles, premultiplied BGRA canvas, no dependencies | [📖](https://lorehaven.github.io/quench/crates/quench-raster/) |
| [`quench-text`](./quench-text/) | A TrueType parser, kerning, line breaking and Noto Sans embedded, drawn through quench-raster | [📖](https://lorehaven.github.io/quench/crates/quench-text/) |

### User interfaces

| Crate | Description | Docs |
| --- | --- | --- |
| [`quench-ui`](./quench-ui/) | UI core: signals, views, events and the renderer interface | [📖](https://lorehaven.github.io/quench/crates/quench-ui/) |
| [`quench-ui-web`](./quench-ui-web/) | Runs a Quench UI app in the browser from WebAssembly, with a small JS bridge | [📖](https://lorehaven.github.io/quench/crates/quench-ui-web/) |
| [`quench-ui-web-macros`](./quench-ui-web-macros/) | The `#[plugin]` attribute: a typed client for a native plugin of the host app | [📖](https://lorehaven.github.io/quench/crates/quench-ui-web-macros/) |
| [`quench-web`](./quench-web/) | Dependency-light server-rendered HTML/CSS/JS page builder | [📖](https://lorehaven.github.io/quench/crates/quench-web/) |
| [`quench-web-components`](./quench-web-components/) | Higher-level UI builders on top of Quench Web | [📖](https://lorehaven.github.io/quench/crates/quench-web-components/) |
| [`quench-style`](./quench-style/) | Shared design tokens, themes, base CSS and i18n for web, mobile and desktop | [📖](https://lorehaven.github.io/quench/crates/quench-style/) |
| [`quench-fullstack`](./quench-fullstack/) | Serves Quench UI apps from Quench HTTP: server rendering, hashed assets, strict CSP, typed server functions, sessions | [📖](https://lorehaven.github.io/quench/crates/quench-fullstack/) |
| [`quench-fullstack-macros`](./quench-fullstack-macros/) | The `#[server]` attribute | [📖](https://lorehaven.github.io/quench/crates/quench-fullstack-macros/) |

### Native rendering and hosts

| Crate | Description | Docs |
| --- | --- | --- |
| [`quench-css`](./quench-css/) | A CSS engine: parser with nesting, selectors, the cascade, custom properties, calc() and media queries | [📖](https://lorehaven.github.io/quench/crates/quench-css/) |
| [`quench-layout`](./quench-layout/) | Box layout: block and inline flow, flexbox, positioning, margin collapsing | [📖](https://lorehaven.github.io/quench/crates/quench-layout/) |
| [`quench-paint`](./quench-paint/) | Paints a laid out box tree: backgrounds, borders, radii, shadows, clipping, opacity, text | [📖](https://lorehaven.github.io/quench/crates/quench-paint/) |
| [`quench-ui-native`](./quench-ui-native/) | A quench-ui renderer that draws itself: DOM, style, layout and paint with no WebView | [📖](https://lorehaven.github.io/quench/crates/quench-ui-native/) |
| [`quench-desktop`](./quench-desktop/) | A GTK window around webkit2gtk that runs a Quench UI bundle on Linux, with the same bridge | [📖](https://lorehaven.github.io/quench/crates/quench-desktop/) |
| [`quench-desktop-native`](./quench-desktop-native/) | Runs a quench-ui-native renderer in a Wayland window: input, repeat, caret, input method, accessibility | [📖](https://lorehaven.github.io/quench/crates/quench-desktop-native/) |
| [`quench-desktop-wayland`](./quench-desktop-wayland/) | A Wayland client on the wire protocol: window, shared-memory frames, pointer input | [📖](https://lorehaven.github.io/quench/crates/quench-desktop-wayland/) |
| [`quench-desktop-atspi`](./quench-desktop-atspi/) | Screen-reader support on Linux: the D-Bus protocol by hand and the AT-SPI objects | [📖](https://lorehaven.github.io/quench/crates/quench-desktop-atspi/) |
| [`quench-android`](./quench-android/) | The WebView app that runs a Quench UI bundle on Android, with a plugin bridge | [📖](https://lorehaven.github.io/quench/crates/quench-android/) |
| [`quench-android-surface`](./quench-android-surface/) | An Android app drawn by the native renderer: the program side, the protocol, the Kotlin host plugin | [📖](https://lorehaven.github.io/quench/crates/quench-android-surface/) |

### Command line and testing

| Crate | Description | Docs |
| --- | --- | --- |
| [`quench-cli`](./quench-cli/) | Shared terminal UI styling for CLI tools | [📖](https://lorehaven.github.io/quench/crates/quench-cli/) |
| [`quench-args`](./quench-args/) | Command-line parsing with clap-compatible derives, no clap | [📖](https://lorehaven.github.io/quench/crates/quench-args/) |
| [`quench-args-macros`](./quench-args-macros/) | Derive macros for Quench Args (`Parser`, `Args`, `Subcommand`, `ValueEnum`) | [📖](https://lorehaven.github.io/quench/crates/quench-args-macros/) |
| [`quench-tool`](./quench-tool/) | The `quench` command: new projects, a dev loop with live reload, release builds, doctor | [📖](https://lorehaven.github.io/quench/crates/quench-tool/) |
| [`quench-test`](./quench-test/) | Test harness with JSON/HTML result pages, command runner, predicates, mock HTTP server | [📖](https://lorehaven.github.io/quench/crates/quench-test/) |
| [`quench-test-macros`](./quench-test-macros/) | The `#[test]` attribute of Quench Test | [📖](https://lorehaven.github.io/quench/crates/quench-test-macros/) |
| [`quench-test-bdd`](./quench-test-bdd/) | Gherkin runner with cucumber expressions, async steps, concurrent scenarios, JSON/HTML pages | [📖](https://lorehaven.github.io/quench/crates/quench-test-bdd/) |
| [`quench-test-bdd-macros`](./quench-test-bdd-macros/) | The step attributes and `World` derive of Quench Test BDD | [📖](https://lorehaven.github.io/quench/crates/quench-test-bdd-macros/) |
| [`quench-security`](./quench-security/) | The security gate: supply-chain, static and attack suites as one Quench Test report | [📖](https://lorehaven.github.io/quench/crates/quench-security/) |

Most crates have cargo features for the heavier optional parts (TLS, the Postgres backend, the HTTP layer of `quench-auth`, the REPL editor, ...); see each crate's feature table in the [crate reference](https://lorehaven.github.io/quench/crates/).

Quench was split out of [Forge](https://github.com/lorehaven/forge), which remains its primary consumer.

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
