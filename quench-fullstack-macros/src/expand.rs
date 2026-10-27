//! The expansion of `#[server]`, as plain `proc_macro2` functions so it can be unit tested.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{
    FnArg, GenericArgument, Ident, ItemFn, LitStr, Pat, PathArguments, ReturnType, Type,
    parse::Parser,
};

struct Param {
    ident: Ident,
    ty: Type,
}

fn error(tokens: &impl Spanned, message: &str) -> TokenStream {
    syn::Error::new(tokens.span(), message).to_compile_error()
}

/// What `#[server(...)]` can say.
#[derive(Default)]
struct Options {
    path: Option<String>,
    auth: Auth,
}

/// Whether the function needs a signed-in user.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Auth {
    /// The call is public; the user is not looked up.
    #[default]
    None,
    /// A valid session is required, or the call is answered 401 before the function runs.
    Required,
    /// The user is looked up and available if there is one.
    Optional,
}

fn options(attr: TokenStream) -> syn::Result<Options> {
    let mut options = Options::default();
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("path") {
            let lit: LitStr = meta.value()?.parse()?;
            let value = lit.value();
            if !value.starts_with('/') || value.contains(char::is_whitespace) {
                return Err(syn::Error::new(
                    lit.span(),
                    "the path must start with `/` and have no spaces",
                ));
            }
            options.path = Some(value);
            Ok(())
        } else if meta.path.is_ident("auth") {
            options.auth = if meta.input.peek(syn::Token![=]) {
                let lit: LitStr = meta.value()?.parse()?;
                match lit.value().as_str() {
                    "required" => Auth::Required,
                    "optional" => Auth::Optional,
                    _ => {
                        return Err(syn::Error::new(
                            lit.span(),
                            "`auth` is `\"required\"` or `\"optional\"`",
                        ));
                    }
                }
            } else {
                Auth::Required
            };
            Ok(())
        } else {
            Err(meta.error(
                "unknown option; they are `path = \"/...\"` and `auth` or `auth = \"optional\"`",
            ))
        }
    });
    parser.parse2(attr)?;
    Ok(options)
}

/// `T` out of `Result<T, E>`.
fn ok_type(ret: &ReturnType) -> Option<Type> {
    let ReturnType::Type(_, ty) = ret else {
        return None;
    };
    let Type::Path(path) = &**ty else {
        return None;
    };
    let last = path.path.segments.last()?;
    if last.ident != "Result" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    args.args.iter().find_map(|a| match a {
        GenericArgument::Type(t) => Some(t.clone()),
        _ => None,
    })
}

/// The user, found before the arguments are read, so a call without a session never gets that far.
fn find_user(auth: Auth) -> TokenStream {
    match auth {
        Auth::None => quote! {},
        Auth::Required => quote! {
            let __user = match <::quench_fullstack::User as ::quench_fullstack::__private::FromRequest>::from_request(&mut req).await {
                ::std::result::Result::Ok(user) => ::std::option::Option::Some(user),
                ::std::result::Result::Err(err) => {
                    return ::quench_fullstack::ServerError::from_http(&err).into_response();
                }
            };
        },
        Auth::Optional => quote! {
            let __user = <::quench_fullstack::User as ::quench_fullstack::__private::FromRequest>::from_request(&mut req).await.ok();
        },
    }
}

pub fn server(attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = match syn::parse2::<ItemFn>(item) {
        Ok(func) => func,
        Err(err) => return err.to_compile_error(),
    };
    let options = match options(attr) {
        Ok(options) => options,
        Err(err) => return err.to_compile_error(),
    };
    if func.sig.asyncness.is_none() {
        return error(&func.sig, "a server function must be `async`");
    }
    if !func.sig.generics.params.is_empty() || func.sig.generics.where_clause.is_some() {
        return error(&func.sig.generics, "a server function cannot be generic");
    }
    let Some(ok_ty) = ok_type(&func.sig.output) else {
        return error(
            &func.sig.output,
            "a server function must return `Result<T, ServerError>`",
        );
    };

    // Every parameter is an argument of the call: the browser and the server have one signature.
    let mut params = Vec::new();
    for input in &func.sig.inputs {
        let FnArg::Typed(typed) = input else {
            return error(input, "a server function cannot take `self`");
        };
        let Pat::Ident(pat) = &*typed.pat else {
            return error(
                &typed.pat,
                "parameters must be plain names, such as `title: String`",
            );
        };
        params.push(Param {
            ident: pat.ident.clone(),
            ty: (*typed.ty).clone(),
        });
    }

    let name = &func.sig.ident;
    let vis = &func.vis;
    let attrs = &func.attrs;
    let ret = &func.sig.output;
    let path = options.path.unwrap_or_else(|| format!("/_fn/{name}"));
    let args_ident = format_ident!("__QuenchArgs");
    let idents: Vec<&Ident> = params.iter().map(|p| &p.ident).collect();
    let types: Vec<&Type> = params.iter().map(|p| &p.ty).collect();
    let signature = params.iter().map(|p| {
        let (ident, ty) = (&p.ident, &p.ty);
        quote! { #ident: #ty }
    });

    let find_user = find_user(options.auth);
    let run = if options.auth == Auth::None {
        quote! { #name(#(__args.#idents),*).await }
    } else {
        quote! { ::quench_fullstack::server::with_user(__user, #name(#(__args.#idents),*)).await }
    };

    quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #[allow(clippy::unused_async)] // a server function is async by contract
        #func

        #[cfg(not(target_arch = "wasm32"))]
        const _: () = {
            #[derive(::quench_serde::Deserialize)]
            #[allow(non_camel_case_types, dead_code)]
            struct #args_ident { #(#idents: #types,)* }

            struct __QuenchEndpoint;

            #[::quench_fullstack::__private::async_trait]
            impl ::quench_fullstack::__private::Endpoint for __QuenchEndpoint {
                async fn call(
                    &self,
                    mut req: ::quench_fullstack::__private::Request,
                ) -> ::quench_fullstack::__private::Response {
                    if let ::std::option::Option::Some(refusal) = ::quench_fullstack::server::guard(&req) {
                        return refusal;
                    }
                    #find_user
                    ::quench_fullstack::server::dispatch(req, move |__args: #args_ident| async move {
                        #run
                    })
                    .await
                }
            }

            ::quench_fullstack::__private::submit!(
                ::quench_fullstack::server::FUNCTIONS,
                ::quench_fullstack::server::ServerFn,
                ::quench_fullstack::server::ServerFn {
                    path: #path,
                    endpoint: || ::std::sync::Arc::new(__QuenchEndpoint)
                        as ::std::sync::Arc<dyn ::quench_fullstack::__private::Endpoint>,
                }
            );
        };

        #(#attrs)*
        #[cfg(target_arch = "wasm32")]
        #[allow(clippy::future_not_send)] // the browser runs one thread
        #vis async fn #name(#(#signature),*) #ret {
            #[derive(::quench_serde::Serialize)]
            #[allow(non_camel_case_types)]
            struct #args_ident { #(#idents: #types,)* }
            ::quench_fullstack::client::call::<_, #ok_ty>(#path, &#args_ident { #(#idents),* }).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::server;
    use proc_macro2::TokenStream;
    use quote::quote;

    fn expand(attr: TokenStream, item: TokenStream) -> String {
        server(attr, item).to_string()
    }

    fn ok(attr: TokenStream) -> String {
        expand(
            attr,
            quote! {
                pub async fn add_note(text: String, done: bool) -> Result<Note, ServerError> {
                    Ok(todo!())
                }
            },
        )
    }

    #[test]
    fn the_server_gets_the_function_a_route_and_a_registration() {
        let out = ok(quote!());
        assert!(out.contains("cfg (not (target_arch = \"wasm32\"))"));
        assert!(out.contains("pub async fn add_note (text : String , done : bool)"));
        assert!(out.contains("\"/_fn/add_note\""), "{out}");
        assert!(out.contains("server :: guard"));
        assert!(out.contains("server :: dispatch"));
        assert!(out.contains("server :: FUNCTIONS"));
        assert!(
            !out.contains("with_user") && !out.contains("FromRequest"),
            "a public call looks nobody up: {out}"
        );
    }

    #[test]
    fn the_browser_gets_a_stub_with_the_same_signature() {
        let out = ok(quote!());
        assert!(out.contains("cfg (target_arch = \"wasm32\")"));
        assert!(
            out.contains(
                "async fn add_note (text : String , done : bool) -> Result < Note , ServerError >"
            ),
            "{out}"
        );
        assert!(out.contains("client :: call :: < _ , Note >"));
        assert!(out.contains("derive (:: quench_serde :: Serialize)"));
        assert!(out.contains("derive (:: quench_serde :: Deserialize)"));
    }

    #[test]
    fn auth_looks_the_user_up_before_the_arguments_are_read() {
        let required = ok(quote!(auth));
        assert!(required.contains("FromRequest"), "{required}");
        assert!(required.contains("with_user"));
        assert!(
            required.contains("return :: quench_fullstack :: ServerError :: from_http"),
            "a missing session is answered before the function runs"
        );
        let optional = ok(quote!(auth = "optional"));
        assert!(
            optional.contains(". ok ()") && !optional.contains("from_http"),
            "{optional}"
        );
        assert!(ok(quote!(auth = "required")).contains("from_http"));
    }

    #[test]
    fn the_path_can_be_chosen() {
        let out = expand(
            quote!(path = "/api/notes/add"),
            quote! { async fn a() -> Result<(), ServerError> { Ok(()) } },
        );
        assert!(
            out.contains("\"/api/notes/add\"") && !out.contains("/_fn/a"),
            "{out}"
        );
    }

    fn error_of(attr: TokenStream, item: TokenStream) -> String {
        let out = expand(attr, item);
        assert!(
            out.contains("compile_error"),
            "expected an error, got {out}"
        );
        out
    }

    #[test]
    fn mistakes_are_reported_where_they_are() {
        let ret = quote! { -> Result<(), ServerError> };
        assert!(
            error_of(quote!(), quote! { fn sync() #ret { Ok(()) } }).contains("must be `async`")
        );
        assert!(
            error_of(quote!(), quote! { async fn g<T>(x: T) #ret { Ok(()) } })
                .contains("cannot be generic")
        );
        assert!(
            error_of(quote!(), quote! { async fn m(&self) #ret { Ok(()) } })
                .contains("cannot take `self`")
        );
        assert!(
            error_of(
                quote!(),
                quote! { async fn p((a, b): (u8, u8)) #ret { Ok(()) } }
            )
            .contains("plain names")
        );
        assert!(
            error_of(quote!(), quote! { async fn r() -> u32 { 1 } })
                .contains("must return `Result<T, ServerError>`")
        );
        assert!(error_of(quote!(), quote! { async fn r() { } }).contains("must return"));
        assert!(
            error_of(
                quote!(path = "no-slash"),
                quote! { async fn a() #ret { Ok(()) } }
            )
            .contains("must start with `/`")
        );
        assert!(
            error_of(
                quote!(path = "/a b"),
                quote! { async fn a() #ret { Ok(()) } }
            )
            .contains("no spaces")
        );
        assert!(
            error_of(
                quote!(speed = "fast"),
                quote! { async fn a() #ret { Ok(()) } }
            )
            .contains("unknown option")
        );
        assert!(
            error_of(
                quote!(auth = "sometimes"),
                quote! { async fn a() #ret { Ok(()) } }
            )
            .contains("`auth` is")
        );
        assert!(error_of(quote!(), quote! { struct NotAFunction; }).contains("compile_error"));
    }

    #[test]
    fn a_function_with_no_arguments_has_an_empty_argument_object() {
        let out = expand(
            quote!(auth),
            quote! { async fn me() -> Result<User, ServerError> { user() } },
        );
        assert!(
            out.contains("async fn me () -> Result < User , ServerError >"),
            "{out}"
        );
        assert!(
            out.contains("struct __QuenchArgs { }") || out.contains("struct __QuenchArgs {}"),
            "{out}"
        );
    }
}
