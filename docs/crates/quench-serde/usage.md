---
title: Usage
parent: quench-serde
grand_parent: Crates
nav_order: 1
---

Worked examples. Items are documented in [API - values and traits](api.md) and [API - formats](api-formats.md).

## 1. Typed JSON round trip

```rust
use quench_serde::{Deserialize, Serialize, json};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Plain { name: String, count: u32, note: Option<String> }

let p: Plain = json::from_str(r#"{"name":"a","count":3}"#)?;
assert_eq!(p.note, None);                       // Option: absent key is None
assert_eq!(json::to_string(&p), r#"{"name":"a","count":3,"note":null}"#);
```

Struct fields keep their declaration order. Pitfall: any other missing field is an error: ``missing field `count` ``.

## 2. Work with the dynamic `Value`

```rust
use quench_serde::{Value, json};

let mut doc = json::parse(r#"{"user":{"tags":["a","b"]}}"#)?;
assert_eq!(doc["user"]["tags"][1], "b");
assert_eq!(doc.pointer("/user/tags/0").and_then(Value::as_str), Some("a"));
doc["user"]["active"] = Value::Bool(true);       // creates the key
println!("{doc:#}");                              // pretty JSON
```

Reading a missing entry gives `Null`; assigning through a scalar panics.

## 3. Build a value with `json!`

```rust
let n = 3;
let v = quench_serde::json!({ "n": n + 1, "list": [1, 2, null], "who": "ada" });
```

## 4. Rename, defaults, flatten, enums

```rust
use quench_serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Server {
    host_name: String,
    #[serde(default)]
    port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    tls: Option<bool>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Event { Ping, Message { text: String } }
```

`Event::Message{..}` serialises as `{"type":"message","text":"..."}`. All attributes: [quench-serde-macros](../quench-serde-macros/api.md).

## 5. Query strings and forms

```rust
use quench_serde::urlencoded;

#[derive(quench_serde::Deserialize)]
struct Query { n: u32, tags: Vec<String> }

let q: Query = urlencoded::from_str("n=5&tags=a&tags=b")?;
let lone: Query = urlencoded::from_str("n=5&tags=a")?;      // one value is a one-element list
assert_eq!(urlencoded::to_string(&quench_serde::json!({"a": "x y"}))?, "a=x+y");
assert_eq!(urlencoded::percent_encode("x y"), "x%20y");
```

Pitfall: nested objects cannot be written as a form (error).

## 6. TOML configuration

```rust
use quench_serde::toml;
#[derive(quench_serde::Deserialize)]
struct Cfg { port: u16, names: Vec<String> }
let cfg: Cfg = toml::from_str("port = 8080\nnames = ['a', 'b']\n")?;
let text = toml::to_string_pretty(&quench_serde::json!({"port": 8080, "names": ["a"]}))?;
```

A datetime reads as text; read an offset datetime into `quench_core::time::Time`.

## 7. Edit a YAML manifest (feature `yaml`)

```rust
use quench_serde::{Map, Value, yaml};

let mut doc = yaml::parse("kind: ConfigMap\nmetadata:\n  name: cfg\n")?;
let meta = doc.get_mut("metadata").and_then(Value::as_object_mut).unwrap();
let mut ann = Map::new();
ann.insert("edited", Value::from("true"));
meta.insert("annotations", Value::Object(ann));
let out = yaml::to_string(&doc)?;      // block style, "true" is quoted because it would read as a bool
let docs: Vec<Value> = yaml::parse_all("a: 1\n---\na: 2\n")?;
```

(Adapted from the unit tests of `yaml/mod.rs`.)

## 8. Error paths

```rust
let e = quench_serde::json::from_str::<Vec<u8>>("[1, 300]").unwrap_err();
assert_eq!(e.path(), "[1]");
```

Parse errors carry `location()`; conversion errors carry `path()`.
