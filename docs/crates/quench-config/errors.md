---
title: Errors
parent: quench-config
grand_parent: Crates
nav_order: 4
---

| Variant | When | Message |
|---|---|---|
| `ReadError(String)` | the file cannot be read | `Failed to read config file: {path}: {io error}` |
| `ParseError(String)` | content does not parse into `T` | `Failed to parse config: JSON parse error: {quench_serde error}` (or `YAML` / `TOML`). The inner message can carry a data path and line/column ([quench-serde errors](../quench-serde/errors.md)) |
| `MissingValue(String)` | `load_with_fallback`: no file, no usable environment variable, no default | `Missing required config value: No config found in file or env var {env_var}` |
| `InvalidValue(String)` | never produced by this crate | `Invalid config value: {0}` |

`ConfigError` implements `Debug`, `Display` and `std::error::Error`; it does not carry the underlying error as a `source`.
