---
title: quench-convert
parent: Crates
has_children: true
permalink: /crates/quench-convert/
nav_order: 11
---

`quench-convert` (library `quench_convert`) turns other formats into `quench_serde::Value`s: CSV, XML, Android binary XML (`AndroidManifest.xml` inside an APK) and the header and metadata of GGUF model files. It replaces the `csv`, `axmldecoder` and `gguf` crates. Because everything ends up as a `Value`, the result can be queried directly, written out with the other `quench-serde` formats, or turned into a struct with `quench_serde::from_value`.

## Facts

| | |
|---|---|
| Version | 0.1.3 |
| Edition | 2024 |
| License | MIT |
| Dependencies | `quench-serde`; dev: `quench-test` |
| Test harness | `autotests = false`; one integration test `convert` with `harness = false` |

```toml
[dependencies]
quench-convert = { version = "0.1.3", registry = "ennor" }                       # all four formats
# quench-convert = { version = "0.1.3", registry = "ennor", default-features = false, features = ["csv"] }
```

## Features

`default = ["csv", "xml", "axml", "gguf"]`.

| Feature | Module | Implies | Reads |
|---|---|---|---|
| `csv` | `csv` | | delimited text (RFC 4180) into arrays or into objects keyed by the header row; writes it back |
| `xml` | `xml` | | XML text into an element tree of values; writes it back |
| `axml` | `axml` | `xml` | Android binary XML into the same tree |
| `gguf` | `gguf` | | header, metadata and (optionally) the tensor table of a GGUF file |

The crate root re-exports `Error`, `Result` and `quench_serde::Value`.

## Where to go next

* [Usage](usage.md)
* [API reference](api.md)
* [Errors](errors.md)
* [Formats](formats.md): exact behaviour and limits of each reader.
* [Testing](testing.md)

## Minimal example

```rust
use quench_convert::csv::{self, Options};

let rows = csv::parse("name,qty\nbolt,4\n\"nut, hex\",9\n", &Options::default()).unwrap();
assert_eq!(rows[1]["name"], "nut, hex");
assert_eq!(csv::to_string(&rows, b','), "name,qty\nbolt,4\n\"nut, hex\",9\n");
```
