# quench-convert
Readers and writers that turn other formats into `quench-serde` values: CSV, XML, Android binary XML and GGUF metadata.

## What it is
`quench-convert` turns other formats into `quench_serde::Value`s: CSV, XML, Android binary XML (`AndroidManifest.xml` inside an APK) and the header and metadata of GGUF model files. It replaces the `csv`, `axmldecoder` and `gguf` crates.

Because everything ends up as a `Value`, the result can be queried directly, written out with the other `quench-serde` formats, or turned into a struct with `quench_serde::from_value`. The crate depends on `quench-serde`; `axml` does not open APKs, so pair it with `quench-archive` to extract the manifest first.

## Use cases
- Load a CSV file into records keyed by the header row.
- Write a list of objects back out as CSV.
- Read, walk and build XML documents as an element tree of values.
- Read the package name and version code from an APK's binary `AndroidManifest.xml`.
- Inspect the header and metadata of a GGUF model file.

## Example
```rust
use quench_convert::csv::{self, Options};

let rows = csv::parse("name,qty\nbolt,4\n\"nut, hex\",9\n", &Options::default()).unwrap();
assert_eq!(rows[1]["name"], "nut, hex");
assert_eq!(csv::to_string(&rows, b','), "name,qty\nbolt,4\n\"nut, hex\",9\n");
```

```rust
use quench_convert::xml;

let doc = xml::parse(r#"<list n="2"><item>a</item><item>b &amp; c</item></list>"#).unwrap();
assert_eq!(xml::attr(&doc, "n"), Some("2"));
let items: Vec<_> = xml::elements(&doc, "item").map(xml::text).collect();
assert_eq!(items, ["a", "b & c"]);
```

## Features
Default: all four.
- `csv`: delimited text (RFC 4180), read and write.
- `xml`: XML text to and from the element tree.
- `axml`: Android binary XML into the same tree (implies `xml`).
- `gguf`: header, metadata and optionally the tensor table of a GGUF file.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-convert/
