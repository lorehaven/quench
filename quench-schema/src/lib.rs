//! JSON Schema validation (draft 2020-12 keywords) over [`quench_serde::Value`].
//!
//! ```
//! use quench_schema::Schema;
//! use quench_serde::json;
//!
//! let schema = Schema::new(&json::parse(r#"{
//!     "type": "object",
//!     "properties": { "replicas": { "type": "integer", "minimum": 1 } },
//!     "required": ["replicas"],
//!     "additionalProperties": false
//! }"#).unwrap()).unwrap();
//!
//! let problems = schema.validate(&json::parse(r#"{"replicas": 0, "extra": true}"#).unwrap());
//! assert_eq!(problems[0].message(), "0 is less than the minimum of 1");
//! assert_eq!(problems[0].instance_path(), "/replicas");
//! ```
//!
//! Supported keywords: `type`, `enum`, `const`, `properties`, `patternProperties`, `additionalProperties`,
//! `required`, `propertyNames`, `minProperties`, `maxProperties`, `dependentRequired`, `items`, `prefixItems`,
//! `contains`, `minItems`, `maxItems`, `uniqueItems`, `minLength`, `maxLength`, `pattern`, `minimum`, `maximum`,
//! `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`, `allOf`, `anyOf`, `oneOf`, `not`, `if`/`then`/`else`
//! and local `$ref` (`#`, `#/$defs/x`, `#/definitions/x`). `format` and every unknown keyword (`description`,
//! `x-kubernetes-*`, ...) are annotations: they never fail validation. The messages read like the
//! `jsonschema` crate's.

use std::collections::HashMap;
use std::fmt;

use quench_regex::Regex;
use quench_serde::{Map, Value, json};

/// A schema that cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaError(String);

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid schema: {}", self.0)
    }
}

impl std::error::Error for SchemaError {}

/// Which rule an instance broke.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorKind {
    /// A `false` schema.
    False,
    /// `type`: the accepted type names.
    Type {
        expected: Vec<String>,
    },
    /// `required`: the missing property.
    Required {
        property: String,
    },
    /// `additionalProperties`: the properties nothing allows.
    AdditionalProperties {
        unexpected: Vec<String>,
    },
    Enum,
    Const,
    Pattern {
        pattern: String,
    },
    MinLength {
        limit: u64,
    },
    MaxLength {
        limit: u64,
    },
    Minimum {
        limit: f64,
    },
    Maximum {
        limit: f64,
    },
    ExclusiveMinimum {
        limit: f64,
    },
    ExclusiveMaximum {
        limit: f64,
    },
    MultipleOf {
        divisor: f64,
    },
    MinItems {
        limit: u64,
    },
    MaxItems {
        limit: u64,
    },
    UniqueItems,
    Contains,
    MinProperties {
        limit: u64,
    },
    MaxProperties {
        limit: u64,
    },
    PropertyNames,
    /// `anyOf` where no branch fits.
    AnyOf,
    /// `oneOf` where no branch fits.
    OneOfNotValid,
    /// `oneOf` where several branches fit.
    OneOfMultipleValid,
    Not,
}

/// One broken rule, where in the instance, and what it says.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    path: String,
    message: String,
    kind: ErrorKind,
}

impl ValidationError {
    /// A JSON Pointer to the offending part of the instance (`""` for the whole of it, `/spec/replicas`).
    #[must_use]
    pub fn instance_path(&self) -> &str {
        &self.path
    }

    /// For people: `0 is less than the minimum of 1`.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub const fn kind(&self) -> &ErrorKind {
        &self.kind
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ValidationError {}

/// A compiled schema.
#[derive(Debug)]
pub struct Schema {
    root: Value,
    patterns: HashMap<String, Regex>,
}

/// How deep `$ref`s and subschemas may nest before validation gives up (a reference cycle).
const MAX_DEPTH: usize = 128;

impl Schema {
    /// Checks `schema` and compiles its regular expressions.
    ///
    /// # Errors
    ///
    /// [`SchemaError`] when a keyword has the wrong shape or a `pattern` is not a valid expression.
    pub fn new(schema: &Value) -> Result<Self, SchemaError> {
        let mut patterns = HashMap::new();
        collect(schema, &mut patterns, 0)?;
        Ok(Self {
            root: schema.clone(),
            patterns,
        })
    }

    /// Every rule `instance` breaks (empty when it is valid).
    #[must_use]
    pub fn validate(&self, instance: &Value) -> Vec<ValidationError> {
        let mut out = Vec::new();
        let mut path = Vec::new();
        self.check(&self.root, instance, &mut path, &mut out, 0);
        out
    }

    #[must_use]
    pub fn is_valid(&self, instance: &Value) -> bool {
        self.validate(instance).is_empty()
    }

    fn pointer(path: &[String]) -> String {
        path.iter().fold(String::new(), |mut pointer, segment| {
            pointer.push('/');
            pointer.push_str(&segment.replace('~', "~0").replace('/', "~1"));
            pointer
        })
    }

    fn fail(path: &[String], out: &mut Vec<ValidationError>, kind: ErrorKind, message: String) {
        out.push(ValidationError {
            path: Self::pointer(path),
            message,
            kind,
        });
    }

    fn is_valid_under(&self, schema: &Value, instance: &Value, depth: usize) -> bool {
        let mut scratch = Vec::new();
        self.check(schema, instance, &mut Vec::new(), &mut scratch, depth);
        scratch.is_empty()
    }

    #[allow(clippy::too_many_lines)]
    fn check(
        &self,
        schema: &Value,
        instance: &Value,
        path: &mut Vec<String>,
        out: &mut Vec<ValidationError>,
        depth: usize,
    ) {
        match schema {
            Value::Object(_) => {}
            Value::Bool(false) => {
                Self::fail(
                    path,
                    out,
                    ErrorKind::False,
                    format!("False schema does not allow {}", show(instance)),
                );
                return;
            }
            _ => return,
        }
        if depth > MAX_DEPTH {
            return;
        }
        let depth = depth + 1;
        let keyword = |name: &str| schema.get(name);

        if let Some(types) = keyword("type") {
            Self::check_type(types, instance, path, out);
        }
        if let Some(Value::Array(options)) = keyword("enum")
            && !options.iter().any(|o| equal(o, instance))
        {
            Self::fail(
                path,
                out,
                ErrorKind::Enum,
                format!("{} is not one of {}", show(instance), list_options(options)),
            );
        }
        if let Some(expected) = keyword("const")
            && !equal(expected, instance)
        {
            Self::fail(
                path,
                out,
                ErrorKind::Const,
                format!("{} was expected", show(expected)),
            );
        }

        self.check_scalar(schema, instance, path, out);
        self.check_array(schema, instance, path, out, depth);
        self.check_object(schema, instance, path, out, depth);
        self.check_combinators(schema, instance, path, out, depth);

        if let Some(Value::String(target)) = keyword("$ref")
            && let Some(resolved) = resolve(&self.root, target)
        {
            self.check(resolved, instance, path, out, depth);
        }
    }

    fn check_type(
        types: &Value,
        instance: &Value,
        path: &[String],
        out: &mut Vec<ValidationError>,
    ) {
        let names: Vec<&str> = match types {
            Value::String(name) => vec![name.as_str()],
            Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
            _ => return,
        };
        if names.iter().any(|name| has_type(instance, name)) {
            return;
        }
        let mut expected: Vec<String> = names.iter().map(|n| (*n).to_string()).collect();
        expected.sort_by_key(|n| type_rank(n));
        let message = if expected.len() == 1 {
            format!("{} is not of type \"{}\"", show(instance), expected[0])
        } else {
            let quoted: Vec<String> = expected.iter().map(|n| format!("\"{n}\"")).collect();
            format!("{} is not of types {}", show(instance), quoted.join(", "))
        };
        Self::fail(path, out, ErrorKind::Type { expected }, message);
    }

    // One block per keyword family, in the order the keywords are documented.
    #[allow(clippy::too_many_lines)]
    fn check_scalar(
        &self,
        schema: &Value,
        instance: &Value,
        path: &[String],
        out: &mut Vec<ValidationError>,
    ) {
        if let Value::String(text) = instance {
            let length = text.chars().count() as u64;
            if let Some(limit) = schema.get("minLength").and_then(Value::as_u64)
                && length < limit
            {
                Self::fail(
                    path,
                    out,
                    ErrorKind::MinLength { limit },
                    format!(
                        "{} is shorter than {limit} character{}",
                        show(instance),
                        plural(limit)
                    ),
                );
            }
            if let Some(limit) = schema.get("maxLength").and_then(Value::as_u64)
                && length > limit
            {
                Self::fail(
                    path,
                    out,
                    ErrorKind::MaxLength { limit },
                    format!(
                        "{} is longer than {limit} character{}",
                        show(instance),
                        plural(limit)
                    ),
                );
            }
            if let Some(Value::String(pattern)) = schema.get("pattern")
                && let Some(regex) = self.patterns.get(pattern)
                && !regex.is_match(text)
            {
                Self::fail(
                    path,
                    out,
                    ErrorKind::Pattern {
                        pattern: pattern.clone(),
                    },
                    format!("{} does not match \"{pattern}\"", show(instance)),
                );
            }
        }
        let Some(number) = instance.as_f64().filter(|_| instance.is_number()) else {
            return;
        };
        let limit = |name: &str| schema.get(name).and_then(Value::as_f64);
        if let Some(limit) = limit("minimum")
            && number < limit
        {
            Self::fail(
                path,
                out,
                ErrorKind::Minimum { limit },
                format!(
                    "{} is less than the minimum of {}",
                    show(instance),
                    show_f64(schema, "minimum", limit)
                ),
            );
        }
        if let Some(limit) = limit("maximum")
            && number > limit
        {
            Self::fail(
                path,
                out,
                ErrorKind::Maximum { limit },
                format!(
                    "{} is greater than the maximum of {}",
                    show(instance),
                    show_f64(schema, "maximum", limit)
                ),
            );
        }
        if let Some(limit) = limit("exclusiveMinimum")
            && number <= limit
        {
            Self::fail(
                path,
                out,
                ErrorKind::ExclusiveMinimum { limit },
                format!(
                    "{} is less than or equal to the minimum of {}",
                    show(instance),
                    show_f64(schema, "exclusiveMinimum", limit)
                ),
            );
        }
        if let Some(limit) = limit("exclusiveMaximum")
            && number >= limit
        {
            Self::fail(
                path,
                out,
                ErrorKind::ExclusiveMaximum { limit },
                format!(
                    "{} is greater than or equal to the maximum of {}",
                    show(instance),
                    show_f64(schema, "exclusiveMaximum", limit)
                ),
            );
        }
        if let Some(divisor) = limit("multipleOf")
            && divisor > 0.0
            && !is_multiple(instance, schema.get("multipleOf"), number, divisor)
        {
            Self::fail(
                path,
                out,
                ErrorKind::MultipleOf { divisor },
                format!(
                    "{} is not a multiple of {}",
                    show(instance),
                    show_f64(schema, "multipleOf", divisor)
                ),
            );
        }
    }

    fn check_array(
        &self,
        schema: &Value,
        instance: &Value,
        path: &mut Vec<String>,
        out: &mut Vec<ValidationError>,
        depth: usize,
    ) {
        let Value::Array(items) = instance else {
            return;
        };
        let count = items.len() as u64;
        if let Some(limit) = schema.get("minItems").and_then(Value::as_u64)
            && count < limit
        {
            Self::fail(
                path,
                out,
                ErrorKind::MinItems { limit },
                format!(
                    "{} has less than {limit} item{}",
                    show(instance),
                    plural(limit)
                ),
            );
        }
        if let Some(limit) = schema.get("maxItems").and_then(Value::as_u64)
            && count > limit
        {
            Self::fail(
                path,
                out,
                ErrorKind::MaxItems { limit },
                format!(
                    "{} has more than {limit} item{}",
                    show(instance),
                    plural(limit)
                ),
            );
        }
        if schema.get("uniqueItems") == Some(&Value::Bool(true))
            && items
                .iter()
                .enumerate()
                .any(|(i, a)| items[..i].iter().any(|b| equal(a, b)))
        {
            Self::fail(
                path,
                out,
                ErrorKind::UniqueItems,
                format!("{} has non-unique elements", show(instance)),
            );
        }
        if let Some(contains) = schema.get("contains")
            && !items
                .iter()
                .any(|item| self.is_valid_under(contains, item, depth))
        {
            Self::fail(
                path,
                out,
                ErrorKind::Contains,
                format!(
                    "None of {} are valid under the given schema",
                    show(instance)
                ),
            );
        }
        let prefix = match schema.get("prefixItems") {
            Some(Value::Array(prefix)) => prefix.as_slice(),
            _ => &[],
        };
        for (index, (sub, item)) in prefix.iter().zip(items).enumerate() {
            path.push(index.to_string());
            self.check(sub, item, path, out, depth);
            path.pop();
        }
        if let Some(sub) = schema.get("items") {
            for (index, item) in items.iter().enumerate().skip(prefix.len()) {
                path.push(index.to_string());
                self.check(sub, item, path, out, depth);
                path.pop();
            }
        }
    }

    // One block per keyword, in the order the keywords are documented.
    #[allow(clippy::too_many_lines)]
    fn check_object(
        &self,
        schema: &Value,
        instance: &Value,
        path: &mut Vec<String>,
        out: &mut Vec<ValidationError>,
        depth: usize,
    ) {
        let Value::Object(map) = instance else { return };
        let count = map.len() as u64;
        if let Some(limit) = schema.get("minProperties").and_then(Value::as_u64)
            && count < limit
        {
            Self::fail(
                path,
                out,
                ErrorKind::MinProperties { limit },
                format!(
                    "{} has less than {limit} propert{}",
                    show(instance),
                    if limit == 1 { "y" } else { "ies" }
                ),
            );
        }
        if let Some(limit) = schema.get("maxProperties").and_then(Value::as_u64)
            && count > limit
        {
            Self::fail(
                path,
                out,
                ErrorKind::MaxProperties { limit },
                format!(
                    "{} has more than {limit} propert{}",
                    show(instance),
                    if limit == 1 { "y" } else { "ies" }
                ),
            );
        }
        if let Some(Value::Array(required)) = schema.get("required") {
            for name in required.iter().filter_map(Value::as_str) {
                if !map.contains_key(name) {
                    Self::fail(
                        path,
                        out,
                        ErrorKind::Required {
                            property: name.to_string(),
                        },
                        format!("\"{name}\" is a required property"),
                    );
                }
            }
        }
        if let Some(Value::Object(dependent)) = schema.get("dependentRequired") {
            for (trigger, needed) in dependent.iter() {
                if map.contains_key(trigger)
                    && let Value::Array(needed) = needed
                {
                    for name in needed.iter().filter_map(Value::as_str) {
                        if !map.contains_key(name) {
                            Self::fail(
                                path,
                                out,
                                ErrorKind::Required {
                                    property: name.to_string(),
                                },
                                format!("\"{name}\" is a required property"),
                            );
                        }
                    }
                }
            }
        }
        if let Some(names) = schema.get("propertyNames") {
            for key in map.keys() {
                let mut problems = Vec::new();
                self.check(
                    names,
                    &Value::String(key.clone()),
                    &mut Vec::new(),
                    &mut problems,
                    depth,
                );
                for problem in problems {
                    Self::fail(path, out, ErrorKind::PropertyNames, problem.message);
                }
            }
        }

        let properties = match schema.get("properties") {
            Some(Value::Object(p)) => Some(p),
            _ => None,
        };
        let pattern_properties = match schema.get("patternProperties") {
            Some(Value::Object(p)) => Some(p),
            _ => None,
        };
        for (name, sub) in properties.into_iter().flat_map(Map::iter) {
            if let Some(value) = map.get(name) {
                path.push(name.clone());
                self.check(sub, value, path, out, depth);
                path.pop();
            }
        }
        for (pattern, sub) in pattern_properties.into_iter().flat_map(Map::iter) {
            let Some(regex) = self.patterns.get(pattern) else {
                continue;
            };
            for (name, value) in map.iter().filter(|(name, _)| regex.is_match(name)) {
                path.push(name.clone());
                self.check(sub, value, path, out, depth);
                path.pop();
            }
        }
        if let Some(additional) = schema.get("additionalProperties") {
            let unmatched = |name: &String| {
                properties.is_none_or(|p| !p.contains_key(name))
                    && pattern_properties.is_none_or(|p| {
                        !p.keys().any(|pattern| {
                            self.patterns.get(pattern).is_some_and(|r| r.is_match(name))
                        })
                    })
            };
            if matches!(additional, Value::Bool(false)) {
                let unexpected: Vec<String> =
                    map.keys().filter(|n| unmatched(n)).cloned().collect();
                if !unexpected.is_empty() {
                    let quoted: Vec<String> = unexpected.iter().map(|n| format!("'{n}'")).collect();
                    let verb = if unexpected.len() == 1 { "was" } else { "were" };
                    Self::fail(
                        path,
                        out,
                        ErrorKind::AdditionalProperties { unexpected },
                        format!(
                            "Additional properties are not allowed ({} {verb} unexpected)",
                            quoted.join(", ")
                        ),
                    );
                }
            } else if !matches!(additional, Value::Bool(true)) {
                for (name, value) in map.iter().filter(|(n, _)| unmatched(n)) {
                    path.push(name.clone());
                    self.check(additional, value, path, out, depth);
                    path.pop();
                }
            }
        }
    }

    fn check_combinators(
        &self,
        schema: &Value,
        instance: &Value,
        path: &mut Vec<String>,
        out: &mut Vec<ValidationError>,
        depth: usize,
    ) {
        if let Some(Value::Array(branches)) = schema.get("allOf") {
            for branch in branches {
                self.check(branch, instance, path, out, depth);
            }
        }
        if let Some(Value::Array(branches)) = schema.get("anyOf")
            && !branches
                .iter()
                .any(|b| self.is_valid_under(b, instance, depth))
        {
            Self::fail(
                path,
                out,
                ErrorKind::AnyOf,
                format!(
                    "{} is not valid under any of the schemas listed in the 'anyOf' keyword",
                    show(instance)
                ),
            );
        }
        if let Some(Value::Array(branches)) = schema.get("oneOf") {
            let valid = branches
                .iter()
                .filter(|b| self.is_valid_under(b, instance, depth))
                .count();
            if valid == 0 {
                Self::fail(
                    path,
                    out,
                    ErrorKind::OneOfNotValid,
                    format!(
                        "{} is not valid under any of the schemas listed in the 'oneOf' keyword",
                        show(instance)
                    ),
                );
            } else if valid > 1 {
                Self::fail(
                    path,
                    out,
                    ErrorKind::OneOfMultipleValid,
                    format!(
                        "{} is valid under more than one of the schemas listed in the 'oneOf' keyword",
                        show(instance)
                    ),
                );
            }
        }
        if let Some(not) = schema.get("not")
            && self.is_valid_under(not, instance, depth)
        {
            Self::fail(
                path,
                out,
                ErrorKind::Not,
                format!("{} is not allowed for {}", show(not), show(instance)),
            );
        }
        if let Some(condition) = schema.get("if") {
            let branch = if self.is_valid_under(condition, instance, depth) {
                schema.get("then")
            } else {
                schema.get("else")
            };
            if let Some(branch) = branch {
                self.check(branch, instance, path, out, depth);
            }
        }
    }
}

/// The instance as compact JSON, for messages.
fn show(value: &Value) -> String {
    json::to_string(value)
}

/// A numeric limit as written in the schema (so `5` stays `5`, not `5.0`).
fn show_f64(schema: &Value, keyword: &str, fallback: f64) -> String {
    schema
        .get(keyword)
        .map_or_else(|| fallback.to_string(), show)
}

const fn plural(n: u64) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// `"a"`, `"a" or "b"`, `"a", "b" or "c"`, and for longer lists the first two and a count.
fn list_options(options: &[Value]) -> String {
    let shown: Vec<String> = options.iter().map(show).collect();
    match shown.len() {
        0 => String::new(),
        1 => shown[0].clone(),
        2 | 3 => {
            let (last, rest) = shown
                .split_last()
                .map_or(("", &[][..]), |(l, r)| (l.as_str(), r));
            format!("{} or {last}", rest.join(", "))
        }
        n => format!("{}, {} or {} other candidates", shown[0], shown[1], n - 2),
    }
}

fn type_rank(name: &str) -> usize {
    [
        "null", "boolean", "integer", "number", "string", "array", "object",
    ]
    .iter()
    .position(|n| *n == name)
    .unwrap_or(usize::MAX)
}

fn has_type(instance: &Value, name: &str) -> bool {
    match name {
        "null" => instance.is_null(),
        "boolean" => instance.is_bool(),
        "string" => instance.is_string(),
        "array" => instance.is_array(),
        "object" => instance.is_object(),
        "number" => instance.is_number(),
        "integer" => match instance {
            Value::Int(_) | Value::UInt(_) => true,
            Value::Float(f) => f.is_finite() && f.fract() == 0.0,
            _ => false,
        },
        _ => false,
    }
}

/// JSON equality, where `1` and `1.0` are the same number.
fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| equal(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| equal(v, w)))
        }
        _ if a.is_number() && b.is_number() => match (a, b) {
            (Value::Float(_), _) | (_, Value::Float(_)) => a.as_f64() == b.as_f64(),
            _ => a == b,
        },
        _ => a == b,
    }
}

fn is_multiple(instance: &Value, divisor_value: Option<&Value>, number: f64, divisor: f64) -> bool {
    if let (Some(Value::Int(_) | Value::UInt(_)), Some(Value::Int(_) | Value::UInt(_))) =
        (Some(instance), divisor_value)
    {
        return match (instance.as_i64(), divisor_value.and_then(Value::as_i64)) {
            (Some(a), Some(b)) if b != 0 => a % b == 0,
            _ => false,
        };
    }
    let quotient = number / divisor;
    quotient.is_finite() && (quotient - quotient.round()).abs() <= 1e-9 * quotient.abs().max(1.0)
}

/// Resolves a local reference (`#`, `#/definitions/x`).
fn resolve<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    let pointer = reference.strip_prefix('#')?;
    if pointer.is_empty() {
        return Some(root);
    }
    root.pointer(pointer)
}

// The arms are grouped by keyword; several keywords simply have nothing to compile.
#[allow(clippy::match_same_arms)]
/// Walks the schema, compiling every `pattern` / `patternProperties` key and checking keyword shapes.
fn collect(
    schema: &Value,
    patterns: &mut HashMap<String, Regex>,
    depth: usize,
) -> Result<(), SchemaError> {
    let bad = |m: String| SchemaError(m);
    if depth > MAX_DEPTH {
        return Err(bad("nested too deeply".into()));
    }
    let Value::Object(map) = schema else {
        return match schema {
            Value::Bool(_) => Ok(()),
            _ => Err(bad("a schema is an object or a boolean".into())),
        };
    };
    let compile =
        |pattern: &str, patterns: &mut HashMap<String, Regex>| -> Result<(), SchemaError> {
            if !patterns.contains_key(pattern) {
                let regex = Regex::new(pattern)
                    .map_err(|e| bad(format!("`{pattern}` is not a valid pattern: {e}")))?;
                patterns.insert(pattern.to_string(), regex);
            }
            Ok(())
        };
    for (keyword, value) in map.iter() {
        match (keyword.as_str(), value) {
            ("pattern", Value::String(p)) => compile(p, patterns)?,
            ("pattern", _) => return Err(bad("`pattern` is a string".into())),
            ("type", Value::String(_)) => {}
            ("type", Value::Array(names)) if names.iter().all(Value::is_string) => {}
            ("type", _) => return Err(bad("`type` is a string or an array of strings".into())),
            ("required", Value::Array(names)) if names.iter().all(Value::is_string) => {}
            ("required", _) => return Err(bad("`required` is an array of strings".into())),
            ("enum", Value::Array(_)) => {}
            ("enum", _) => return Err(bad("`enum` is an array".into())),
            (
                "properties" | "patternProperties" | "dependentRequired" | "$defs" | "definitions",
                Value::Object(subs),
            ) => {
                for (name, sub) in subs.iter() {
                    if keyword == "patternProperties" {
                        compile(name, patterns)?;
                    }
                    if keyword != "dependentRequired" {
                        collect(sub, patterns, depth + 1)?;
                    }
                }
            }
            (
                "properties" | "patternProperties" | "dependentRequired" | "$defs" | "definitions",
                _,
            ) => {
                return Err(bad(format!("`{keyword}` is an object")));
            }
            ("allOf" | "anyOf" | "oneOf" | "prefixItems", Value::Array(subs)) => {
                for sub in subs {
                    collect(sub, patterns, depth + 1)?;
                }
            }
            ("allOf" | "anyOf" | "oneOf" | "prefixItems", _) => {
                return Err(bad(format!("`{keyword}` is an array of schemas")));
            }
            (
                "items"
                | "additionalProperties"
                | "not"
                | "contains"
                | "propertyNames"
                | "if"
                | "then"
                | "else",
                sub,
            ) => {
                collect(sub, patterns, depth + 1)?;
            }
            (
                "minLength" | "maxLength" | "minItems" | "maxItems" | "minProperties"
                | "maxProperties",
                v,
            ) if v.as_u64().is_none() => {
                return Err(bad(format!("`{keyword}` is a non-negative integer")));
            }
            ("minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" | "multipleOf", v)
                if !v.is_number() =>
            {
                return Err(bad(format!("`{keyword}` is a number")));
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
quench_test::main!();
