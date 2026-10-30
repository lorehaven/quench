---
title: Errors
parent: quench-schema
grand_parent: Crates
nav_order: 3
---

Two kinds of error exist.

| Type | Raised by | When |
|---|---|---|
| `SchemaError` | `Schema::new` | the schema itself is unusable: wrong keyword shape or an invalid regular expression; messages in [keywords](keywords.md#shape-checks-in-schemanew), `Display` adds `invalid schema: ` |
| `ValidationError` | `Schema::validate` (a `Vec`, not a `Result`) | the instance breaks a rule; the `ErrorKind` variant and message of each rule are in [keywords](keywords.md) |

`validate` never fails: an empty vector means valid. `SchemaError` wraps a private string; to branch on the cause, match on `to_string()`. `ValidationError` and `SchemaError` implement `std::error::Error`.

Message conventions follow the `jsonschema` crate: instances appear as compact JSON (`{"a":1,"b":2}`), singular/plural nouns are adjusted (`1 character`, `2 characters`, `1 property`, `2 properties`), and `enum` lists up to three options before collapsing to `"a", "b" or N other candidates`.
