---
title: Internals
parent: quench-serde
grand_parent: Crates
nav_order: 5
---

## The one detour

Every format parses text into `Value`; every type converts to/from `Value`. Typed reading is `parse` then `T::deserialize`; typed writing is `serialize` then print. There is no streaming and no borrowed deserialisation (`&str` cannot be deserialized).

## Lenient (form) mode

`urlencoded::from_str` enters `private::Lenient::enter()`, a thread-local flag restored on drop. While it is set, `bool`, the integer types, `f64`/`f32` accept string input (parsing it) and `Vec<T>` accepts a lone string as a one-element list. JSON, TOML and YAML never set it.

## Value model

* `Map` is a `Vec<(String, Value)>` plus a `HashMap<String, usize>` index: lookups are hashed, iteration is in insertion order. `remove` keeps the order of the rest.
* Canonical integers: `Value::from_i64` yields `UInt` for non-negative input. `Value::Float` never equals an integer.
* `u128`/`i128` beyond 64 bits become inexact floats on serialise (`as f64`) and cannot be read back beyond 64 bits.

## Limits

* JSON nesting is capped at `json::MAX_DEPTH` (128). The TOML and YAML readers have no such cap: in a scratch program, `toml::parse("a = " + "[" * 100000 + "]" * 100000)` and `yaml::parse("[" * 100000 + "]" * 100000)` abort the process with `fatal runtime error: stack overflow` (1000 levels parse fine), while `json::parse` of the same depth returns `recursion limit exceeded`. Do not feed untrusted text to the TOML or YAML parser without bounding its size or nesting yourself.
* JSON float output is the shortest round-trip text laid out like `serde_json`. The old page records one known difference from `serde_json` (a last-digit tie in about 3 of 10,000 random floats; both read back equal) -- not re-measured here.

## Derive support code

The derives call `::quench_serde::__private::*` (hidden). The module name and function set may change with the macros ([quench-serde-macros](../quench-serde-macros/index.md)).

## Verification claims carried over (not re-run here)

The old pages state: JSON output matched `serde_json` byte for byte on 140,000 generated documents and parsing was cross-checked on 200,000 generated and mutated documents; urlencoded output and decoding matched `serde_urlencoded` on 200,000 inputs; TOML agreed with the `toml` crate on a spec corpus, 300,000 mutated documents and 30,000 random trees; the YAML reader agreed with `serde_norway` on 1,517 real YAML files (with a handful of differences: OpenCV's non-standard `%YAML:1.0` header and invalid files libyaml rejects) and 60,000 random trees round-tripped. Those harnesses are not in the repository.

## YAML details from the old page

An alias is a copy; properties on the same line as a key label the key; tags `!!str`, `!!int`, `!!float`, `!!bool`, `!!null` change the type, other tags are ignored and a `!!`-tagged plain scalar stays text; the source's own module comment adds that anchors/aliases, comments and several documents are supported and that mapping keys that are not strings are read as their text. The YAML reader exists for the Kubernetes manifests and config files that the forge services read, edit and re-emit.
