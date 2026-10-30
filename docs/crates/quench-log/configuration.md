---
title: Configuration
parent: quench-log
grand_parent: Crates
nav_order: 3
---

## `Config` defaults

`Config::default()` (and what `from_env()` falls back to for anything unset or unparseable):

| Field | Type | Default |
|---|---|---|
| `console_enabled` | `bool` | `true` |
| `file_enabled` | `bool` | `true` |
| `level` | `String` | `"info"` |
| `file_dir` | `PathBuf` | `logs` |
| `file_prefix` | `String` | `"app"` |
| `file_format` | `Format` | `Format::Json` |
| `rotation` | `Rotation` | `Rotation::Daily` |
| `max_files` | `usize` | `7` (`0` = unlimited) |

## Environment variables (`Config::from_env`)

A variable that is set but blank (whitespace only) counts as unset.

| Variable | Maps to | Accepted values |
|---|---|---|
| `LOG_CONSOLE_ENABLED` | `console_enabled` | `1 true yes on` / `0 false no off`, case-insensitive (`config::parse_bool`); other text keeps the default |
| `LOG_FILE_ENABLED` | `file_enabled` | same |
| `RUST_LOG` | `level` | a filter directive string (below) |
| `LOG_DIR` | `file_dir` | a path |
| `LOG_FILE_PREFIX` | `file_prefix` | text; falls back to `CARGO_BIN_NAME`, then `"app"` (a blank explicit value counts as absent) |
| `CARGO_BIN_NAME` | `file_prefix` fallback | set by Cargo for a running workspace binary |
| `LOG_FILE_FORMAT` | `file_format` | `json`, `plain` or `text` (case-insensitive) |
| `LOG_ROTATION` | `rotation` | `daily`, `hourly`, `never` |
| `LOG_MAX_FILES` | `max_files` | a non-negative integer |

The console is always plain text; only the file format is a choice. The level applies to both sinks.

## Filter syntax

`RUST_LOG` style, comma separated: `info`, `warn,quench_http=debug`, `info,hyper=off`.

* A bare level (`trace`, `debug`, `info`, `warn`, `warning`, `error`, or `5 4 3 2 1`, case-insensitive) sets the default for all targets; `off` or `0` disables.
* `target=level` applies to that module path and everything beneath it (`target == prefix` or `target` starts with `prefix::`); the longest matching target wins.
* A bare word that is not a level and looks like a target (`[A-Za-z0-9_:-]+`) turns that target fully on (`trace`).
* Anything after a `[` in a directive is dropped (span directives are not supported).
* With directives but no bare level, the default is `error`.
* An empty string, or text that cannot be parsed, makes `Filter::parse` fail; `subscriber` then falls back to `Filter::level(Level::Info)`.

## Files

`RollingWriter` names files `<prefix>.<period>.log`, or `<prefix>.log` for `Rotation::Never`. The period is the UTC date `YYYY-MM-DD` (daily) or `YYYY-MM-DD-HH` (hourly). On each roll the oldest files beyond `max_files` (those whose names start with `<prefix>.` and end with `.log`, sorted by name) are deleted; nothing is pruned for `max_files = 0` or `Never`. The directory `file_dir` is created (`create_dir_all`) when the file sink is enabled.

Queue: `QUEUE = 128_000` lines between the logging threads and the writer thread (`quench-log-writer`); when full, new lines are dropped and counted (`NonBlocking::dropped`).
