//! The expansion of `#[get]` and friends and of `#[injectable]`, as plain
//! `proc_macro2` functions so they can be unit tested.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{FnArg, GenericArgument, ItemFn, LitStr, PathArguments, ReturnType, Type};

pub fn route(method: &str, attr: TokenStream2, item: TokenStream2) -> TokenStream2 {
    let pattern = match syn::parse2::<LitStr>(attr) {
        Ok(pattern) => pattern,
        Err(err) => return err.to_compile_error(),
    };
    let func = match syn::parse2::<ItemFn>(item) {
        Ok(func) => func,
        Err(err) => return err.to_compile_error(),
    };
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
                    .to_compile_error();
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

        #[::quench_http::async_trait]
        impl ::quench_http::endpoint::Endpoint for #struct_name {
            async fn call(&self, mut req: ::quench_http::request::Request) -> ::quench_http::response::Response {
                #(#arg_bindings)*
                let __result = #call_expr;
                ::quench_http::endpoint::IntoResponse::into_response(__result)
            }
        }

        ::quench_http::submit!(
            ::quench_http::route::ROUTES,
            ::quench_http::route::RouteRegistration,
            ::quench_http::route::RouteRegistration {
                method: ::quench_http::http::Method::#method_ident,
                pattern: #pattern,
                endpoint: || ::std::sync::Arc::new(#struct_name) as ::std::sync::Arc<dyn ::quench_http::endpoint::Endpoint>,
            }
        );
    };

    expanded
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

pub fn injectable(item: TokenStream2) -> TokenStream2 {
    let func = match syn::parse2::<ItemFn>(item) {
        Ok(func) => func,
        Err(err) => return err.to_compile_error(),
    };
    let fn_name = &func.sig.ident;
    let construct_fn_name = format_ident!("__quench_construct_{}", fn_name);

    let ReturnType::Type(_, ret_ty) = &func.sig.output else {
        return syn::Error::new_spanned(
            &func.sig,
            "#[injectable] functions must return a type (optionally `Result<T, E>`)",
        )
        .to_compile_error();
    };
    let (target_ty, is_result) = result_inner(ret_ty);

    let mut dep_types: Vec<Type> = Vec::new();
    let mut arg_idents: Vec<syn::Ident> = Vec::new();

    for (i, input) in func.sig.inputs.iter().enumerate() {
        let ty = match input {
            FnArg::Typed(pat_type) => &pat_type.ty,
            FnArg::Receiver(_) => {
                return syn::Error::new_spanned(input, "#[injectable] functions can't take `self`")
                    .to_compile_error();
            }
        };
        let Some(dep_ty) = arc_inner(ty) else {
            return syn::Error::new_spanned(
                ty,
                "#[injectable] constructor parameters must be `Arc<Dependency>` - that's the only \
                 dependency shape the container resolves",
            )
            .to_compile_error();
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

        ::quench_http::submit!(
            ::quench_http::di::COMPONENTS,
            ::quench_http::di::ComponentFactory,
            ::quench_http::di::ComponentFactory {
                type_id: || ::std::any::TypeId::of::<#target_ty>(),
                type_name: || ::std::any::type_name::<#target_ty>(),
                dependencies: || ::std::vec![
                    #( (::std::any::TypeId::of::<#dep_ty_pairs>(), ::std::any::type_name::<#dep_ty_pairs>()) ),*
                ],
                construct: #construct_fn_name,
            }
        );
    };

    expanded
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn text(tokens: &TokenStream2) -> String {
        tokens
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn a_route_keeps_the_function_and_registers_it() {
        let out = text(&route(
            "GET",
            quote! { "/users/{id}" },
            quote! { async fn show(id: Path<u64>, db: Inject<Db>) -> String { id.to_string() } },
        ));
        assert!(
            out.contains("async fn show"),
            "the original function is kept: {out}"
        );
        assert!(out.contains("struct __quench_route_show"), "{out}");
        assert!(
            out.contains(":: quench_http :: submit !") && out.contains("ROUTES"),
            "{out}"
        );
        assert!(
            out.contains("Method :: GET") && out.contains("\"/users/{id}\""),
            "{out}"
        );
        assert!(
            out.contains("__arg0") && out.contains("__arg1"),
            "one extraction per parameter: {out}"
        );
        assert!(
            out.contains("show (__arg0 , __arg1) . await"),
            "an async handler is awaited: {out}"
        );
    }

    #[test]
    fn a_plain_function_is_called_without_await() {
        let out = text(&route(
            "POST",
            quote! { "/x" },
            quote! { fn plain() -> &'static str { "x" } },
        ));
        assert!(
            out.contains("plain () ;") || out.contains("plain ()"),
            "{out}"
        );
        assert!(!out.contains("plain () . await"), "{out}");
        assert!(out.contains("Method :: POST"));
        for method in ["PUT", "DELETE", "PATCH"] {
            assert!(
                text(&route(method, quote! { "/x" }, quote! { async fn h() {} }))
                    .contains(&format!("Method :: {method}"))
            );
        }
    }

    #[test]
    fn bad_route_input_becomes_a_compile_error() {
        let no_pattern = text(&route("GET", quote! {}, quote! { async fn h() {} }));
        assert!(no_pattern.contains("compile_error"), "{no_pattern}");
        let not_a_function = text(&route("GET", quote! { "/x" }, quote! { struct S; }));
        assert!(not_a_function.contains("compile_error"), "{not_a_function}");
        let with_self = text(&route(
            "GET",
            quote! { "/x" },
            quote! { async fn h(&self) {} },
        ));
        assert!(
            with_self.contains("compile_error") && with_self.contains("can't take"),
            "{with_self}"
        );
    }

    #[test]
    fn an_injectable_registers_its_dependencies() {
        let out = text(&injectable(
            quote! { async fn build(db: Arc<Db>, cache: std::sync::Arc<Cache>) -> Service { Service::new(db, cache) } },
        ));
        assert!(out.contains("fn __quench_construct_build"), "{out}");
        assert!(
            out.contains("container . get :: < Db >")
                && out.contains("container . get :: < Cache >"),
            "{out}"
        );
        assert!(
            out.contains("COMPONENTS") && out.contains("ComponentFactory"),
            "{out}"
        );
        assert!(out.contains("TypeId :: of :: < Service >"), "{out}");
        assert!(out.contains("build (__dep0 , __dep1) . await"), "{out}");
    }

    #[test]
    fn a_fallible_constructor_reports_construction_failures() {
        let out = text(&injectable(
            quote! { fn build() -> Result<Service, String> { Ok(Service) } },
        ));
        assert!(out.contains("ConstructionFailed"), "{out}");
        assert!(
            out.contains("TypeId :: of :: < Service >"),
            "the registered type is the Ok type: {out}"
        );
        assert!(!out.contains("build () . await"), "{out}");
    }

    #[test]
    fn bad_injectables_are_rejected_with_a_reason() {
        for (input, needle) in [
            (quote! { fn build() {} }, "must return a type"),
            (
                quote! { fn build(&self) -> Service { Service } },
                "can't take",
            ),
            (
                quote! { fn build(db: Db) -> Service { Service } },
                "must be `Arc<Dependency>`",
            ),
            (
                quote! { fn build(db: &Db) -> Service { Service } },
                "must be `Arc<Dependency>`",
            ),
            (
                quote! { fn build(db: Arc) -> Service { Service } },
                "must be `Arc<Dependency>`",
            ),
            (
                quote! { fn build(db: Arc<'static>) -> Service { Service } },
                "must be `Arc<Dependency>`",
            ),
        ] {
            let out = text(&injectable(input));
            assert!(
                out.contains("compile_error") && out.contains(needle),
                "{needle}: {out}"
            );
        }
        assert!(text(&injectable(quote! { struct S; })).contains("compile_error"));
    }

    #[test]
    fn the_type_helpers_see_through_arc_and_result() {
        let ty: Type = syn::parse_quote!(std::sync::Arc<Vec<u8>>);
        assert!(arc_inner(&ty).is_some());
        assert!(arc_inner(&syn::parse_quote!(Rc<u8>)).is_none());
        assert!(arc_inner(&syn::parse_quote!(&Arc<u8>)).is_none());
        let (inner, is_result) = result_inner(&syn::parse_quote!(Result<Foo, Bar>));
        assert!(is_result && quote!(#inner).to_string() == "Foo");
        let (same, is_result) = result_inner(&syn::parse_quote!(Option<Foo>));
        assert!(!is_result && quote!(#same).to_string() == "Option < Foo >");
        assert!(!result_inner(&syn::parse_quote!(Result)).1);
        assert!(!result_inner(&syn::parse_quote!((u8, u8))).1);
    }
}
