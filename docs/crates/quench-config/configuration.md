---
title: Configuration
parent: quench-config
grand_parent: Crates
nav_order: 3
---

The crate reads no fixed environment variable of its own except through `AppConfig::from_env`. The prefix and the key names are supplied by the caller.

## Lookup order

`ConfigLoader::new("P").env_string("KEY", d)`: `P_KEY`, then `KEY`, then `d`.

## `AppConfig::from_env`

| Field | Variables (first set wins) | Default |
|---|---|---|
| `service_name` | `APP_SERVICE_NAME`, `SERVICE_NAME` | `unknown` |
| `port` | `APP_PORT`, `PORT` | `8080` (also when the value is not a number or does not fit `u16`) |
| `database_url` | `APP_DATABASE_URL`, `DATABASE_URL` | empty string |
| `log_level` | `APP_LOG_LEVEL`, `LOG_LEVEL` | `info` |
| `debug` | `APP_DEBUG`, `DEBUG` | `false` (true only for `true`, `1`, `yes`, `on`, case-insensitive) |

## File formats

| Extension (case-insensitive) | Reader |
|---|---|
| `json` | `quench_serde::json` |
| `yaml`, `yml` | `quench_serde::yaml` (YAML 1.2, see [quench-serde](../quench-serde/api-formats.md#yaml-feature-yaml)) |
| `toml` | `quench_serde::toml` (TOML 1.0) |

Environment fallback in `load_with_fallback` expects the whole configuration as one JSON document in the variable.
