---
title: Generated code
parent: quench-http-macros
grand_parent: Crates
nav_order: 3
---

Source: `quench-http-macros/src/expand.rs`. `lib.rs` only forwards to `expand::route` / `expand::injectable`, which operate on `proc_macro2` tokens so they can be unit tested.

## `#[get("/p")] async fn f(a: A, b: B) -> R { .. }`

Expands to (names exact, formatting normalised):

```rust
async fn f(a: A, b: B) -> R { .. }              // the original item, unchanged

#[allow(non_camel_case_types)]
struct __quench_route_f;

#[::quench_http::async_trait]
impl ::quench_http::endpoint::Endpoint for __quench_route_f {
    async fn call(&self, mut req: ::quench_http::request::Request) -> ::quench_http::response::Response {
        let __arg0 = match <A as ::quench_http::extract::FromRequest>::from_request(&mut req).await {
            ::std::result::Result::Ok(v) => v,
            ::std::result::Result::Err(e) => {
                return ::quench_http::endpoint::IntoResponse::into_response(e);
            }
        };
        let __arg1 = /* same for B */;
        let __result = f(__arg0, __arg1).await;     // no .await for a non-async fn
        ::quench_http::endpoint::IntoResponse::into_response(__result)
    }
}

::quench_http::submit!(
    ::quench_http::route::ROUTES,
    ::quench_http::route::RouteRegistration,
    ::quench_http::route::RouteRegistration {
        method: ::quench_http::http::Method::GET,
        pattern: "/p",
        endpoint: || ::std::sync::Arc::new(__quench_route_f) as ::std::sync::Arc<dyn ::quench_http::endpoint::Endpoint>,
    }
);
```

Notes: the struct is private and unit-like; it is a new `Endpoint` instance per `endpoint()` call. Extractors run in parameter order, so the first failing one decides the response. `Method::GET/POST/PUT/DELETE/PATCH` are `quench_proto::Method` associated constants. The generated items sit next to the function, so the handler can be anywhere items are allowed.

## `#[injectable] async fn f(d0: Arc<D0>, ..) -> T`

```rust
async fn f(d0: Arc<D0>, ..) -> T { .. }          // the original item

fn __quench_construct_f(container: &::quench_http::di::Container) -> ::quench_http::di::ConstructFuture {
    let container = ::std::clone::Clone::clone(container);
    ::std::boxed::Box::pin(async move {
        let __dep0 = match container.get::<D0>() { Ok(v) => v, Err(e) => return Err(e) };
        let __value = f(__dep0, ..).await;           // for Result<T,E>: match, Err(e) => ConstructionFailed
        ::std::result::Result::Ok(::std::sync::Arc::new(__value) as ::quench_http::di::BoxedAny)
    })
}

::quench_http::submit!(
    ::quench_http::di::COMPONENTS,
    ::quench_http::di::ComponentFactory,
    ::quench_http::di::ComponentFactory {
        type_id: || ::std::any::TypeId::of::<T>(),
        type_name: || ::std::any::type_name::<T>(),
        dependencies: || ::std::vec![ (TypeId::of::<D0>(), type_name::<D0>()), .. ],
        construct: __quench_construct_f,
    }
);
```

Everything is a function pointer or closure without captures because `submit!` places the value in a `static` initialiser. For a fallible constructor the `Ok` type is registered and the failure is `::quench_http::error::DiError::ConstructionFailed { component: type_name::<T>(), reason: e.to_string() }`.

## Helper functions

* `arc_inner(&Type) -> Option<Type>`: path type whose last segment is `Arc` with angle-bracketed args; returns the first type argument.
* `result_inner(&Type) -> (Type, bool)`: if the last segment is `Result` with a first type argument, `(T, true)`, otherwise `(ty, false)`.

Both are private to the crate; they are purely syntactic (a user type named `Arc` or `Result` is treated the same way).
