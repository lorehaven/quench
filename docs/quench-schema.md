# Quench Schema

`quench-schema` (crate `quench_schema`) validates a `quench_serde::Value` against a JSON Schema, replacing the `jsonschema` crate for Kubernetes CRD checks and configuration validation. It depends on `quench-serde` and `quench-regex` only.

```rust
use quench_schema::Schema;
use quench_serde::json;

let schema = Schema::new(&json::parse(r#"{
    "type": "object",
    "properties": { "replicas": { "type": "integer", "minimum": 1 } },
    "required": ["replicas"],
    "additionalProperties": false
}"#)?)?;

let problems = schema.validate(&json::parse(r#"{"replicas": 0, "extra": true}"#)?);
// problems[0].instance_path() == "/replicas"
// problems[0].message()       == "0 is less than the minimum of 1"
```

## API

- `Schema::new(&Value)` checks the keywords' shapes and compiles every `pattern` / `patternProperties` expression (a bad one is a `SchemaError`, not a silent pass). `validate(&Value)` returns every broken rule; `is_valid` just says whether there are any.
- A `ValidationError` has `instance_path()` (a JSON Pointer, `""` for the whole instance, `/spec/replicas`, with `~0` and `~1` escapes), `message()` and a typed `kind()` (`Type`, `Required`, `AdditionalProperties { unexpected }`, `Enum`, `Pattern`, `MinLength`, `Minimum`, `OneOfMultipleValid`, ...) for callers that react to a particular failure, as riveter does to suggest a spelling for an unknown field.

## Keywords

`type`, `enum`, `const`, `properties`, `patternProperties`, `additionalProperties`, `required`, `propertyNames`, `minProperties`, `maxProperties`, `dependentRequired`, `items`, `prefixItems`, `contains`, `minItems`, `maxItems`, `uniqueItems`, `minLength`, `maxLength`, `pattern`, `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`, `allOf`, `anyOf`, `oneOf`, `not`, `if`/`then`/`else`, and local `$ref` (`#`, `#/$defs/x`, `#/definitions/x`; a reference cycle stops at a depth limit instead of looping). `format` and every other keyword (`description`, `default`, `x-kubernetes-*`) are annotations and never fail an instance. Numbers compare as JSON numbers (`1` equals `1.0`; `integer` accepts `2.0`). String lengths count characters.

The messages read like the `jsonschema` crate's (`"a" is a required property`, `5 is not of type "string"`, `"z" is not one of "a", "b" or "c"`, `Additional properties are not allowed ('x' was unexpected)`), so output that people or tests already match on does not change. The errors of a schema appear in a fixed keyword order, not the schema's own.

## Differences from `jsonschema`

`additionalProperties: false` on a schema with no `properties` reports the unexpected names as `AdditionalProperties` (the crate reports a false-schema error at the wrong path), and `propertyNames: false` reports one error per name. Not implemented: remote references, `$anchor`/`$dynamicRef`, `unevaluated*`, `contentSchema`, `format` assertion, older drafts' array-form `items`.

## Testing

Unit tests next to the code; against the `jsonschema` crate (draft 2020-12), messages and paths are identical on 3,600 mutated instances of the nine CRD schemas riveter ships (1,868 of them invalid) and on 32,000 instances of 4,000 random schemas over the keywords above, apart from the two differences listed.

[Home](../README.md)
