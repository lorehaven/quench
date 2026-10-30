---
title: Configuration
parent: quench-redis
grand_parent: Crates
nav_order: 5
---

`quench-redis` reads no environment variables and no files. It is configured by a URL (`Config::from_url`), by building `Config` and `PoolOptions` values directly, and by one Cargo feature (`tls`). Callers such as `quench-cache` read `CACHE_URL`/`REDIS_URL` themselves and pass the URL on.

## URL format

```text
redis://[[user]:password@]host[:port][/db][?connect_timeout=secs&command_timeout=millis]
rediss://...                      TLS
rediss://...#insecure             TLS without certificate checks (development only)
```

Examples (all accepted by the tests): `redis://localhost`, `redis://:secret@/2`, `redis://[::1]:7000`, `rediss://app:p%40ss@cache.example.com:6380/3?connect_timeout=2&command_timeout=1500#insecure`. The full parsing rules are in [API: configuration, commands, connection](api-config-and-commands.md#config).

## Defaults and ranges

| Setting | Where | Default | Valid values |
|---|---|---|---|
| host | `Config.host` | `127.0.0.1` | any; IPv6 in brackets in URLs |
| port | `Config.port` | `6379` | `0..=65535` |
| username / password | `Config.username/password` | none | percent-encoded in URLs |
| database | `Config.db` | `0` | `u32`; `SELECT` sent when not 0; ignored on clusters |
| TLS | `Config.tls` | off | `rediss://` |
| insecure TLS | `Config.tls_insecure` | off | only with `rediss://` and `#insecure` |
| connect timeout | `Config.connect_timeout` | 5 s | URL: whole seconds (`u64`) |
| command timeout | `Config.command_timeout` | 30 s | URL: milliseconds (`u64`); `None` (struct only) disables |
| pool size | `PoolOptions.max_size` | 8 | `usize`; 0 acts as 1 |
| acquire timeout | `PoolOptions.acquire_timeout` | 10 s | `Duration` |
| idle timeout | `PoolOptions.idle_timeout` | 300 s | `Duration` |
| cluster redirect limit | constant `MAX_REDIRECTS` | 5 | not configurable |

There is no URL syntax for pool options; use `Client::connect_with` / `ClusterClient::connect_with`.

## TLS

With the `tls` feature, `rediss://` builds a `quench_tls::ClientOptions` with `accept_invalid_certs = config.tls_insecure` and `use_system_roots = true` (other fields default), and verifies the server name against `config.host`. Client certificates are not supported. Without the feature, a TLS configuration fails with `Error::Tls`.

## Cluster

Credentials, TLS settings and timeouts come from the first seed URL for all nodes. Each URL contributes its `host:port` as a seed address. See [API: client, pool, cluster](api-client.md#clusterclient).
