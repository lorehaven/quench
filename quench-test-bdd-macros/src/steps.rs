use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Expr, FnArg, ItemFn, Lit, Meta, Pat, Type, parse2};

struct Pattern {
    text: String,
    regex: bool,
}

fn parse_pattern(attr: &TokenStream) -> syn::Result<Pattern> {
    // `"text"`, `expr = "text"` or `regex = "pattern"`.
    if let Ok(Lit::Str(text)) = parse2::<Lit>(attr.clone()) {
        return Ok(Pattern {
            text: text.value(),
            regex: false,
        });
    }
    let meta: Meta = parse2(attr.clone()).map_err(|_| {
        syn::Error::new(
            attr.span(),
            "expected a string, `expr = \"..\"` or `regex = \"..\"`",
        )
    })?;
    let Meta::NameValue(pair) = meta else {
        return Err(syn::Error::new(
            attr.span(),
            "expected `expr = \"..\"` or `regex = \"..\"`",
        ));
    };
    let Expr::Lit(literal) = &pair.value else {
        return Err(syn::Error::new(
            pair.value.span(),
            "expected a string literal",
        ));
    };
    let Lit::Str(text) = &literal.lit else {
        return Err(syn::Error::new(literal.span(), "expected a string literal"));
    };
    let regex = if pair.path.is_ident("regex") {
        true
    } else if pair.path.is_ident("expr") {
        false
    } else {
        return Err(syn::Error::new(
            pair.path.span(),
            "expected `expr` or `regex`",
        ));
    };
    Ok(Pattern {
        text: text.value(),
        regex,
    })
}

/// Whether a parameter type is `&Step` (any path ending in `Step`).
fn is_step_reference(ty: &Type) -> bool {
    let Type::Reference(reference) = ty else {
        return false;
    };
    let Type::Path(path) = &*reference.elem else {
        return false;
    };
    reference.mutability.is_none() && path.path.segments.last().is_some_and(|s| s.ident == "Step")
}

pub fn expand(kind: &str, attr: TokenStream, item: TokenStream) -> TokenStream {
    match try_expand(kind, attr, item) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

// Token streams are passed by value, like the macro entry points that call this.
#[allow(clippy::needless_pass_by_value)]
fn try_expand(kind: &str, attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let pattern = parse_pattern(&attr)?;
    let function: ItemFn = parse2(item)?;
    let name = &function.sig.ident;

    let mut inputs = function.sig.inputs.iter();
    let Some(FnArg::Typed(first)) = inputs.next() else {
        return Err(syn::Error::new(
            function.sig.span(),
            "a step takes `&mut World` first",
        ));
    };
    let Type::Reference(world_reference) = &*first.ty else {
        return Err(syn::Error::new(
            first.ty.span(),
            "the first parameter must be `&mut World`",
        ));
    };
    if world_reference.mutability.is_none() {
        return Err(syn::Error::new(
            first.ty.span(),
            "the first parameter must be `&mut World`",
        ));
    }
    let world = &world_reference.elem;

    // The remaining parameters: the step itself, or the next capture parsed to the declared type.
    let mut arguments = Vec::new();
    let mut captures = 0usize;
    for input in inputs {
        let FnArg::Typed(typed) = input else {
            return Err(syn::Error::new(input.span(), "unexpected receiver"));
        };
        if is_step_reference(&typed.ty) {
            arguments.push(quote!(__context.step));
        } else {
            let index = captures;
            captures += 1;
            let ty = &typed.ty;
            let label = match &*typed.pat {
                Pat::Ident(ident) => ident.ident.to_string(),
                _ => format!("argument {}", index + 1),
            };
            arguments.push(quote! {
                match ::quench_test_bdd::step::parse_argument::<#ty>(__context, #index, #label) {
                    ::core::result::Result::Ok(value) => value,
                    ::core::result::Result::Err(message) => return ::core::result::Result::Err(message),
                }
            });
        }
    }

    let call = if function.sig.asyncness.is_some() {
        quote!(#name(__world, #(#arguments),*).await)
    } else {
        quote!(#name(__world, #(#arguments),*))
    };
    let kind_ident = format_ident!("{kind}");
    let text = &pattern.text;
    let regex = pattern.regex;
    let span = function.sig.ident.span();
    let registration = quote_spanned! {span=>
        const _: () = {
            #[allow(clippy::all, clippy::pedantic, clippy::nursery)]
            fn __quench_step<'a>(
                __world: &'a mut (dyn ::core::any::Any + ::core::marker::Send),
                __context: &'a ::quench_test_bdd::step::Context<'a>,
            ) -> ::quench_test_bdd::BoxFuture<'a, ::core::result::Result<(), ::std::string::String>> {
                ::std::boxed::Box::pin(async move {
                    let __world = match __world.downcast_mut::<#world>() {
                        ::core::option::Option::Some(world) => world,
                        ::core::option::Option::None => {
                            return ::core::result::Result::Err("step run against the wrong World type".to_string());
                        }
                    };
                    ::quench_test_bdd::step::StepOutcome::into_result(#call)
                })
            }
            ::quench_test_bdd::__core::submit!(
                ::quench_test_bdd::STEPS,
                ::quench_test_bdd::step::Definition,
                ::quench_test_bdd::step::Definition {
                    kind: ::quench_test_bdd::step::Kind::#kind_ident,
                    pattern: #text,
                    regex: #regex,
                    file: ::core::file!(),
                    line: ::core::line!(),
                    world: || ::core::any::TypeId::of::<#world>(),
                    run: __quench_step,
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

    fn rendered(
        kind: &str,
        attr: proc_macro2::TokenStream,
        item: proc_macro2::TokenStream,
    ) -> String {
        expand(kind, attr, item).to_string()
    }

    #[test]
    fn expressions_regexes_and_arguments() {
        let out = rendered(
            "Given",
            quote!("a user {string}"),
            quote!(
                async fn f(w: &mut W, name: String, n: u8, s: &Step) {}
            ),
        );
        assert!(
            out.contains("Kind :: Given")
                && out.contains("\"a user {string}\"")
                && out.contains("regex : false"),
            "{out}"
        );
        assert!(
            out.contains("parse_argument :: < String >")
                && out.contains("parse_argument :: < u8 >")
                && out.contains("__context . step"),
            "{out}"
        );
        assert!(out.contains(". await"), "{out}");
        let out = rendered(
            "Then",
            quote!(regex = r"^x$"),
            quote!(
                fn f(w: &mut W) -> Result<(), String> {
                    Ok(())
                }
            ),
        );
        assert!(
            out.contains("Kind :: Then")
                && out.contains("regex : true")
                && !out.contains(". await"),
            "{out}"
        );
        let out = rendered(
            "When",
            quote!(expr = "go"),
            quote!(
                fn f(w: &mut W, q: (i32, i32)) {}
            ),
        );
        assert!(
            out.contains("regex : false") && out.contains("parse_argument :: < (i32 , i32) >"),
            "{out}"
        );
        let out = rendered(
            "When",
            quote!("go"),
            quote!(
                fn f(w: &mut W, _: u8, (a, b): (u8, u8)) {}
            ),
        );
        assert!(
            out.contains("\"argument 1\"") && out.contains("\"argument 2\""),
            "{out}"
        );
    }

    #[test]
    fn mistakes_are_compile_errors() {
        for (attr, item) in [
            (
                quote!(),
                quote!(
                    fn f(w: &mut W) {}
                ),
            ),
            (
                quote!(nope = "x"),
                quote!(
                    fn f(w: &mut W) {}
                ),
            ),
            (
                quote!(regex = 5),
                quote!(
                    fn f(w: &mut W) {}
                ),
            ),
            (
                quote!(regex),
                quote!(
                    fn f(w: &mut W) {}
                ),
            ),
            (
                quote!("x"),
                quote!(
                    fn f() {}
                ),
            ),
            (
                quote!("x"),
                quote!(
                    fn f(w: &W) {}
                ),
            ),
            (
                quote!("x"),
                quote!(
                    fn f(w: W) {}
                ),
            ),
            (
                quote!("x"),
                quote!(
                    struct S;
                ),
            ),
        ] {
            assert!(rendered("Given", attr, item).contains("compile_error"));
        }
    }
}
