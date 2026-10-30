---
title: Testing
parent: quench-db
grand_parent: Crates
nav_order: 11
---

## How the crate is tested

`Cargo.toml` sets `autotests = false` and declares five `[[test]]` targets, all with `harness = false` (they use `quench-test`'s own runner, `#[quench_test::test]`):

| Target | File | Needs Postgres | Covers |
|---|---|---|---|
| `unit` | `tests/unit.rs` (modules in `tests/unit/`) | no | `Db`/`checked_column` dispatch, in-memory CRUD, `DbError` mapping, migration rendering and loader, catalog loading and manifest rules, runner builder, identifier validation, lock key |
| `catalog_plan` | `tests/catalog_plan.rs` | no | catalog loading and plan resolution end to end: order, schema inheritance, version gating, variables, reproducibility, cycles, unknown variables |
| `catalog_errors` | `tests/catalog_errors.rs` | no | the ways a catalog or plan can be wrong, and their messages |
| `postgres_backend` | `tests/postgres_backend.rs` (feature `postgres`) | yes | CRUD, numeric keys, `jsonb`/array/timestamp columns, error mapping, `migrate` once-only and rollback |
| `postgres_runner` | `tests/postgres_runner.rs` (feature `postgres`) | yes | dry run, idempotent apply, reset, drift |

Run the database-free targets with `cargo test -p quench-db`. The two Postgres targets return early (pass) unless `DB_TEST_POSTGRES_URL` is set to a non-empty value. Use a throwaway database: the tests create and drop tables (`quench_db_test_widgets` and others) and set `DB_MIGRATION_TABLE`/`DB_POOL_MAX_SIZE` in the process. The repository's `foreman db` starts a disposable Postgres and `scripts/test-env.fish` exports the URL; do not point the tests at a database you care about.

## Testing code built on quench-db

* Use `Db::connect("")` for an in-memory `Db`, then `db.repository::<T>()`. Remember the [differences](internals.md#backend-differences).
* For catalog logic, create a directory tree in a `quench_core::tmp::TempDir` (as `tests/catalog_plan.rs` does with its `module()` helper), call `Catalog::load` and `MigrationPlan::resolve`, and inspect `plan.modules` and `plan.migrations[..].statements`. This needs no database.
* For the runner, `MigrationRunner::new().dry_run(true).apply(..)` still needs a reachable Postgres (it reads the ledger) but writes nothing.
