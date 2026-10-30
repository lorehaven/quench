---
title: Internals
parent: quench-schema
grand_parent: Crates
nav_order: 5
---

## Evaluation

`Schema::new` clones the schema `Value` and builds a map of compiled patterns (one `Regex` per distinct pattern string, including `patternProperties` keys). `validate` walks the schema and the instance together:

1. `check(schema, instance, path, out, depth)`: a `false` schema reports `False`; a non-object schema (`true`) returns; depth above 128 returns silently (so a `$ref` cycle accepts).
2. `type`, `enum`, `const`; scalar keywords by instance type; array keywords; object keywords; combinators; `$ref`.
3. `anyOf`, `oneOf`, `not`, `if` and `contains` test branches with `is_valid_under`, which runs the subschema into a scratch vector and discards it.

Keywords apply independently; there is no short-circuit, so a single value can report several problems (for example `minimum` and `multipleOf` can both fire on one number).

## Numbers

Numeric comparisons convert to `f64`. `Value::Int`/`UInt` beyond 2^53 lose precision in comparisons. `multipleOf` uses integer remainder when both the instance and the divisor are integers, else a relative tolerance of `1e-9`.

## Complexity

Linear in the size of the instance times the number of keywords, with the cost of the regex for each `pattern` (Pike VM, never exponential). `uniqueItems` and `enum` are quadratic in the number of items/options. `Schema::new` and `validate` clone little: `Schema::new` clones the schema once.

## Not implemented

Remote and relative `$ref`, `$id`/`$anchor`/`$dynamicRef`, `unevaluatedProperties`/`unevaluatedItems`, `dependentSchemas`, `minContains`/`maxContains`, `contentEncoding`/`contentMediaType`, `format` assertion, vocabularies, meta-schema validation of the schema, draft-07 array `items`, output formats other than a flat list.

## Differences from the `jsonschema` crate

Stated by the previous documentation of this crate and not re-verified here: `additionalProperties: false` on a schema with no `properties` reports the unexpected names as `AdditionalProperties` (where `jsonschema` reports a false-schema error at the wrong path), and `propertyNames: false` reports one error per name. The same page reports a comparison with `jsonschema` (draft 2020-12) on 3,600 mutated instances of nine CRD schemas and 32,000 instances of 4,000 random schemas; no such comparison harness is in this repository.
