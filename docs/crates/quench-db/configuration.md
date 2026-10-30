---
title: Configuration
parent: quench-db
grand_parent: Crates
nav_order: 8
---

`quench-db` reads two environment variables, both in the Postgres backend (`src/backends/postgres.rs`) through `quench_core::env::var` (unset or non-UTF-8 text counts as unset). Everything else is passed in code.

## Environment variables

| Variable | Default | Read when | Effect |
|---|---|---|---|
| `DB_POOL_MAX_SIZE` | `5` | each `PostgresDb::new` (so each `Db::connect` with a URL) | `PoolOptions::max_size` of the connection pool. Parsed as `usize`; text that does not parse silently falls back to `5`. `0` is accepted and the pool then clamps it to 1 (`Pool::lazy` uses `max_size.max(1)`). |
| `DB_MIGRATION_TABLE` | `quench_migrations` | each `Database::migrate` call on `PostgresDb` | Name of the ledger table used by the simple `migrate` path. Interpolated into SQL as given: not validated, not quoted; may be schema-qualified. Does not affect the catalog runner. |

All other pool options (`acquire_timeout` 30 s, `idle_timeout` 10 min, `max_lifetime` 30 min, `test_after_idle` 30 s) keep `quench_pg::PoolOptions::default()`; they are not configurable through `quench-db` (build your own `quench_pg::Pool` if you need that; `PostgresDb` has no constructor from a pool).

The connection URL (`postgres://user:password@host:port/database?sslmode=...&connect_timeout=...&statement_timeout=...`) is handled entirely by `quench_pg::Config::from_url`; see [quench-pg configuration](../quench-pg/configuration.md).

## Builder options

| Type | Option | Default |
|---|---|---|
| `MigrationRunner` | `ledger_schema(..)` | `"foundry"` |
| | `ledger_table(..)` | `"forge_migrations"` |
| | `module_table(..)` | `"forge_modules"` |
| | `allow_drift(bool)` | `false` |
| | `dry_run(bool)` | `false` |

Valid identifier values: ASCII letters, digits and `_`, not starting with a digit.

## Files

* `<catalog root>/<module>/module.toml` and `<module>/*.toml` migration files; the formats are in [catalog](api-catalog.md) and [migration files](api-migrations.md).

## Cargo features

`postgres` and `tls` (both default); see the [overview](index.md#features).
