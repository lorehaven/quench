---
title: Expansion
parent: quench-fullstack-macros
grand_parent: Crates
nav_order: 3
---

For `#[server(auth)] pub async fn add_note(text: String, done: bool) -> Result<Note, ServerError> { body }` the macro emits three groups of items (source: `quench-fullstack-macros/src/expand.rs`).

## 1. The function itself (server)

```rust
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::unused_async)]
pub async fn add_note(text: String, done: bool) -> Result<Note, ServerError> { body }
```

The original item, including its attributes and visibility, unchanged.

## 2. The endpoint and its registration (server)

```rust
#[cfg(not(target_arch = "wasm32"))]
const _: () = {
    #[derive(::quench_serde::Deserialize)]
    #[allow(non_camel_case_types, dead_code)]
    struct __QuenchArgs { text: String, done: bool }

    struct __QuenchEndpoint;

    #[::quench_fullstack::__private::async_trait]
    impl ::quench_fullstack::__private::Endpoint for __QuenchEndpoint {
        async fn call(&self, mut req: ::quench_fullstack::__private::Request)
            -> ::quench_fullstack::__private::Response
        {
            if let Some(refusal) = ::quench_fullstack::server::guard(&req) { return refusal; }
            // only with auth / auth = "required":
            let __user = match <::quench_fullstack::User as FromRequest>::from_request(&mut req).await {
                Ok(user) => Some(user),
                Err(err) => return ::quench_fullstack::ServerError::from_http(&err).into_response(),
            };
            // only with auth = "optional":
            //   let __user = <User as FromRequest>::from_request(&mut req).await.ok();
            ::quench_fullstack::server::dispatch(req, move |__args: __QuenchArgs| async move {
                // with auth:  ::quench_fullstack::server::with_user(__user, add_note(__args.text, __args.done)).await
                // without:    add_note(__args.text, __args.done).await
            }).await
        }
    }

    ::quench_fullstack::__private::submit!(
        ::quench_fullstack::server::FUNCTIONS,
        ::quench_fullstack::server::ServerFn,
        ::quench_fullstack::server::ServerFn {
            path: "/_fn/add_note",
            endpoint: || Arc::new(__QuenchEndpoint) as Arc<dyn Endpoint>,
        }
    );
};
```

Order at run time: `guard` (method, content type, RPC header, origin), user lookup (so a call without a session never reads its arguments), `dispatch` (body limit, argument decode, call). The registration runs before `main` (`quench_core::submit!`) and is collected by `UiApp::new`.

## 3. The browser stub (wasm32)

```rust
#[cfg(target_arch = "wasm32")]
#[allow(clippy::future_not_send)]
pub async fn add_note(text: String, done: bool) -> Result<Note, ServerError> {
    #[derive(::quench_serde::Serialize)]
    #[allow(non_camel_case_types)]
    struct __QuenchArgs { text: String, done: bool }
    ::quench_fullstack::client::call::<_, Note>("/_fn/add_note", &__QuenchArgs { text, done }).await
}
```

Same attributes (doc comments) and visibility as the original. The body is replaced: the arguments are serialized and sent to the route; the answer is decoded as `Note`.

## Notes

* A function without parameters gets an empty argument struct (`struct __QuenchArgs { }`); the request body is `{}`.
* The server item and the browser stub share one name, one signature, so shared code needs no `cfg`.
* Parameter types are repeated in the generated struct, so they must be nameable and `Deserialize`/`Serialize`; parameter types containing `impl Trait` would not compile in a struct field (the macro does not reject them).
