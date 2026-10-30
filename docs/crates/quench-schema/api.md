---
title: API reference
parent: quench-schema
grand_parent: Crates
nav_order: 2
---

Every public item of `quench_schema`. No `unsafe`, no feature gates.

## `struct Schema`

```rust
#[derive(Debug)]
pub struct Schema { /* root: Value, patterns: HashMap<String, Regex> */ }

impl Schema {
    pub fn new(schema: &Value) -> Result<Self, SchemaError>
    #[must_use] pub fn validate(&self, instance: &Value) -> Vec<ValidationError>
    #[must_use] pub fn is_valid(&self, instance: &Value) -> bool
}
```

| Method | Behaviour |
|---|---|
| `new(schema)` | walks the schema, checks the shape of the keywords listed in [keywords](keywords.md#shape-checks-in-schemanew) and compiles every `pattern` and `patternProperties` key with `quench_regex::Regex::new`. Clones the schema value. Errors: `SchemaError`. |
| `validate(instance)` | every rule the instance breaks, in evaluation order; empty when valid. Never panics for any instance. |
| `is_valid(instance)` | `validate(instance).is_empty()` (it builds the full error list). |

A `Schema` is immutable and `Send + Sync` (it holds a `Value` and compiled `Regex`es), so one can be shared between threads.

## `struct SchemaError`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaError(/* private String */);
impl fmt::Display for SchemaError   // "invalid schema: {message}"
impl std::error::Error for SchemaError
```

A schema that cannot be used. The message is available only through `Display`/`to_string()` (the field is private). Messages are in [keywords](keywords.md#shape-checks-in-schemanew).

## `struct ValidationError`

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError { /* path, message, kind */ }

impl ValidationError {
    #[must_use] pub fn instance_path(&self) -> &str
    #[must_use] pub fn message(&self) -> &str
    #[must_use] pub const fn kind(&self) -> &ErrorKind
}
impl fmt::Display for ValidationError   // the message
impl std::error::Error for ValidationError
```

* `instance_path()`: a JSON Pointer into the instance (`""` = the whole instance).
* `message()`: for people, e.g. `0 is less than the minimum of 1`.
* `kind()`: which rule was broken.

## `enum ErrorKind`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorKind {
    False,
    Type { expected: Vec<String> },
    Required { property: String },
    AdditionalProperties { unexpected: Vec<String> },
    Enum,
    Const,
    Pattern { pattern: String },
    MinLength { limit: u64 },
    MaxLength { limit: u64 },
    Minimum { limit: f64 },
    Maximum { limit: f64 },
    ExclusiveMinimum { limit: f64 },
    ExclusiveMaximum { limit: f64 },
    MultipleOf { divisor: f64 },
    MinItems { limit: u64 },
    MaxItems { limit: u64 },
    UniqueItems,
    Contains,
    MinProperties { limit: u64 },
    MaxProperties { limit: u64 },
    PropertyNames,
    AnyOf,
    OneOfNotValid,
    OneOfMultipleValid,
    Not,
}
```

`Type.expected` is sorted by type rank (null, boolean, integer, number, string, array, object). `Required` is also produced by `dependentRequired`. `PropertyNames` carries no detail; the message is the inner problem's message. There is no variant for `$ref`, `allOf`, `if`/`then`/`else` or `items`: their errors are the errors of the subschemas. The enum is not `#[non_exhaustive]`.
