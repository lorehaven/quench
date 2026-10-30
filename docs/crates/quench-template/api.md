---
title: API reference
parent: quench-template
grand_parent: Crates
nav_order: 2
---
{% raw %}
Every public item of `quench_template`. No `unsafe`, no feature gates.

## Re-exports

```rust
pub use builtins::Args;
pub use error::{Error, ErrorKind, Result};
pub use quench_serde::Value;
```

## `struct Template`

```rust
#[derive(Debug)]
pub struct Template { /* name, parsed body */ }

impl Template {
    #[must_use] pub fn name(&self) -> &str
}
```

A parsed template. Obtained from `Environment::template`; it has no render method of its own (render through the environment). The name is the one it was registered or loaded under, `<string>` for `render_str`.

## `struct Environment`

`#[derive(Default)]`; implements `Debug` (prints `Environment(N templates)`, counting only templates added with `add_template`). Holds registered templates, an optional loader and its cache, globals, filters, tests, functions and a formatter. `Send + Sync`.

```rust
impl Environment {
    #[must_use] pub fn new() -> Self
    pub fn add_template(&mut self, name: &str, source: &str) -> Result<()>
    pub fn set_loader<F>(&mut self, loader: F)
        where F: Fn(&str) -> Result<Option<String>> + Send + Sync + 'static
    pub fn add_global(&mut self, name: &str, value: impl Into<Value>)
    pub fn add_filter<F>(&mut self, name: &str, filter: F)
        where F: Fn(&Value, &Args) -> Result<Value> + Send + Sync + 'static
    pub fn add_test<F>(&mut self, name: &str, test: F)
        where F: Fn(&Value, &[Value]) -> Result<bool> + Send + Sync + 'static
    pub fn add_function<F>(&mut self, name: &str, function: F)
        where F: Fn(&Args) -> Result<Value> + Send + Sync + 'static
    pub fn set_formatter<F>(&mut self, formatter: F)
        where F: Fn(&Value) -> Option<String> + Send + Sync + 'static
    pub fn template(&self, name: &str) -> Result<Arc<Template>>
    pub fn render(&self, name: &str, context: &Value) -> Result<String>
    pub fn render_str(&self, source: &str, context: &Value) -> Result<String>
}
```

| Method | Behaviour |
|---|---|
| `new()` | an empty environment (same as `default()`). |
| `add_template(name, source)` | parses and registers; replaces an earlier template of the same name. Error: `ErrorKind::Syntax` (with the template name and line). |
| `set_loader(f)` | `f(name)` returns the source (`Some`), "not found" (`None`) or an error. Used by `template`, `render`, `import` and `include` for names not registered. Calling it clears the loader cache. Successfully parsed loaded templates are cached by name. |
| `add_global(name, value)` | a variable every template can read; a render-context entry or a `set` of the same name wins. |
| `add_filter(name, f)` | `{{ v\|name(args) }}` calls `f(&v, &args)`. Replaces a built-in of the same name. |
| `add_test(name, f)` | `{{ v is name(args) }}` calls `f(&v, &args)`; not called for undefined values; `defined`/`undefined` cannot be replaced. |
| `add_function(name, f)` | `{{ name(args) }}`, used only if `name` is not a variable. |
| `set_formatter(f)` | called for every printed value; `Some(text)` is printed, `None` falls back to the default form. Not called for undefined values (nothing is printed). |
| `template(name)` | the registered template, else the loader's (cached). Errors: `TemplateNotFound` (message `` `name` ``), the loader's error, or `Syntax`. |
| `render(name, context)` | `template(name)` then render. `context` is a `Value`; if it is a map its entries are variables, otherwise there are none. |
| `render_str(source, context)` | parses `source` (template name `<string>`) and renders it once. |

Rendering is single-threaded per call and takes `&self`; many threads may render with one shared environment.

## `struct Args`

```rust
#[derive(Debug, Default, Clone)]
pub struct Args {
    pub positional: Vec<Value>,
    pub keyword: Vec<(String, Value)>,
}

impl Args {
    #[must_use] pub fn get(&self, index: usize, name: &str) -> Option<&Value>
}
```

The arguments of a filter or function call (for a test, a plain `&[Value]`). `get(index, name)` is positional argument `index`, else the keyword argument called `name`. For a filter the filtered value is **not** in `positional`; it is the first closure parameter. Keyword arguments keep their call order.

## `enum ErrorKind`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind { Syntax, Undefined, InvalidOperation, TemplateNotFound, UnknownName, Custom }
```

See [errors](errors.md).

## `struct Error`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error { /* kind, message, template, line */ }

impl Error {
    #[must_use] pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self
    #[must_use] pub const fn kind(&self) -> ErrorKind
    #[must_use] pub fn message(&self) -> &str
    #[must_use] pub fn template(&self) -> Option<&str>
    #[must_use] pub const fn line(&self) -> usize
}
impl fmt::Display for Error
impl std::error::Error for Error
pub type Result<T> = std::result::Result<T, Error>;
```

`new` is what custom filters use (`Error::new(ErrorKind::Custom, "..")`); template and line are filled in by the engine as the error propagates (`template()` is `None` until then). `line()` is 1-based, 0 when unknown. `message()` is the text without the kind prefix and location.

## `Value`

`quench_template::Value` is `quench_serde::Value` (`Null`, `Bool`, `Int`, `UInt`, `Float`, `String`, `Array`, `Object`). Build contexts with `quench_serde::json!`. Integers above `i64::MAX` are `UInt`.
{% endraw %}
