---
title: quench-template
parent: Crates
has_children: true
permalink: /crates/quench-template/
nav_order: 47
---
{% raw %}
`quench-template` (library `quench_template`) is a Jinja-compatible template engine over `quench_serde::Value`. It replaces `minijinja` and removes the serde bridge: the render context, globals and filter arguments are plain `Value`s. Templates are parsed once into a tree and rendered by a small interpreter; output is never escaped.

## Facts

| | |
|---|---|
| Version | 0.1.3 |
| Edition | 2024 |
| License | MIT |
| Features | none |
| Dependencies | `quench-serde` (the `Value` type and the `json!` macro; re-exported as `quench_template::Value`); dev: `quench-test` |
| Test harness | `autotests = false`; one integration test `template` with `harness = false` |
| Thread safety | `Environment` holds boxed `Send + Sync` closures and a `Mutex` cache, so it is `Send + Sync`; rendering takes `&self` |

```toml
[dependencies]
quench-template = { version = "0.1.3", registry = "ennor" }
quench-serde = { version = "0.1", registry = "ennor" }   # for json! and Value
```

## Where to go next

* [Usage](usage.md): rendering, registering templates, loaders, custom filters, tests, functions and formatters.
* [Language reference](language.md): every tag, expression form, operator and the whitespace rules.
* [Built-ins](builtins.md): every filter, test, function and method, with examples.
* [API reference](api.md)
* [Errors](errors.md)
* [Internals](internals.md): scoping, undefined values, limits.
* [Testing](testing.md)

## Minimal example

```rust
use quench_serde::json;
use quench_template::Environment;

let mut env = Environment::new();
env.add_template("hello", "Hello {{ user.name|upper }}!{% for t in tags %} #{{ t }}{% endfor %}").unwrap();
let out = env.render("hello", &json!({ "user": { "name": "ada" }, "tags": ["a", "b"] })).unwrap();
assert_eq!(out, "Hello ADA! #a #b");
```

## What it is not

No template inheritance (`extends`, `block`), no `call` blocks, no `filter` blocks, no autoescaping (the `escape`, `e`, `safe` and `forceescape` filters return their input unchanged), no `loop.cycle`, no recursive loops, no line statements, no custom delimiters. All of these fail with `unknown tag` or a missing-name error (see [language](language.md#not-supported)).
{% endraw %}
