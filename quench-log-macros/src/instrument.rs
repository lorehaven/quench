//! The expansion of `#[instrument]`.

use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{FnArg, Ident, ItemFn, LitStr, Pat, Token};

#[derive(Default)]
struct Options {
    skip: Vec<Ident>,
    skip_all: bool,
    fields: TokenStream,
    level: Option<LitStr>,
    name: Option<LitStr>,
}

impl Parse for Options {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut options = Self::default();
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "skip_all" => options.skip_all = true,
                "skip" => {
                    let content;
                    syn::parenthesized!(content in input);
                    // `parse_any`: `self` is a keyword, and `skip(self)` is how a method leaves its receiver out.
                    options.skip = Punctuated::<Ident, Token![,]>::parse_terminated_with(&content, Ident::parse_any)?
                        .into_iter()
                        .collect();
                }
                "fields" => {
                    let content;
                    syn::parenthesized!(content in input);
                    options.fields = content.parse()?;
                }
                "level" => {
                    input.parse::<Token![=]>()?;
                    options.level = Some(input.parse()?);
                }
                "name" => {
                    input.parse::<Token![=]>()?;
                    options.name = Some(input.parse()?);
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unsupported #[instrument] option `{other}`; supported: skip, skip_all, fields, level, name"
                        ),
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(options)
    }
}

pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    try_expand(attr, item).unwrap_or_else(|e| e.to_compile_error())
}

fn level_variant(level: Option<&LitStr>) -> syn::Result<Ident> {
    let Some(level) = level else {
        return Ok(format_ident!("Info"));
    };
    match level.value().to_ascii_lowercase().as_str() {
        "trace" => Ok(format_ident!("Trace")),
        "debug" => Ok(format_ident!("Debug")),
        "info" => Ok(format_ident!("Info")),
        "warn" => Ok(format_ident!("Warn")),
        "error" => Ok(format_ident!("Error")),
        _ => Err(syn::Error::new(
            level.span(),
            "level must be one of trace, debug, info, warn, error",
        )),
    }
}

fn try_expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let options: Options = syn::parse2(attr)?;
    let mut function: ItemFn = syn::parse2(item)?;
    let level = level_variant(options.level.as_ref())?;
    let name = options
        .name
        .as_ref()
        .map_or_else(|| function.sig.ident.to_string(), LitStr::value);

    // Arguments are recorded with `Debug`, unless skipped.
    let mut recorded = Vec::new();
    if !options.skip_all {
        for input in &function.sig.inputs {
            let ident = match input {
                FnArg::Receiver(receiver) => Ident::new("self", receiver.self_token.span),
                FnArg::Typed(typed) => match &*typed.pat {
                    Pat::Ident(pat) => pat.ident.clone(),
                    // Patterns have no single name to record under.
                    _ => continue,
                },
            };
            if !options.skip.contains(&ident) {
                recorded.push(ident);
            }
        }
    }
    if let Some(unknown) = options
        .skip
        .iter()
        .find(|skipped| !function.sig.inputs.iter().any(|i| arg_named(i, skipped)))
    {
        return Err(syn::Error::new(
            unknown.span(),
            format!("`{unknown}` is not an argument of this function"),
        ));
    }

    let extra = options.fields;
    let extra = if extra.is_empty() {
        quote!()
    } else {
        quote!(#extra,)
    };
    let argument_fields = recorded.iter().map(|ident| quote!(#ident = ?#ident,));
    let span = quote! {
        ::quench_log::span!(::quench_log::Level::#level, #name, #(#argument_fields)* #extra)
    };

    let block = &function.block;
    let body = if function.sig.asyncness.is_some() {
        quote!({
            let __quench_span = #span;
            ::quench_log::Instrument::instrument(async move #block, __quench_span).await
        })
    } else {
        quote!({
            let __quench_span = #span;
            let __quench_entered = __quench_span.enter();
            #block
        })
    };
    *function.block = syn::parse2(body)?;
    Ok(quote!(#function))
}

fn arg_named(input: &FnArg, name: &Ident) -> bool {
    match input {
        FnArg::Receiver(_) => name == "self",
        // A name bound inside a pattern (`Inject(config): Inject<Config>`) can be skipped too, even
        // though such an argument is not recorded under a name of its own.
        FnArg::Typed(typed) => binds(typed.pat.to_token_stream(), name),
    }
}

fn binds(tokens: TokenStream, name: &Ident) -> bool {
    tokens.into_iter().any(|tree| match tree {
        proc_macro2::TokenTree::Ident(ident) => ident == *name,
        proc_macro2::TokenTree::Group(group) => binds(group.stream(), name),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(attr: &str, item: &str) -> String {
        expand(attr.parse().unwrap(), item.parse().unwrap())
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn sync_functions_enter_the_span_for_their_body() {
        let out = run("", "fn add(a: u32, b: u32) -> u32 { a + b }");
        assert!(
            out.contains("span ! (:: quench_log :: Level :: Info , \"add\" , a = ? a , b = ? b ,)"),
            "{out}"
        );
        assert!(
            out.contains("let __quench_entered = __quench_span . enter ()"),
            "{out}"
        );
        assert!(out.contains("{ a + b }"), "{out}");
        assert!(!out.contains("async"), "{out}");
    }

    #[test]
    fn async_functions_are_instrumented_as_futures() {
        let out = run("", "async fn fetch(id: u64) -> u64 { id }");
        assert!(out.contains("async fn fetch"), "{out}");
        assert!(out.contains(":: quench_log :: Instrument :: instrument (async move { id } , __quench_span) . await"), "{out}");
        assert!(!out.contains("enter ()"), "{out}");
    }

    #[test]
    fn skip_all_and_skip_leave_arguments_out() {
        let skip = run(
            "skip(db, body)",
            "async fn up(db: &Db, name: String, body: Vec<u8>) {}",
        );
        assert!(skip.contains("name = ? name"), "{skip}");
        assert!(
            !skip.contains("db = ? db") && !skip.contains("body = ? body"),
            "{skip}"
        );
        let all = run("skip_all", "fn f(a: u8, b: u8) {}");
        assert!(!all.contains("= ?"), "{all}");
    }

    #[test]
    fn methods_record_self() {
        let out = run("", "fn m(&self, n: u8) {}");
        assert!(out.contains("self = ? self , n = ? n"), "{out}");
        let skipped = run("skip(self)", "fn m(&self, n: u8) {}");
        assert!(!skipped.contains("self = ?"), "{skipped}");
    }

    #[test]
    fn options_set_the_level_name_and_extra_fields() {
        let out = run(
            r#"level = "debug", name = "uploading", fields(size = body.len(), who = %user)"#,
            "fn f(body: Vec<u8>, user: String) {}",
        );
        assert!(out.contains("Level :: Debug , \"uploading\""), "{out}");
        assert!(
            out.contains("size = body . len () , who = % user ,"),
            "{out}"
        );
        for (level, variant) in [
            ("trace", "Trace"),
            ("info", "Info"),
            ("warn", "Warn"),
            ("ERROR", "Error"),
        ] {
            assert!(
                run(&format!("level = \"{level}\""), "fn f() {}")
                    .contains(&format!("Level :: {variant}"))
            );
        }
    }

    #[test]
    fn patterns_without_a_name_are_not_recorded() {
        let out = run("", "fn f((a, b): (u8, u8), c: u8) {}");
        assert!(out.contains("c = ? c") && !out.contains("a = ? a"), "{out}");
    }

    #[test]
    fn refusals() {
        assert!(run("bogus", "fn f() {}").contains("unsupported #[instrument] option `bogus`"));
        assert!(run(r#"level = "loud""#, "fn f() {}").contains("level must be one of"));
        assert!(run("skip(nope)", "fn f(a: u8) {}").contains("`nope` is not an argument"));
        assert!(run("", "struct S;").contains("compile_error"));
        assert!(run("skip(a) extra", "fn f(a: u8) {}").contains("compile_error"));
    }

    #[test]
    fn skip_accepts_the_receiver() {
        let out = run("skip(self, b)", "fn f(&self, a: u8, b: u8) {}");
        assert!(out.contains("a = ? a"), "{out}");
        assert!(!out.contains("self = ? self") && !out.contains("b = ? b"), "{out}");
    }

    #[test]
    fn skip_accepts_names_bound_inside_a_pattern() {
        let out = run(
            "skip(claims, db)",
            "async fn f(Wrapper(claims): Wrapper<Claims>, Inject(db): Inject<Db>, id: u8) {}",
        );
        assert!(out.contains("id = ? id"), "{out}");
        let err = expand(
            "skip(nope)".parse().unwrap(),
            "fn f(Wrapper(claims): Wrapper<Claims>) {}".parse().unwrap(),
        );
        assert!(err.to_string().contains("not an argument"), "{err}");
    }
}
