---
title: Configuration
parent: quench-pg
grand_parent: Crates
nav_order: 7
---

`quench-pg` reads no environment variables and no files by itself, except the PEM file named by `sslrootcert` in a URL. Everything is in `Config` and `PoolOptions`.

## Connection URL

`postgres://[user[:password]@][host][:port][/database][?param=value&...]`; the scheme may be `postgresql://`. Parsing rules: [Config::from_url](api-connection.md#configfrom_url).

| Query parameter | Type | Default | Effect |
|---|---|---|---|
| `sslmode` | `disable`, `allow`, `prefer`, `require`, `verify-ca`, `verify-full` | `prefer` | see below |
| `sslrootcert` | path to a PEM file | none | extra trusted roots (used with `verify-*`); the file is read when the URL is parsed; may repeat |
| `application_name` | text | none | startup parameter |
| `options` | text | none | startup parameter `options` (for example `-c search_path=app`, percent-encoded) |
| `connect_timeout` | whole seconds | `10` | bound for transport + TLS + authentication |
| `statement_timeout` | whole milliseconds | none (no limit) | client-side limit per statement; the connection is dropped on expiry |

Unknown parameters are ignored.

| `sslmode` | `SslMode` | Behaviour |
|---|---|---|
| `disable` | `Disable` | plain text |
| `prefer`, `allow` | `Prefer` (default) | send an `SSLRequest`; if the server says `N`, continue in plain text; the certificate is not verified |
| `require` | `Require` | TLS or fail; certificate not verified |
| `verify-full`, `verify-ca` | `VerifyFull` | TLS or fail; chain and server name verified against the webpki roots, the operating system's CA bundle and `sslrootcert` |

Unix sockets (host starting with `/`) never use TLS. Use `require` or stronger on any network you do not control: `prefer` can be downgraded by an active attacker.

## `Config` fields and defaults

See the table in [api-connection](api-connection.md#struct-config). Defaults: `localhost:5432`, user `postgres`, no password, database = user name (sent as the `database` startup parameter), `Prefer`, `connect_timeout` 10 s, no statement timeout, `max_message_size` 268 435 456 bytes (256 MiB).

Session settings the client always sends: `client_encoding=UTF8`, `TimeZone=UTC`, `DateStyle=ISO, MDY`. Result conversion depends on them: do not change them with `options` or `SET`.

## `PoolOptions`

| Field | Default | Valid values |
|---|---|---|
| `max_size` | 5 | any `usize`; 0 behaves as 1 |
| `acquire_timeout` | 30 s | any `Duration` |
| `idle_timeout` | 600 s | |
| `max_lifetime` | 1800 s | |
| `test_after_idle` | 30 s | |

## Cargo features

`tls` (default). See the [overview](index.md#features).
