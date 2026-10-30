---
title: quench-serde
parent: Crates
nav_order: 42
has_children: true
permalink: /crates/quench-serde/
---

`quench-serde` (library `quench_serde`) replaces `serde`, `serde_json`, `serde_urlencoded`, `toml` and the YAML crate with one small crate. Types do not implement a visitor protocol: they convert to and from a dynamic `Value`, and every format parses into or prints from that same `Value`.

| | |
|---|---|
| Version | 0.1.7 |
| Edition | 2024 (workspace) |
| Test harness | `harness = false`, `quench_test::main!()` |
| Builds for `wasm32-unknown-unknown` | yes (it depends only on `quench-core` and the derive crate) |

```toml
[dependencies]
quench-serde = { version = "0.1", registry = "ennor" }
# YAML as well:
# quench-serde = { version = "0.1", registry = "ennor", features = ["yaml"] }
```

The derives generate paths of the form `::quench_serde::...`, so the dependency must be named `quench-serde` (no rename) in any crate that uses `#[derive(Serialize, Deserialize)]`.

## Features

| Feature | Default | Enables |
|---|---|---|
| `toml` | yes | module `toml` (TOML 1.0 reader and writer) |
| `yaml` | no | module `yaml` (YAML 1.2 reader and writer); `quench-config` turns it on |

`json`, `urlencoded`, `error`, `Value`, `Map` and the traits are always compiled in.

## Dependencies

| Dependency | Why |
|---|---|
| `quench-core` | `Time`, `Uuid`, `Version`, `VersionReq` impls and `Uuid` map keys |
| `quench-serde-macros` | the `Serialize` / `Deserialize` derives, re-exported |
| `quench-test` (dev) | test harness |

## Modules and root items

| Item | Page |
|---|---|
| `Value`, `ValueIndex`, `Map`, `Error`, `Result`, `json!` | [API: values and traits](api.md) |
| `Serialize`, `Deserialize`, `DeserializeOwned`, `ToKey`, `FromKey`, `to_value`, `from_value`, `invalid_type` | [API: values and traits](api.md) |
| `json`, `urlencoded`, `toml`, `yaml` | [API: formats](api-formats.md) |
| `#[derive(Serialize, Deserialize)]` and `#[serde(..)]` | [quench-serde-macros](../quench-serde-macros/index.md) |

## Minimal example

```rust
use quench_serde::{Deserialize, Serialize, json};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct User {
    name: String,
    #[serde(default)]
    tags: Vec<String>,
    note: Option<String>,
}

fn main() -> quench_serde::Result<()> {
    let user: User = json::from_str(r#"{"name":"ada"}"#)?;
    assert_eq!(user.tags, Vec::<String>::new());
    assert_eq!(json::to_string(&user), r#"{"name":"ada","tags":[],"note":null}"#);
    Ok(())
}
```

## Where to go next

* [Usage](usage.md), [API: values and traits](api.md), [API: formats](api-formats.md).
* [Errors](errors.md), [Internals](internals.md), [Testing](testing.md).
