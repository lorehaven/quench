# YAML in Quench Serde

The `yaml` module of [Quench Serde](./quench-serde.md) (cargo feature `yaml`, off by default) reads and writes YAML 1.2 on `quench_serde::Value`, replacing `serde_norway`. Its only dependency is `quench-serde`. It exists for the Kubernetes manifests and config files forge reads, edits and re-emits.

```rust
let mut doc = quench_serde::yaml::parse(text)?;                 // a Value
doc.get_mut("metadata").and_then(Value::as_object_mut).unwrap()
    .insert("annotations", /* ... */);
let out = quench_serde::yaml::to_string(&doc)?;

let config: Config = quench_serde::yaml::from_str(text)?;       // typed, through quench-serde's derives
let manifests: Vec<Manifest> = quench_serde::yaml::from_str_all(stream)?;   // `---`-separated
```

## What it reads

- Block mappings and sequences (including the compact `- key: v` and `- - x` forms, and a sequence flush with its key), flow `[..]` / `{..}` collections across lines, comments.
- Plain, single- and double-quoted scalars (all escapes, line folding), and `|` / `>` block scalars with indentation indicators and `-` / `+` chomping.
- The core schema's types for plain scalars: `null`/`~`/empty, `true`/`false` (any of `true`, `True`, `TRUE`), integers (decimal, `0x`, `0o`, `0b`), floats (`.5`, `5.`, `1e3`, `.inf`, `.nan`). YAML 1.1 spellings (`yes`, `on`, `007`, `1_000`) are text, as in 1.2.
- Anchors and aliases (an alias is a copy; properties on the same line as a key label the key), tags (`!!str`, `!!int`, `!!float`, `!!bool`, `!!null` change the type; other tags are ignored and a `!!`-tagged plain scalar stays text), several documents per stream (`---`, `...`, `%` directives skipped, a leading byte-order mark).
- Mapping keys that are not strings are read as their text (`1: a` is the key `"1"`).

Not supported, and reported as errors: complex `? ` keys, tabs as indentation. Not applied: `<<` merge keys (the key is kept as written).

## What it writes

Block style with two-space indentation and sequences flush with their key (the layout `serde_yaml` and `kubectl` produce). A string is written plain when it reads back as the same string, single-quoted when it would otherwise read as another type or contains YAML syntax, double-quoted when it holds control characters, and as a `|` block when it has line breaks. Floats use `1e20` / `.inf` / `.nan`.

## Verification

Run against `serde_norway` as an oracle: 1,517 real YAML files (forge's golden manifests and overlays, homecloud overlays, and every YAML file in the installed Python packages and cargo registry) parse to identical values, with zero accept/reject differences other than OpenCV's non-standard `%YAML:1.0` header and a handful of invalid files libyaml rejects. 60,000 random trees full of awkward strings round-trip through both parsers.

[Home](../README.md)
