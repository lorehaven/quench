---
title: Usage
parent: quench-template
grand_parent: Crates
nav_order: 1
---
{% raw %}
## 1. Render once

```rust
use quench_serde::json;
use quench_template::Environment;

let env = Environment::new();
let out = env.render_str("{{ greeting }}, {{ who|title }}!", &json!({"greeting": "Hello", "who": "ada"})).unwrap();
assert_eq!(out, "Hello, Ada!");
```

`render_str` parses the text every call. For repeated use register a template.

## 2. Named templates, globals, includes

```rust
use quench_serde::json;
use quench_template::Environment;

let mut env = Environment::new();
env.add_global("env", "prod");
env.add_template("part.j2", "[{{ who }}]").unwrap();
env.add_template("page.j2", "{{ env }}: {% include 'part.j2' %}").unwrap();
assert_eq!(env.render("page.j2", &json!({"who": "me"})).unwrap(), "prod: [me]");
```

A context entry of the same name as a global wins. The context should be a map; anything else means no variables.

## 3. Load templates on demand

```rust
use quench_serde::json;
use quench_template::Environment;

let mut env = Environment::new();
env.set_loader(|name| Ok((name == "dyn.j2").then(|| "dynamic {{ env }}".to_string())));
assert_eq!(env.render("dyn.j2", &json!({"env": "ctx"})).unwrap(), "dynamic ctx");
```

The loader is used by `import`, `include` and `render` for names not added with `add_template`; its results (and parses) are cached per name until `set_loader` is called again. Return `Ok(None)` for "not found" (the error is then `TemplateNotFound`) or `Err(..)` for a real failure.

## 4. Custom filters, tests and functions

```rust
use quench_serde::{Value, json};
use quench_template::{Args, Environment};

let mut env = Environment::new();
env.add_filter("shout", |v, args| {
    let suffix = args.get(0, "suffix").and_then(Value::as_str).unwrap_or("!");
    Ok(Value::String(format!("{}{suffix}", v.as_str().unwrap_or_default().to_uppercase())))
});
env.add_test("big", |v, _| Ok(v.as_i64().is_some_and(|n| n > 100)));
env.add_function("twice", |args: &Args| Ok(Value::from(args.positional[0].as_i64().unwrap_or(0) * 2)));
let out = env.render_str("{{ 'a'|shout }} {{ 'a'|shout('?') }} {{ 500 is big }} {{ twice(4) }}", &json!({})).unwrap();
assert_eq!(out, "A! A? true 8");
```

`Args::get(index, "name")` returns positional argument `index`, else the keyword `name`. Return `Err(Error::new(ErrorKind::Custom, "message"))` to fail the render.

## 5. Control how values print

```rust
use quench_serde::json;
use quench_template::Environment;

let mut env = Environment::new();
env.set_formatter(|v| matches!(v, quench_serde::Value::Bool(_)).then(|| format!("<{}>", v.as_bool().unwrap())));
assert_eq!(env.render_str("{{ true }} {{ 'x' }}", &json!({})).unwrap(), "<true> x");
```

The formatter returns `Some(text)` or `None` for the default. It applies to every `{{ }}` output (not to `~` or `|string`, which use the default form).

## 6. Macros and imports

```rust
use quench_serde::json;
use quench_template::Environment;

let mut env = Environment::new();
env.add_template("macros.j2",
    "{% macro greet(name, greeting='Hello') %}{{ greeting }}, {{ name }}!{% endmacro %}{% set shared = 5 %}").unwrap();
env.add_template("main.j2",
    "{% import 'macros.j2' as m %}{{ m.greet('Ada') }} {{ m.shared }}{% from 'macros.j2' import greet as g %} {{ g('D', greeting='Hi') }}").unwrap();
assert_eq!(env.render("main.j2", &json!({})).unwrap(), "Hello, Ada! 5 Hi, D!");
```

## 7. Errors with a place

```rust
use quench_serde::json;
use quench_template::{Environment, ErrorKind};

let env = Environment::new();
let e = env.render_str("a\n{{ 1 + }}", &json!({})).unwrap_err();
assert_eq!(e.kind(), ErrorKind::Syntax);
assert_eq!(e.line(), 2);
assert_eq!(e.to_string(), "syntax error: unexpected None in an expression (in <string>:2)");
```

## Pitfalls

* Output is not escaped. If you render HTML, escape values yourself before they enter the context.
* `{% set %}` inside a `for` does not change variables outside the loop.
* `loop` exists only inside a `for`.
* Filters never mutate; `|sort` returns a new list.
* `{{ x.y }}` on an undefined `x` is an error; use `{{ x.y|default("") }}` or test `x is defined` first.
{% endraw %}
