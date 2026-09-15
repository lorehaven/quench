//! Proc macros behind `quench-http`'s "discovery through annotations"
//! model:
//!
//! - `#[get]`/`#[post]`/`#[put]`/`#[delete]`/`#[patch]` turn a handler
//!   function into a discoverable route (registered via `inventory`, no
//!   `.service(handler)` list to maintain).
//! - `#[injectable]` turns a constructor function into a discoverable
//!   component in the dependency-injection graph.
//!
//! Generated code only ever references `::quench_http::...` paths (the
//! crate re-exports `async_trait`, `http`, and `inventory` for this
//! purpose), so a crate using these macros needs `quench-http` as its only
//! new dependency.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    FnArg, GenericArgument, ItemFn, LitStr, PathArguments, ReturnType, Type, parse_macro_input,
};

fn route_impl(method: &str, attr: TokenStream, item: TokenStream) -> TokenStream {
    let pattern = parse_macro_input!(attr as LitStr);
    let func = parse_macro_input!(item as ItemFn);
    let fn_name = &func.sig.ident;
    let struct_name = format_ident!("__quench_route_{}", fn_name);
    let method_ident = format_ident!("{}", method);

    let mut arg_bindings: Vec<TokenStream2> = Vec::new();
    let mut arg_idents: Vec<syn::Ident> = Vec::new();

    for (i, input) in func.sig.inputs.iter().enumerate() {
        let ty = match input {
            FnArg::Typed(pat_type) => &pat_type.ty,
            FnArg::Receiver(_) => {
                return syn::Error::new_spanned(input, "route handlers can't take `self`")
                    .to_compile_error()
                    .into();
            }
        };
        let arg_ident = format_ident!("__arg{}", i);
        arg_bindings.push(quote! {
            let #arg_ident = match <#ty as ::quench_http::extract::FromRequest>::from_request(&mut req).await {
                ::std::result::Result::Ok(v) => v,
                ::std::result::Result::Err(e) => {
                    return ::quench_http::endpoint::IntoResponse::into_response(e);
                }
            };
        });
        arg_idents.push(arg_ident);
    }

    let call_expr = if func.sig.asyncness.is_some() {
        quote! { #fn_name(#(#arg_idents),*).await }
    } else {
        quote! { #fn_name(#(#arg_idents),*) }
    };

    let expanded = quote! {
        #func

        #[allow(non_camel_case_types)]
        struct #struct_name;

        #[::quench_http::async_trait::async_trait]
        impl ::quench_http::endpoint::Endpoint for #struct_name {
            async fn call(&self, mut req: ::quench_http::request::Request) -> ::quench_http::response::Response {
                #(#arg_bindings)*
                let __result = #call_expr;
                ::quench_http::endpoint::IntoResponse::into_response(__result)
            }
        }

        ::quench_http::inventory::submit! {
            ::quench_http::route::RouteRegistration {
                method: ::quench_http::http::Method::#method_ident,
                pattern: #pattern,
                endpoint: || ::std::sync::Arc::new(#struct_name) as ::std::sync::Arc<dyn ::quench_http::endpoint::Endpoint>,
            }
        }
    };

    expanded.into()
}

/// Discoverable `GET` route: `#[get("/users/{id}")]`. Path patterns use
/// actix-router syntax, including regex-constrained multi-segment segments
/// (`{name:.+}`) for cases like the docker registry API where a captured
/// segment has to swallow further `/`s.
#[proc_macro_attribute]
pub fn get(attr: TokenStream, item: TokenStream) -> TokenStream {
    route_impl("GET", attr, item)
}

/// Discoverable `POST` route. See [`macro@get`].
#[proc_macro_attribute]
pub fn post(attr: TokenStream, item: TokenStream) -> TokenStream {
    route_impl("POST", attr, item)
}

/// Discoverable `PUT` route. See [`macro@get`].
#[proc_macro_attribute]
pub fn put(attr: TokenStream, item: TokenStream) -> TokenStream {
    route_impl("PUT", attr, item)
}

/// Discoverable `DELETE` route. See [`macro@get`].
#[proc_macro_attribute]
pub fn delete(attr: TokenStream, item: TokenStream) -> TokenStream {
    route_impl("DELETE", attr, item)
}

/// Discoverable `PATCH` route. See [`macro@get`].
#[proc_macro_attribute]
pub fn patch(attr: TokenStream, item: TokenStream) -> TokenStream {
    route_impl("PATCH", attr, item)
}

/// Extracts `T` out of `Arc<T>` (accepting any path ending in `Arc`, so
/// `std::sync::Arc<T>` and a bare `Arc<T>` both work).
fn arc_inner(ty: &Type) -> Option<Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let last = type_path.path.segments.last()?;
    if last.ident != "Arc" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    args.args.iter().find_map(|arg| match arg {
        GenericArgument::Type(t) => Some(t.clone()),
        _ => None,
    })
}

/// Extracts `T` out of `Result<T, E>`, or returns `ty` unchanged (and
/// `false`) if it isn't a `Result`.
fn result_inner(ty: &Type) -> (Type, bool) {
    if let Type::Path(type_path) = ty
        && let Some(last) = type_path.path.segments.last()
        && last.ident == "Result"
        && let PathArguments::AngleBracketed(args) = &last.arguments
        && let Some(GenericArgument::Type(t)) = args.args.first()
    {
        return (t.clone(), true);
    }
    (ty.clone(), false)
}

/// Registers a constructor function as a component in the dependency
/// graph. Every parameter must be `Arc<Dep>` - the container resolves
/// `Dep` (another `#[injectable]`, or a value seeded via
/// `ContainerBuilder::provide`) and passes it in; the return type (bare
/// `T`, or `Result<T, E>` for a fallible constructor) is what other
/// components can depend on:
///
/// ```ignore
/// #[injectable]
/// async fn build_user_db(db: Arc<Db>) -> UserDb {
///     UserDb::init(db).await
/// }
/// ```
#[proc_macro_attribute]
pub fn injectable(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let fn_name = &func.sig.ident;
    let construct_fn_name = format_ident!("__quench_construct_{}", fn_name);

    let ReturnType::Type(_, ret_ty) = &func.sig.output else {
        return syn::Error::new_spanned(
            &func.sig,
            "#[injectable] functions must return a type (optionally `Result<T, E>`)",
        )
        .to_compile_error()
        .into();
    };
    let (target_ty, is_result) = result_inner(ret_ty);

    let mut dep_types: Vec<Type> = Vec::new();
    let mut arg_idents: Vec<syn::Ident> = Vec::new();

    for (i, input) in func.sig.inputs.iter().enumerate() {
        let ty = match input {
            FnArg::Typed(pat_type) => &pat_type.ty,
            FnArg::Receiver(_) => {
                return syn::Error::new_spanned(input, "#[injectable] functions can't take `self`")
                    .to_compile_error()
                    .into();
            }
        };
        let Some(dep_ty) = arc_inner(ty) else {
            return syn::Error::new_spanned(
                ty,
                "#[injectable] constructor parameters must be `Arc<Dependency>` - that's the only \
                 dependency shape the container resolves",
            )
            .to_compile_error()
            .into();
        };
        dep_types.push(dep_ty);
        arg_idents.push(format_ident!("__dep{}", i));
    }

    let dep_gets: Vec<TokenStream2> = arg_idents
        .iter()
        .zip(dep_types.iter())
        .map(|(ident, ty)| {
            quote! {
                let #ident = match container.get::<#ty>() {
                    ::std::result::Result::Ok(v) => v,
                    ::std::result::Result::Err(e) => return ::std::result::Result::Err(e),
                };
            }
        })
        .collect();

    let call_expr = if func.sig.asyncness.is_some() {
        quote! { #fn_name(#(#arg_idents),*).await }
    } else {
        quote! { #fn_name(#(#arg_idents),*) }
    };

    let value_expr = if is_result {
        quote! {
            match #call_expr {
                ::std::result::Result::Ok(v) => v,
                ::std::result::Result::Err(e) => {
                    return ::std::result::Result::Err(::quench_http::error::DiError::ConstructionFailed {
                        component: ::std::any::type_name::<#target_ty>(),
                        reason: ::std::string::ToString::to_string(&e),
                    });
                }
            }
        }
    } else {
        quote! { #call_expr }
    };

    let dep_ty_pairs = dep_types.iter();

    let expanded = quote! {
        #func

        fn #construct_fn_name(
            container: &::quench_http::di::Container,
        ) -> ::quench_http::di::ConstructFuture {
            let container = ::std::clone::Clone::clone(container);
            ::std::boxed::Box::pin(async move {
                #(#dep_gets)*
                let __value = #value_expr;
                ::std::result::Result::Ok(
                    ::std::sync::Arc::new(__value) as ::quench_http::di::BoxedAny
                )
            })
        }

        ::quench_http::inventory::submit! {
            ::quench_http::di::ComponentFactory {
                type_id: || ::std::any::TypeId::of::<#target_ty>(),
                type_name: || ::std::any::type_name::<#target_ty>(),
                dependencies: || ::std::vec![
                    #( (::std::any::TypeId::of::<#dep_ty_pairs>(), ::std::any::type_name::<#dep_ty_pairs>()) ),*
                ],
                construct: #construct_fn_name,
            }
        }
    };

    expanded.into()
}
