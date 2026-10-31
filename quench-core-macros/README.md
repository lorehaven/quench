# quench-core-macros
Proc macros for `quench-core`: `#[async_trait]` and `#[derive(Error)]`.

## What it is
`quench-core-macros` is the procedural-macro crate behind two items re-exported by `quench-core`: the attribute `#[async_trait]` and the derive `#[derive(Error)]`. It replaces `async-trait` and `thiserror`. Use the macros through `quench_core`; the crate is not meant to be depended on directly.

`#[async_trait]` lets a trait and its impls declare `async fn`s while staying usable as `dyn Trait`: each `async fn` becomes a method returning a boxed future (`Send` by default, or not with `#[async_trait(?Send)]`). It depends only on `proc-macro2`, `quote` and `syn`.

## Use cases
- Declare async methods on a trait object.
- Define a library error enum with `#[error("...")]` messages.
- Wrap a source error transparently with `#[from]` and `#[error(transparent)]`.
- Drop the `Send` bound for single-threaded futures with `#[async_trait(?Send)]`.

## Example
```rust
use quench_core::Error;

#[derive(Debug, Error)]
enum LoadError {
    #[error("cannot read {path}: {source}")]
    Read { path: String, source: std::io::Error },
    #[error("bad value {0}")]
    Bad(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

Apply `#[quench_core::async_trait]` to a trait and to every impl of it:

```rust
#[quench_core::async_trait]
trait Store {
    async fn get(&self, key: &str) -> Option<String>;
}
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-core-macros/
