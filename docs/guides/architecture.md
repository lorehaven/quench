---
title: Architecture
parent: Guides
nav_order: 2
---

How the crates depend on each other. The edges below are the **normal and build dependencies between workspace crates** read from each `Cargo.toml` (`cargo metadata --no-deps --all-features`, 2026-10-30). Dev-dependencies are listed separately. The levels are computed: a crate's level is one more than the highest level among its workspace dependencies.

## Layers

| Level | Crates |
|---|---|
| 0, no workspace dependency | `quench-core-macros`, `quench-serde-macros`, `quench-log-macros`, `quench-http-macros`, `quench-args-macros`, `quench-fullstack-macros`, `quench-ui-web-macros`, `quench-test-macros`, `quench-test-bdd-macros`, `quench-archive`, `quench-css`, `quench-html`, `quench-qr`, `quench-raster`, `quench-regex`, `quench-ui`, `quench-security` |
| 1 | `quench-core` (-> core-macros), `quench-args`, `quench-image`, `quench-pdf`, `quench-text`, `quench-desktop-wayland` |
| 2 | `quench-serde` (-> core, serde-macros), `quench-tz`, `quench-tls`, `quench-cli`, `quench-layout`, `quench-svg` |
| 3 | `quench-log`, `quench-config`, `quench-crypto`, `quench-test`, `quench-schema`, `quench-template`, `quench-convert`, `quench-style`, `quench-redis`, `quench-mail`, `quench-paint`, `quench-ui-web`, `quench-android`, `quench-desktop` |
| 4 | `quench-proto`, `quench-pg`, `quench-cache`, `quench-web`, `quench-ui-native`, `quench-test-bdd` |
| 5 | `quench-http`, `quench-client`, `quench-db`, `quench-web-components`, `quench-desktop-atspi`, `quench-android-surface` |
| 6 | `quench-auth`, `quench-k8s`, `quench-desktop-native`, `quench-tool` |
| 7 | `quench-starter`, `quench-fullstack` |

## The foundation

```text
quench-core-macros ──> quench-core ──> quench-serde <── quench-serde-macros
                          │                │
                          ├────────────────┴──> quench-log <── quench-log-macros
                          ├──> quench-tz
                          └──> quench-config (also needs quench-serde with feature "yaml")
```

* `quench-core` is the root: only `quench-core-macros` and, natively, `getrandom` below it. Almost every other crate depends on it (time, errors, env, uuid, base64).
* `quench-serde` depends on `quench-core` and its derive crate. It is the data model every format and protocol crate shares.
* `quench-log` depends on `quench-core` (time, `Error`), `quench-serde` (JSON lines) and `quench-log-macros`.
* `quench-config` depends on `quench-core` and `quench-serde` (feature `yaml` enabled).
* `quench-tz` depends on `quench-core` only.
* `quench-security` has no regular dependencies; its dev-dependencies are `quench-test` and `quench-core`.

## Domains

| Domain | Crates and notable edges |
|---|---|
| Network and TLS | `quench-tls` -> core; `quench-proto` -> core, crypto, tls; `quench-http` -> core, http-macros, log, proto, serde; `quench-client` -> core, log, proto, serde; `quench-redis` -> core, tls; `quench-pg` -> core, crypto, log, serde, tls; `quench-mail` -> core, tls |
| Security | `quench-crypto` -> core, serde (third party: `ring`); `quench-auth` -> cache, client, core, crypto, db, http, log, proto, serde |
| Data | `quench-db` -> core, crypto, log, pg, serde; `quench-cache` -> core, redis, serde; `quench-schema` -> regex, serde; `quench-convert`, `quench-template` -> serde |
| Services | `quench-starter` -> auth, cli, client, core, db, http, log, serde, web; `quench-k8s` -> client, core, serde; `quench-fullstack` -> archive, auth, core, crypto, fullstack-macros, http, log, serde, ui, ui-web |
| Web pages | `quench-web` -> core, style; `quench-web-components` -> web; `quench-style` -> core, serde |
| UI | `quench-ui` (no workspace deps); `quench-ui-web` -> serde, ui, ui-web-macros; native stack: `quench-css`, `quench-raster`, `quench-text` -> raster, `quench-layout` -> css, text, `quench-paint` -> css, layout, raster, `quench-svg` -> css, image, raster, `quench-ui-native` -> core, css, image, layout, paint, raster, svg, ui |
| Hosts | `quench-desktop` -> core, serde; `quench-desktop-wayland` -> raster (+ `libc`); `quench-desktop-atspi` -> ui-native; `quench-desktop-native` -> atspi, wayland, ui, ui-native; `quench-android` -> core, serde, tz; `quench-android-surface` -> raster, serde, ui, ui-native |
| Files and formats | `quench-archive`, `quench-html`, `quench-qr`, `quench-regex` (no deps); `quench-image` -> archive; `quench-pdf` -> archive |
| Testing | `quench-test` -> core, regex, serde, test-macros (+ `tokio`); `quench-test-bdd` -> args, core, regex, test, test-bdd-macros |
| Tooling | `quench-args` -> args-macros; `quench-cli` -> core; `quench-tool` -> archive, args, client, crypto, serde, template |

## Dev-dependency edges

Almost every crate has `quench-test` as a dev-dependency. This creates a cycle that Cargo accepts because dev-dependencies are not part of the normal graph: `quench-test` depends on `quench-core`, while `quench-core` dev-depends on `quench-test`. Other dev-only edges: `quench-cache` (core, redis), `quench-client` (http), `quench-k8s` (http), `quench-http` (proto), `quench-fullstack` (proto), `quench-css` (style), `quench-desktop-native` (raster), `quench-desktop-wayland` (core), `quench-text` (core), `quench-ui-native` (style), `quench-android-surface` (css), `quench-security` (core).

## Third-party edges

Only nine third-party crates are direct dependencies of any workspace crate: `getrandom` (core), `libc` (desktop-wayland), `proc-macro2`, `quote`, `syn` (all `-macros` crates), `ring` (crypto), `rustls` (http, tls), `tokio` (async crates) and `webpki-roots` (tls). See [Dependency policy](dependency-policy.md).

## Targets

The pure crates build for `wasm32-unknown-unknown`: `quench-core` (except randomness and filesystem helpers), `quench-serde`, `quench-ui`, `quench-ui-web`. The browser apps use the `wasm` profile described in [Getting started](getting-started.md).

## Examples

The eight crates under `examples/` are leaves that combine the above (for instance `quench-example-forge-service` -> auth, cache, core, http, log, serde, starter). They are not part of the library layering.
