# quench-template
A Jinja-compatible template engine over `quench-serde` values: expressions, filters, tests, loops, macros, imports.

## What it is
`quench-template` is a Jinja-compatible template engine over `quench_serde::Value`. It replaces `minijinja` and removes the serde bridge: the render context, globals and filter arguments are plain `Value`s. Templates are parsed once into a tree and rendered by a small interpreter; output is never escaped.

It depends on `quench-serde` for `Value` and the `json!` macro. `Environment` is `Send + Sync` and rendering takes `&self`.

## Use cases
- Render one-off strings with `render_str`.
- Register named templates with includes, globals, macros and imports.
- Load templates on demand with a loader.
- Add custom filters, tests and functions.
- Control how values print with a formatter.

## Example
```rust
use quench_serde::json;
use quench_template::Environment;

let mut env = Environment::new();
env.add_global("env", "prod");
env.add_template("part.j2", "[{{ who }}]").unwrap();
env.add_template("page.j2", "{{ env }}: {% include 'part.j2' %}").unwrap();
assert_eq!(env.render("page.j2", &json!({"who": "me"})).unwrap(), "prod: [me]");
```

```rust
use quench_serde::{Value, json};
use quench_template::Environment;

let mut env = Environment::new();
env.add_filter("shout", |v, _args| {
    Ok(Value::String(v.as_str().unwrap_or_default().to_uppercase()))
});
assert_eq!(env.render_str("{{ 'a'|shout }}", &json!({})).unwrap(), "A");
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-template/
