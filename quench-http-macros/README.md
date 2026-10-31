# quench-http-macros
Proc macros for `quench-http`: route and injectable discovery.

## What it is
`quench-http-macros` is the procedural-macro crate behind `quench-http`'s annotation-driven discovery: five route attributes (`#[get]`, `#[post]`, `#[put]`, `#[delete]`, `#[patch]`) and `#[injectable]`. You do not depend on it directly: `quench_http::prelude` re-exports all six.

Generated code refers only to `::quench_http` paths, so the using crate needs `quench-http` as a direct dependency under that name. Route handlers are registered for `discover_routes()`; injectable functions are registered as components of the dependency-injection container.

## Use cases
- Register a handler for a method and path pattern such as `/users/{id}`.
- Declare a component built by an async or sync function, with other components as `Arc<Dep>` parameters.
- Construct components that can fail, by returning `Result<T, E>`.

## Example
```rust
use quench_http::prelude::*;

#[injectable]
async fn build_user_db(db: Arc<Db>) -> UserDb { UserDb::init(db).await }

#[get("/users/{id}")]
async fn get_user(Path(p): Path<UserPath>, Inject(db): Inject<UserDb>) -> Result<String, HttpError> {
    Ok(format!("{}", p.id))
}
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-http-macros/
