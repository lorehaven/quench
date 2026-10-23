//! `#[async_trait]`: `async fn` in traits that are used as `dyn Trait`.
//!
//! `async fn name(&self, a: &str) -> R` becomes
//!
//! ```ignore
//! fn name<'life0, 'life1, 'async_trait>(&'life0 self, a: &'life1 str)
//!     -> Pin<Box<dyn Future<Output = R> + Send + 'async_trait>>
//! where 'life0: 'async_trait, 'life1: 'async_trait, Self: 'async_trait
//! { Box::pin(async move { ... }) }
//! ```
//!
//! Every elided lifetime in the arguments gets a name, because the returned
//! future borrows all of them and the compiler cannot elide that.

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::visit_mut::{self, VisitMut};
use syn::{
    FnArg, GenericParam, ImplItem, Item, Lifetime, LifetimeParam, ReceiverKind, ReturnType,
    Signature, TraitItem, Type, TypeReference, WherePredicate, parse_quote,
};

/// Expands the attribute; errors become `compile_error!` at the right span.
pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    match try_expand(attr, item) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

fn try_expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let send = parse_send(attr)?;
    match syn::parse2::<Item>(item)? {
        Item::Trait(mut item) => {
            for member in &mut item.items {
                if let TraitItem::Fn(method) = member
                    && method.sig.asyncness.is_some()
                {
                    let has_body = method.default.is_some();
                    rewrite(&mut method.sig, method.default.as_mut(), send, has_body);
                }
            }
            Ok(item.into_token_stream())
        }
        Item::Impl(mut item) => {
            for member in &mut item.items {
                if let ImplItem::Fn(method) = member
                    && method.sig.asyncness.is_some()
                {
                    rewrite(&mut method.sig, Some(&mut method.block), send, false);
                }
            }
            Ok(item.into_token_stream())
        }
        other => Err(syn::Error::new_spanned(
            other,
            "#[async_trait] applies to a trait or an impl block",
        )),
    }
}

/// Empty means `Send` futures; `?Send` lifts that.
fn parse_send(attr: TokenStream) -> syn::Result<bool> {
    if attr.is_empty() {
        return Ok(true);
    }
    let text = attr.to_string().replace(' ', "");
    if text == "?Send" {
        Ok(false)
    } else {
        Err(syn::Error::new_spanned(
            attr,
            "expected `#[async_trait]` or `#[async_trait(?Send)]`",
        ))
    }
}

/// Names every elided lifetime it meets.
struct NameLifetimes {
    fresh: Vec<Lifetime>,
}

impl NameLifetimes {
    fn next(&mut self) -> Lifetime {
        let lifetime = Lifetime::new(&format!("'life{}", self.fresh.len()), Span::call_site());
        self.fresh.push(lifetime.clone());
        lifetime
    }
}

impl VisitMut for NameLifetimes {
    fn visit_type_reference_mut(&mut self, reference: &mut TypeReference) {
        if reference.lifetime.as_ref().is_none_or(|l| l.ident == "_") {
            reference.lifetime = Some(self.next());
        }
        visit_mut::visit_type_reference_mut(self, reference);
    }

    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        if lifetime.ident == "_" {
            *lifetime = self.next();
        }
    }
}

fn rewrite(sig: &mut Signature, body: Option<&mut syn::Block>, send: bool, default_method: bool) {
    sig.asyncness = None;
    let mut names = NameLifetimes { fresh: Vec::new() };
    let mut receiver_kind = None;

    for input in &mut sig.inputs {
        match input {
            FnArg::Receiver(receiver) => match &mut receiver.kind {
                ReceiverKind::Reference(_, lifetime, mutability) => {
                    if lifetime.as_ref().is_none_or(|l| l.ident == "_") {
                        *lifetime = Some(names.next());
                    }
                    receiver_kind = Some(if mutability.is_some() { "mut" } else { "ref" });
                }
                ReceiverKind::Typed(_, ty) => {
                    names.visit_type_mut(ty);
                    receiver_kind = Some("value");
                }
                _ => receiver_kind = Some("value"),
            },
            FnArg::Typed(typed) => names.visit_type_mut(&mut typed.ty),
        }
    }

    let async_trait: Lifetime = parse_quote!('async_trait);
    let mut lifetimes: Vec<GenericParam> = Vec::new();
    // Lifetimes the author already declared must outlive the future too.
    for param in &mut sig.generics.params {
        if let GenericParam::Lifetime(declared) = param {
            declared.bounds.push(async_trait.clone());
        }
    }
    for lifetime in &names.fresh {
        let mut param = LifetimeParam::new(lifetime.clone());
        param.bounds.push(async_trait.clone());
        lifetimes.push(GenericParam::Lifetime(param));
    }
    lifetimes.push(GenericParam::Lifetime(LifetimeParam::new(
        async_trait.clone(),
    )));
    // Lifetimes must precede type and const parameters.
    let (declared_lifetimes, rest): (Vec<_>, Vec<_>) = sig
        .generics
        .params
        .iter()
        .cloned()
        .partition(|p| matches!(p, GenericParam::Lifetime(_)));
    sig.generics.params = declared_lifetimes
        .into_iter()
        .chain(lifetimes)
        .chain(rest)
        .collect();

    let where_clause = sig.generics.make_where_clause();
    where_clause
        .predicates
        .push(parse_quote!(Self: #async_trait));
    // Inline, not in the `where` clause: a bound spelled in both places trips
    // clippy's `multiple_bound_locations` on the user's own code.
    for param in &mut sig.generics.params {
        if let GenericParam::Type(param) = param {
            param.bounds.push(parse_quote!(#async_trait));
        }
    }
    // A default body runs for any implementor, so it must be able to promise
    // what the boxed future needs: `&self` across threads needs `Sync`.
    if send && default_method {
        let bound: Option<WherePredicate> = match receiver_kind {
            Some("ref") => Some(parse_quote!(Self: ::core::marker::Sync)),
            Some("mut" | "value") => Some(parse_quote!(Self: ::core::marker::Send)),
            _ => None,
        };
        if let Some(bound) = bound {
            sig.generics.make_where_clause().predicates.push(bound);
        }
    }

    let output: Type = match &sig.output {
        ReturnType::Default => parse_quote!(()),
        ReturnType::Type(_, ty) => (**ty).clone(),
    };
    let send_bound = if send {
        quote!(+ ::core::marker::Send)
    } else {
        quote!()
    };
    let boxed: Type = parse_quote! {
        ::core::pin::Pin<::std::boxed::Box<dyn ::core::future::Future<Output = #output> #send_bound + #async_trait>>
    };
    sig.output = parse_quote!(-> #boxed);

    if let Some(block) = body {
        let original = std::mem::replace(block, parse_quote!({}));
        *block = parse_quote!({
            let __future: #boxed = ::std::boxed::Box::pin(async move #original);
            __future
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(attr: &str, item: &str) -> String {
        expand(attr.parse().unwrap(), item.parse().unwrap()).to_string()
    }

    fn squash(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn trait_method_without_body_gets_a_boxed_future() {
        let out = squash(&run(
            "",
            "trait T { async fn get(&self, key: &str) -> u8; }",
        ));
        assert!(
            out.contains("fn get < 'life0 : 'async_trait , 'life1 : 'async_trait , 'async_trait >"),
            "{out}"
        );
        assert!(out.contains("& 'life0 self"), "{out}");
        assert!(out.contains("key : & 'life1 str"), "{out}");
        assert!(out.contains("Self : 'async_trait"), "{out}");
        assert!(out.contains("dyn :: core :: future :: Future < Output = u8 > + :: core :: marker :: Send + 'async_trait"), "{out}");
        assert!(!out.contains("async fn"), "{out}");
        // No body, so nothing to box and no Sync bound.
        assert!(!out.contains("Box :: pin"), "{out}");
        assert!(!out.contains("Sync"), "{out}");
    }

    #[test]
    fn default_bodies_are_boxed_and_require_sync_for_ref_self() {
        let out = squash(&run("", "trait T { async fn get(&self) -> u8 { 1 } }"));
        assert!(out.contains("Box :: pin (async move { 1 })"), "{out}");
        assert!(out.contains("Self : :: core :: marker :: Sync"), "{out}");
    }

    #[test]
    fn default_bodies_by_value_or_mut_require_send() {
        for receiver in ["&mut self", "self"] {
            let out = squash(&run(
                "",
                &format!("trait T {{ async fn f({receiver}) {{}} }}"),
            ));
            assert!(
                out.contains("Self : :: core :: marker :: Send"),
                "{receiver}: {out}"
            );
            assert!(!out.contains("Sync"), "{receiver}: {out}");
        }
    }

    #[test]
    fn unit_return_and_no_receiver() {
        let out = squash(&run(
            "",
            "trait T { async fn make() -> Self; async fn nothing(); }",
        ));
        assert!(out.contains("Output = Self"), "{out}");
        assert!(out.contains("Output = ()"), "{out}");
        assert!(out.contains("fn make < 'async_trait >"), "{out}");
    }

    #[test]
    fn impl_methods_are_rewritten() {
        let out = squash(&run(
            "",
            "impl T for S { async fn get(&self, mut n: Vec<&u8>, o: Option<&mut String>) -> Result<u8, E> { n.pop(); Ok(1) } }",
        ));
        assert!(out.contains("& 'life0 self"), "{out}");
        assert!(out.contains("Vec < & 'life1 u8 >"), "{out}");
        assert!(out.contains("Option < & 'life2 mut String >"), "{out}");
        assert!(out.contains("let __future"), "{out}");
        assert!(out.contains("async move { n . pop () ; Ok (1) }"), "{out}");
        // impls never get the Sync bound
        assert!(!out.contains("Sync"), "{out}");
    }

    #[test]
    fn explicit_and_anonymous_lifetimes_are_handled() {
        let out = squash(&run(
            "",
            "trait T { async fn f<'a>(&self, x: &'a str, y: Foo<'_>) -> u8; }",
        ));
        assert!(out.contains("'a : 'async_trait"), "{out}");
        assert!(out.contains("y : Foo < 'life1 >"), "{out}");
    }

    #[test]
    fn type_parameters_must_outlive_the_future() {
        let out = squash(&run("", "trait T { async fn f<X: Clone>(&self, x: X); }"));
        assert!(out.contains("X : Clone + 'async_trait"), "{out}");
        assert!(!out.contains("where X"), "{out}");
    }

    #[test]
    fn question_send_drops_send_requirements() {
        let out = squash(&run("?Send", "trait T { async fn f(&self) -> u8 { 1 } }"));
        assert!(!out.contains("Send"), "{out}");
        assert!(!out.contains("Sync"), "{out}");
        assert!(out.contains("Box :: pin"), "{out}");
        let spaced = squash(&run("? Send", "trait T { async fn f(&self); }"));
        assert!(!spaced.contains("Send"), "{spaced}");
    }

    #[test]
    fn non_async_members_and_other_items_in_a_trait_are_untouched() {
        let out = squash(&run(
            "",
            "trait T { fn plain(&self) -> u8; const N: u8; type A; }",
        ));
        assert!(out.contains("fn plain (& self) -> u8 ;"), "{out}");
        let imp = squash(&run(
            "",
            "impl T for S { fn plain(&self) -> u8 { 1 } const N: u8 = 1; }",
        ));
        assert!(imp.contains("fn plain (& self) -> u8 { 1 }"), "{imp}");
    }

    #[test]
    fn bad_attribute_arguments_are_errors() {
        let out = run("Sync", "trait T {}");
        assert!(out.contains("compile_error"), "{out}");
        assert!(out.contains("expected"), "{out}");
    }

    #[test]
    fn only_traits_and_impls_are_accepted() {
        let out = run("", "fn free() {}");
        assert!(out.contains("compile_error"), "{out}");
        assert!(out.contains("trait or an impl"), "{out}");
    }

    #[test]
    fn unparsable_input_is_an_error() {
        assert!(run("", "this is not rust").contains("compile_error"));
    }
}
