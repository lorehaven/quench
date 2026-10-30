---
title: quench-schema
parent: Crates
has_children: true
permalink: /crates/quench-schema/
nav_order: 40
---

`quench-schema` (library `quench_schema`) validates `quench_serde::Value`s against JSON Schema, using the draft 2020-12 keywords that Kubernetes CRDs and configuration files use. It replaces the `jsonschema` crate; its messages are written to read like that crate's. Schemas are compiled once (patterns become `quench_regex::Regex` values), validation returns every problem found, not just the first.

## Facts

| | |
|---|---|
| Version | 0.1.3 |
| Edition | 2024 |
| License | MIT |
| Features | none |
| Test harness | `harness = false`, `quench_test::main!()` |

```toml
[dependencies]
quench-schema = { version = "0.1.3", registry = "ennor" }
quench-serde = { version = "0.1", registry = "ennor" }
```

## Dependencies

| Crate | Why |
|---|---|
| `quench-serde` | `Value`, `Map`, `json::parse`, `json::to_string` (messages show instances as compact JSON) |
| `quench-regex` | `pattern`, `patternProperties`, `propertyNames` patterns |
| `quench-test` (dev) | test harness |

## Where to go next

* [Usage](usage.md)
* [Keyword reference](keywords.md): every supported keyword and every ignored one, with the message each failure produces.
* [API reference](api.md)
* [Errors](errors.md): `SchemaError` messages and `ErrorKind` variants.
* [Internals](internals.md): evaluation order, limits, differences from the full specification.
* [Testing](testing.md)

## Minimal example

```rust
use quench_schema::Schema;
use quench_serde::json;

let schema = Schema::new(&json::parse(r#"{
    "type": "object",
    "properties": { "replicas": { "type": "integer", "minimum": 1 } },
    "required": ["replicas"],
    "additionalProperties": false
}"#).unwrap()).unwrap();

let problems = schema.validate(&json::parse(r#"{"replicas": 0, "extra": true}"#).unwrap());
assert_eq!(problems[0].message(), "0 is less than the minimum of 1");
assert_eq!(problems[0].instance_path(), "/replicas");
```
