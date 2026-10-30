---
title: Usage
parent: quench-schema
grand_parent: Crates
nav_order: 1
---

## 1. Validate a document and list the problems

```rust
use quench_schema::{ErrorKind, Schema};
use quench_serde::json;

let schema = Schema::new(&json::parse(r#"{
    "type": "object",
    "required": ["name"],
    "properties": {
        "name": {"type": "string", "minLength": 1},
        "tags": {"type": "array", "items": {"type": "string"}, "uniqueItems": true}
    },
    "additionalProperties": false
}"#).unwrap()).unwrap();

let doc = json::parse(r#"{"tags": ["a", 1, "a"], "extra": 1}"#).unwrap();
for problem in schema.validate(&doc) {
    println!("{}: {}", problem.instance_path(), problem.message());
}
let problems = schema.validate(&doc);
assert!(problems.iter().any(|p| p.message() == "\"name\" is a required property"));
assert!(problems.iter().any(|p| p.instance_path() == "/tags/1"));
assert!(matches!(problems[0].kind(), ErrorKind::Required { .. }));
```

## 2. Just a yes/no

```rust
use quench_schema::Schema;
use quench_serde::json;

let schema = Schema::new(&json::parse(r#"{"type": "integer", "minimum": 1}"#).unwrap()).unwrap();
assert!(schema.is_valid(&json::parse("3").unwrap()));
assert!(!schema.is_valid(&json::parse("0").unwrap()));
```

## 3. Reuse definitions with `$ref`

```rust
use quench_schema::Schema;
use quench_serde::json;

let schema = Schema::new(&json::parse(r##"{
    "$defs": { "port": {"type": "integer", "minimum": 1, "maximum": 65535} },
    "properties": { "http": {"$ref": "#/$defs/port"}, "https": {"$ref": "#/$defs/port"} }
}"##).unwrap()).unwrap();
let problems = schema.validate(&json::parse(r#"{"http": 80, "https": 70000}"#).unwrap());
assert_eq!(problems.len(), 1);
assert_eq!(problems[0].instance_path(), "/https");
assert_eq!(problems[0].message(), "70000 is greater than the maximum of 65535");
```

## 4. Conditional rules

```rust
use quench_schema::Schema;
use quench_serde::json;

let schema = Schema::new(&json::parse(r#"{
    "if": {"properties": {"kind": {"const": "tls"}}},
    "then": {"required": ["cert"]},
    "else": {"required": ["host"]}
}"#).unwrap()).unwrap();
assert!(schema.is_valid(&json::parse(r#"{"kind": "tls", "cert": "x"}"#).unwrap()));
assert!(!schema.is_valid(&json::parse(r#"{"kind": "plain"}"#).unwrap()));
```

## 5. Reject a bad schema early

```rust
use quench_schema::Schema;
use quench_serde::json;

let error = Schema::new(&json::parse(r#"{"pattern": "("}"#).unwrap()).unwrap_err();
assert_eq!(error.to_string(), "invalid schema: `(` is not a valid pattern: regex parse error at 1: unclosed group");
```

## Pitfalls

* `pattern` is unanchored: write `^...$` for a full match.
* Unknown keywords and `format` are ignored, so a typo (`"minlength"`) validates everything. Check your schemas with a test that should fail.
* An unresolvable or remote `$ref` is silently ignored.
* Validation of a type-specific keyword is skipped for other types: `"minLength": 3` accepts the number `5`; combine with `"type": "string"`.
