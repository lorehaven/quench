---
title: Usage
parent: quench-convert
grand_parent: Crates
nav_order: 1
---

## 1. CSV with a header row

```rust
use quench_convert::csv::{self, Options};

let rows = csv::parse("name,qty\nbolt,4\nnut,9\n", &Options::default()).unwrap();
assert_eq!(rows.len(), 2);
assert_eq!(rows[0]["name"], "bolt");
assert_eq!(rows[0]["qty"], "4");   // every field is a string
```

Fields are always strings; convert numbers yourself (or with `quench_serde::from_value` into a typed struct whose fields use string parsing). A record shorter than the header simply lacks the trailing keys.

## 2. CSV without a header, other delimiters

```rust
use quench_convert::csv::{self, Options};

let options = Options { delimiter: b';', header: false, ..Options::default() };
let rows = csv::parse("a;b\n1;2\n", &options).unwrap();
assert_eq!(quench_serde::json::to_string(&quench_serde::Value::Array(rows)), r#"[["a","b"],["1","2"]]"#);
```

## 3. Strict or fixed-width records

```rust
use quench_convert::csv::{self, Options};

let strict = Options { strict: true, flexible: false, ..Options::default() };
assert!(csv::parse("a,b\n1\n", &strict).is_err());             // different number of fields
assert!(csv::parse("a\n\"x\"y\n", &strict).is_err());          // text after a closing quote
```

## 4. Writing CSV

```rust
use quench_convert::csv;
use quench_serde::json;

let rows = [json::parse(r#"{"a":1,"b":null}"#).unwrap(), json::parse(r#"{"b":"x y","c":true}"#).unwrap()];
assert_eq!(csv::to_string(&rows, b','), "a,b\n1,\n,x y\n");
```

The header is the keys of the first object; later objects are written in that column order (missing keys give empty fields, extra keys are dropped). Numbers and booleans are written as JSON text, `null` as empty, fields containing the delimiter, a quote, a newline, or leading/trailing space are quoted.

## 5. Walking XML

```rust
use quench_convert::xml;

let doc = xml::parse(r#"<list n="2"><item>a</item><item>b &amp; c</item></list>"#).unwrap();
assert_eq!(xml::tag(&doc), Some("list"));
assert_eq!(xml::attr(&doc, "n"), Some("2"));
let items: Vec<_> = xml::elements(&doc, "item").map(xml::text).collect();
assert_eq!(items, ["a", "b & c"]);
assert_eq!(xml::to_string(&doc), r#"<list n="2"><item>a</item><item>b &amp; c</item></list>"#);
```

## 6. Building XML

```rust
use quench_convert::xml;
use quench_serde::{Map, Value};

let mut attrs = Map::new();
attrs.insert("id", Value::String("7".into()));
let doc = xml::element("item", attrs, vec![Value::String("x < y".into())]);
assert_eq!(xml::to_string(&doc), r#"<item id="7">x &lt; y</item>"#);
```

## 7. Android manifest from an APK

```rust,no_run
use quench_convert::{axml, xml};

let bytes = std::fs::read("AndroidManifest.xml").unwrap();   // extracted from the APK (a ZIP) beforehand
let root = axml::parse(&bytes).unwrap();
let package = xml::attr(&root, "package");
let version_code = xml::attr(&root, "android:versionCode");
println!("{package:?} {version_code:?}");
```

`axml` does not open APKs; extract `AndroidManifest.xml` first (see `quench-archive`).

## 8. GGUF metadata without loading the model

```rust,no_run
use quench_convert::gguf::{self, Options};

let file = std::fs::File::open("model.gguf").unwrap();
let header = gguf::read(file, &Options::default()).unwrap();
let layers = header["metadata"]["llama.block_count"].as_u64();
println!("{layers:?}");
```

Only the header and metadata are read, never the tensor data. With `Options { tensors: true, .. }` the tensor table is included. Wrap a large file in `std::io::BufReader`: the reader issues many tiny reads.
