# quench-serde
JSON/TOML/urlencoded codecs and a dynamic Value model, without serde.

## What it is
`quench-serde` replaces `serde`, `serde_json`, `serde_urlencoded`, `toml` and the YAML crate with one small crate. Types do not implement a visitor protocol: they convert to and from a dynamic `Value`, and every format parses into or prints from that same `Value`.

`#[derive(Serialize, Deserialize)]` comes from `quench-serde-macros` and is re-exported. The crate depends only on `quench-core` and the derive crate, and builds for `wasm32-unknown-unknown`. The derives need the dependency to be named `quench-serde`.

## Use cases
- Typed JSON round trips for API payloads.
- Navigate and edit untyped documents with `Value` and `json!`.
- Read and write TOML configuration.
- Parse query strings and form bodies.

## Example
```rust
use quench_serde::{Deserialize, Serialize, Value, json};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Plain { name: String, count: u32, note: Option<String> }

let p: Plain = json::from_str(r#"{"name":"a","count":3}"#)?;
assert_eq!(json::to_string(&p), r#"{"name":"a","count":3,"note":null}"#);

let mut doc = json::parse(r#"{"user":{"tags":["a","b"]}}"#)?;
assert_eq!(doc["user"]["tags"][1], "b");
doc["user"]["active"] = Value::Bool(true);
```

## Features
- `toml` (default): TOML 1.0 reader and writer.
- `yaml`: YAML 1.2 reader and writer.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-serde/
