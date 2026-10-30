---
title: Attributes and derive
parent: quench-test-bdd-macros
grand_parent: Crates
nav_order: 1
---

## `#[given(..)]`, `#[when(..)]`, `#[then(..)]`

```rust
#[proc_macro_attribute] pub fn given(attr: TokenStream, item: TokenStream) -> TokenStream
#[proc_macro_attribute] pub fn when(attr: TokenStream, item: TokenStream) -> TokenStream
#[proc_macro_attribute] pub fn then(attr: TokenStream, item: TokenStream) -> TokenStream
```

Each registers the function as a step definition of kind `Given`, `When` or `Then`.

### Argument

| Form | Meaning |
|---|---|
| `#[given("a user {string}")]` | a cucumber expression (a bare string literal) |
| `#[given(expr = "a user {string}")]` | the same, explicit |
| `#[given(regex = r"^there are (\d+) users$")]` | a regular expression (not anchored for you) |

Syntax of both: [Gherkin and expression syntax](../quench-test-bdd/syntax.md).

### Function

* First parameter: `&mut World` (a mutable reference to the world type). The type is what the step belongs to.
* Further parameters: each takes the next capture, parsed with `FromStr` through `step::parse_argument::<T>` (error text names the parameter: for a parameter pattern that is not a plain identifier, `argument N`). Types may be anything `FromStr` with a `Display` error (the macro does not restrict the type; a type that is not `FromStr` fails to compile at the generated call).
* One parameter of type `&Step` (a shared reference to a path ending in `Step`) receives the step (doc string, table, line) and does not consume a capture.
* `fn` or `async fn`. The return value may be `()` or `Result<(), E>` with `E: Display` (via `StepOutcome`).
* The function is kept with `#[allow(dead_code)]`.

### Generated code

A private `__quench_step` that downcasts the world, parses the arguments, calls the function (`.await` if async) and converts the result, and a `submit!` of a `step::Definition { kind, pattern, regex, file, line, world: TypeId of the world, run }` into `quench_test_bdd::STEPS`.

### Compile errors

| Cause | Message |
|---|---|
| attribute is not a string, `expr = ".."` or `regex = ".."` | `` expected a string, `expr = ".."` or `regex = ".."` `` |
| a name-value that is not `expr`/`regex` | `` expected `expr` or `regex` `` |
| the value is not a string literal | `expected a string literal` |
| a path without value (`regex`) | `` expected `expr = ".."` or `regex = ".."` `` |
| no parameters, or a receiver | `` a step takes `&mut World` first `` / `unexpected receiver` |
| first parameter not `&mut T` (`&T`, `T`) | `` the first parameter must be `&mut World` `` |
| applied to something that is not a function | the `syn` error |

## `#[derive(World)]`

```rust
#[proc_macro_derive(World, attributes(world))]
pub fn derive_world(input: TokenStream) -> TokenStream
```

Implements `quench_test_bdd::World` for the type: each scenario gets a fresh value.

| Form | Meaning |
|---|---|
| `#[derive(World)]` | the type must implement `Default`; `new()` returns `Default::default()` |
| `#[world(init = path)]` | `path` names an `async fn() -> Self` (or anything callable `path()` returning a future of `Self`); `new()` awaits it |

Generics and `where` clauses are carried over. Errors: any other `#[world(..)]` option is ``unsupported option (use `init = path`)``; a non-type (a function) is the `syn` error. The generated impl is

```rust
impl ::quench_test_bdd::World for Name {
    fn new() -> ::quench_test_bdd::BoxFuture<'static, Self> { Box::pin(async { /* default() or (path)().await */ }) }
}
```

so the world type must be `Send + 'static` (the `World` trait bound).
