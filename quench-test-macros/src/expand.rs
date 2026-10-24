use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Expr, ItemFn, Lit, Meta, parse2};

/// The options of `#[test(..)]`.
#[derive(Default)]
struct Options {
    multi_thread: bool,
    worker_threads: Option<usize>,
}

fn parse_options(attr: TokenStream) -> syn::Result<Options> {
    let mut options = Options::default();
    if attr.is_empty() {
        return Ok(options);
    }
    let parser = syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated;
    let metas = syn::parse::Parser::parse2(parser, attr)?;
    for meta in metas {
        let Meta::NameValue(pair) = &meta else {
            return Err(syn::Error::new(meta.span(), "expected `name = value`"));
        };
        let name = pair
            .path
            .get_ident()
            .map(ToString::to_string)
            .unwrap_or_default();
        let Expr::Lit(literal) = &pair.value else {
            return Err(syn::Error::new(pair.value.span(), "expected a literal"));
        };
        match (name.as_str(), &literal.lit) {
            ("flavor", Lit::Str(s)) => match s.value().as_str() {
                "multi_thread" => options.multi_thread = true,
                "current_thread" => {}
                other => {
                    return Err(syn::Error::new(
                        s.span(),
                        format!("unknown flavor `{other}`"),
                    ));
                }
            },
            ("worker_threads", Lit::Int(n)) => options.worker_threads = Some(n.base10_parse()?),
            _ => {
                return Err(syn::Error::new(
                    meta.span(),
                    "unsupported option (use `flavor` or `worker_threads`)",
                ));
            }
        }
    }
    Ok(options)
}

pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    match try_expand(attr, item) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

fn try_expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let options = parse_options(attr)?;
    let mut function: ItemFn = parse2(item)?;
    if !function.sig.inputs.is_empty() {
        return Err(syn::Error::new(
            function.sig.inputs.span(),
            "a test takes no arguments",
        ));
    }

    // `ignore` and `should_panic` are read here and left off the function: they only mean something to the
    // standard harness.
    let mut ignore = quote!(::core::option::Option::None);
    let mut should_panic = quote!(::quench_test::ShouldPanic::No);
    let mut kept = Vec::new();
    for attribute in function.attrs.drain(..) {
        if attribute.path().is_ident("ignore") {
            let reason = match &attribute.meta {
                Meta::NameValue(pair) => match &pair.value {
                    Expr::Lit(l) => match &l.lit {
                        Lit::Str(s) => s.value(),
                        _ => String::new(),
                    },
                    _ => String::new(),
                },
                _ => String::new(),
            };
            ignore = quote!(::core::option::Option::Some(#reason));
        } else if attribute.path().is_ident("should_panic") {
            should_panic = if let Meta::List(list) = &attribute.meta {
                let inner: Meta = parse2(list.tokens.clone())?;
                match inner {
                    Meta::NameValue(pair) if pair.path.is_ident("expected") => {
                        let value = pair.value;
                        quote!(::quench_test::ShouldPanic::WithMessage(#value))
                    }
                    other => {
                        return Err(syn::Error::new(
                            other.span(),
                            "expected `expected = \"..\"`",
                        ));
                    }
                }
            } else {
                quote!(::quench_test::ShouldPanic::Yes)
            };
        } else {
            kept.push(attribute);
        }
    }
    function.attrs = kept;

    let name = &function.sig.ident;
    let is_async = function.sig.asyncness.is_some();
    let call = if is_async {
        let threads = options.worker_threads.map_or_else(
            || quote!(::core::option::Option::None),
            |n| quote!(::core::option::Option::Some(#n)),
        );
        let multi = options.multi_thread;
        quote!(::quench_test::__private::run_async(#multi, #threads, || #name()))
    } else {
        quote!(::quench_test::__private::run_sync(|| #name()))
    };
    let span = function.sig.ident.span();
    let registration = quote_spanned! {span=>
        const _: () = {
            fn __quench_test_run() -> ::quench_test::Outcome {
                #call
            }
            ::quench_test::__core::submit!(
                ::quench_test::TESTS,
                ::quench_test::TestCase,
                ::quench_test::TestCase {
                    name: ::core::concat!(::core::module_path!(), "::", ::core::stringify!(#name)),
                    file: ::core::file!(),
                    line: ::core::line!(),
                    ignore: #ignore,
                    should_panic: #should_panic,
                    run: __quench_test_run,
                }
            );
        };
    };
    Ok(quote! {
        #[allow(dead_code)]
        #function
        #registration
    })
}

#[cfg(test)]
mod tests {
    use super::expand;
    use quote::quote;

    fn rendered(attr: proc_macro2::TokenStream, item: proc_macro2::TokenStream) -> String {
        expand(attr, item).to_string()
    }

    #[test]
    fn plain_and_async_tests_register() {
        let sync = rendered(
            quote!(),
            quote!(
                fn works() {}
            ),
        );
        assert!(
            sync.contains("run_sync") && sync.contains("TestCase"),
            "{sync}"
        );
        let asynchronous = rendered(
            quote!(flavor = "multi_thread", worker_threads = 2),
            quote!(
                async fn works() {}
            ),
        );
        assert!(
            asynchronous.contains("run_async")
                && asynchronous.contains("true")
                && asynchronous.contains("Some (2usize)"),
            "{asynchronous}"
        );
        let current = rendered(
            quote!(),
            quote!(
                async fn works() {}
            ),
        );
        assert!(current.contains("run_async (false"), "{current}");
    }

    #[test]
    fn ignore_and_should_panic_are_read() {
        let out = rendered(
            quote!(),
            quote!(
                #[ignore = "slow"]
                #[should_panic(expected = "boom")]
                fn t() {}
            ),
        );
        assert!(
            out.contains("Some (\"slow\")") && out.contains("WithMessage (\"boom\")"),
            "{out}"
        );
        assert!(!out.contains("# [ignore"), "{out}");
        let out = rendered(
            quote!(),
            quote!(
                #[ignore]
                #[should_panic]
                #[doc = "x"]
                fn t() {}
            ),
        );
        assert!(
            out.contains("Some (\"\")")
                && out.contains("ShouldPanic :: Yes")
                && out.contains("doc"),
            "{out}"
        );
    }

    #[test]
    fn mistakes_are_reported_at_compile_time() {
        for (attr, item) in [
            (
                quote!(flavor = "weird"),
                quote!(
                    fn t() {}
                ),
            ),
            (
                quote!(nope = 1),
                quote!(
                    fn t() {}
                ),
            ),
            (
                quote!(flavor),
                quote!(
                    fn t() {}
                ),
            ),
            (
                quote!(flavor = x),
                quote!(
                    fn t() {}
                ),
            ),
            (
                quote!(),
                quote!(
                    fn t(x: u8) {}
                ),
            ),
            (
                quote!(),
                quote!(
                    struct S;
                ),
            ),
            (
                quote!(),
                quote!(
                    #[should_panic(wrong = "x")]
                    fn t() {}
                ),
            ),
        ] {
            let out = rendered(attr, item);
            assert!(out.contains("compile_error"), "{out}");
        }
    }
}
