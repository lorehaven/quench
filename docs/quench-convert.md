# Quench Convert

`quench-convert` (crate `quench_convert`) turns other formats into `quench_serde::Value`s. It has no dependencies beyond `quench-serde` and replaces the `csv`, `axmldecoder` and `gguf` crates. Each format is a feature (all on by default) and a module.

| Feature | Module | Reads |
| --- | --- | --- |
| `csv` | `csv` | delimited text → arrays, or objects keyed by the header row; writes it back |
| `xml` | `xml` | XML text → an element tree of values; writes it back |
| `axml` | `axml` | Android binary XML (an APK's `AndroidManifest.xml`) → the same tree (implies `xml`) |
| `gguf` | `gguf` | the header, metadata and optionally the tensor table of a GGUF model file |

Because everything ends up as a `Value`, results can be queried directly, written out with the other `quench-serde` formats, or turned into structs with `quench_serde::from_value`.

## CSV

```rust
use quench_convert::csv::{self, Options};

let rows = csv::parse("name,qty\nbolt,4\n\"nut, hex\",9\n", &Options::default())?;
rows[1]["name"]                      // "nut, hex"
csv::to_string(&rows, b',')          // back to text, quoting where needed
```

`Options`: `delimiter`, `header` (first record names the columns; columns past the names are `column<N>`), `flexible` (records may differ in length), `trim` (unquoted fields), `strict` (refuse malformed quoting). The default reading is lenient in the same way the `csv` crate is (verified on 20,000 generated inputs: identical records): a quote inside a field is a character, text after a closing quote joins the field, and an unclosed quote runs to the end. `csv::records` gives the raw string records.

## XML

An element is an object `{"tag": name, "attrs": {…}, "children": [elements and strings]}`. `xml::parse` / `parse_with` read a document into its root element; `tag`, `attr`, `children`, `child`, `elements` and `text` navigate it; `to_string` and `to_string_pretty` write it. Comments, processing instructions, the declaration and doctype are skipped, CDATA becomes text, entities and numeric references are decoded, and whitespace-only text between elements is dropped (`Options::keep_whitespace` keeps it). Namespaces are not interpreted: prefixes stay part of the names.

## Android binary XML

`axml::parse(bytes)` reads the compiled format into the tree above. Names carry their prefix (`android:versionCode`), the namespace declarations appear as `xmlns:android` attributes, and values are rendered as `aapt` shows them: integers in decimal, `0x…` for hexadecimal, `true`/`false`, `@0x7f0b0001` for references, `#aarrggbb` colors, `16dp` dimensions, `50%` fractions. The three real manifests used in the tests produce exactly the attributes `axmldecoder` does (apart from how an unresolved reference is spelled).

## GGUF

`gguf::read(reader, &Options)` (or `parse(bytes, …)`) returns `{"version", "tensor_count", "metadata": {…}}` and, with `Options::tensors`, `"tensors"`. It reads from any `Read` and stops before the tensor data, so a multi-gigabyte model costs a few milliseconds. Metadata arrays longer than `Options::max_array` (default 1024; token lists) are replaced by `{"array_length": n}` instead of being held in memory. GGUF versions 1 to 3 are supported, little-endian only. Checked against two real model files (Mistral 7B Q4_0, a Qwen split shard): same key counts as the `gguf` crate.
