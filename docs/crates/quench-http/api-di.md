---
title: API - dependency injection
parent: quench-http
grand_parent: Crates
nav_order: 3
---

Module `di` (`quench-http/src/di/mod.rs`). `pub use crate::error::DiError;` (see [errors](errors.md)).

## Type aliases

```rust
pub type BoxedAny = Arc<dyn Any + Send + Sync>;
pub type ConstructFuture = Pin<Box<dyn Future<Output = Result<BoxedAny, DiError>> + Send>>;
```

`ConstructFuture` is the return type of a generated `#[injectable]` constructor; public only so macro output can name it.

## `pub struct ComponentFactory` (`#[derive(Debug)]`)

```rust
pub struct ComponentFactory {
    pub type_id: fn() -> TypeId,
    pub type_name: fn() -> &'static str,
    pub dependencies: fn() -> Vec<(TypeId, &'static str)>,
    pub construct: fn(&Container) -> ConstructFuture,
}
```

Registered by `#[injectable]`. Function pointers (not values) because `submit!` places the value in a `static` initialiser where `TypeId::of`, `type_name` and `vec!` are not const. `dependencies` lists `(TypeId, type_name)` of each `Arc<Dep>` parameter's `Dep`.

## `pub static COMPONENTS: Registry<ComponentFactory>`

Every `#[injectable]` in the linked binary.

## `pub struct Container`

`#[derive(Clone, Default)]`; `Debug` prints `Container { resolved_components: N }`. Holds `Arc<RwLock<HashMap<TypeId, BoxedAny>>>`; cloning shares the map. Handlers reach it through `Request::container()` or `Inject<T>`.

```rust
pub fn get<T: Any + Send + Sync + 'static>(&self) -> Result<Arc<T>, DiError>
```

Returns the component keyed by `TypeId::of::<T>()` or `DiError::NotFound(type_name::<T>())`. Panics (`expect("container lock poisoned")`) if the lock is poisoned. Insert helpers are private; the only ways in are the builder and `#[injectable]`.

## `pub struct ContainerBuilder`

`#[derive(Debug, Default)]`.

```rust
#[must_use] pub fn new() -> Self
#[must_use] pub fn provide<T: Any + Send + Sync + 'static>(self, value: T) -> Self
#[must_use] pub fn provide_arc<T: Any + Send + Sync + 'static>(self, value: Arc<T>) -> Self
pub async fn build(self) -> Result<Container, DiError>
```

* `provide` stores `Arc::new(value)` under `TypeId::of::<T>()`; calling it twice with the same type replaces the earlier value. `provide(arc)` keys the value as `Arc<T>`: use `provide_arc`.
* `build` resolves every component in `COMPONENTS` (see algorithm in [internals](internals.md#di-resolution)) and returns the container.

Errors from `build`: `MissingDependency` (a dependency is neither provided nor an `#[injectable]` return type), `ConstructionFailed` (a fallible constructor returned `Err`; `reason` is `e.to_string()`), `Cycle(names)` (some components never became constructible), and `NotFound` propagated from a constructor's dependency lookup.

Provided values satisfy dependencies. An `#[injectable]` whose return type was also provided is still run and its result overwrites the provided value.
