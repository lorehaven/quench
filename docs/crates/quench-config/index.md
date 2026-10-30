---
title: quench-config
parent: Crates
nav_order: 10
has_children: true
permalink: /crates/quench-config/
---

`quench-config` (library `quench_config`) is a small shared helper for loading configuration: JSON, YAML and TOML files and typed environment variables through one API, instead of each service hand-rolling `std::env::var` plus `.parse()`.

| | |
|---|---|
| Version | 0.1.13 |
| Edition | 2024 (workspace) |
| Features | none (it enables `quench-serde`'s `yaml` feature internally) |
| Source | one file, `src/lib.rs` (151 lines) |

```toml
[dependencies]
quench-config = { version = "0.1", registry = "ennor" }
```

## Dependencies

| Dependency | Why |
|---|---|
| `quench-core` | `#[derive(Error)]`, `env::var` |
| `quench-serde` (features `["yaml"]`) | JSON, YAML and TOML parsing; `DeserializeOwned`, `Serialize` bounds; the derives for `AppConfig` |
| `quench-test` (dev) | harness |

## Minimal example

```rust
use quench_config::ConfigLoader;

#[derive(quench_serde::Deserialize)]
struct Permissions { admins: Vec<String> }

fn main() -> quench_config::Result<()> {
    let loader = ConfigLoader::new("WAREHOUSE");
    let max_upload_mb = loader.env_u64("MAX_UPLOAD_MB", 100); // WAREHOUSE_MAX_UPLOAD_MB, then MAX_UPLOAD_MB
    let debug = loader.env_bool("DEBUG", false);
    let perms: Permissions = ConfigLoader::from_toml_file("config/permissions.toml")?;
    println!("{max_upload_mb} {debug} {:?}", perms.admins);
    Ok(())
}
```

## Where to go next

* [Usage](usage.md), [API](api.md), [Configuration](configuration.md), [Errors](errors.md), [Testing](testing.md).
