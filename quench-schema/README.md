# quench-schema
JSON Schema validation over `quench-serde` values (the draft 2020-12 keywords Kubernetes CRDs and configuration use).

## What it is
`quench-schema` validates `quench_serde::Value`s against JSON Schema, using the draft 2020-12 keywords that Kubernetes CRDs and configuration files use. It replaces the `jsonschema` crate, and its messages are written to read like that crate's.

Schemas are compiled once, with patterns becoming `quench-regex` values, and validation returns every problem found, not just the first. It builds on `quench-serde` and `quench-regex`.

## Use cases
- Validate configuration files before applying them.
- Check CRD-style documents and report every violation with its instance path.
- Get a quick yes/no with `is_valid`.
- Share definitions across a schema with `$ref` and `$defs`.

## Example
```rust
use quench_schema::Schema;
use quench_serde::json;

let schema = Schema::new(&json::parse(r#"{
    "type": "object",
    "required": ["name"],
    "properties": { "name": {"type": "string", "minLength": 1} },
    "additionalProperties": false
}"#).unwrap()).unwrap();

let doc = json::parse(r#"{"extra": 1}"#).unwrap();
for problem in schema.validate(&doc) {
    println!("{}: {}", problem.instance_path(), problem.message());
}
assert!(schema.is_valid(&json::parse(r#"{"name": "a"}"#).unwrap()));
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-schema/
