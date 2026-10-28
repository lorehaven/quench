//! The expansion of `#[plugin]`, as plain `proc_macro2` functions so it can be unit tested.

use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{
    FnArg, GenericArgument, Ident, ItemTrait, LitStr, Pat, PathArguments, ReturnType, TraitItem,
    TraitItemFn, Type,
};

fn error(tokens: &impl Spanned, message: &str) -> TokenStream {
    syn::Error::new(tokens.span(), message).to_compile_error()
}

/// The plugin name: a lowercase identifier, as it is written in the Kotlin class.
fn plugin_name(attr: TokenStream) -> syn::Result<String> {
    let lit: LitStr = syn::parse2(attr)
        .map_err(|e| syn::Error::new(e.span(), "give the plugin's name: `#[plugin(\"echo\")]`"))?;
    let name = lit.value();
    let ok = name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if ok {
        Ok(name)
    } else {
        Err(syn::Error::new(
            lit.span(),
            "a plugin name is lowercase letters, digits and `_`, starting with a letter",
        ))
    }
}

/// `T` out of `Result<T, BridgeError>`.
fn ok_type(ret: &ReturnType) -> Result<Type, TokenStream> {
    let bad = || error(ret, "a plugin method returns `Result<T, BridgeError>`");
    let ReturnType::Type(_, ty) = ret else {
        return Err(bad());
    };
    let Type::Path(path) = &**ty else {
        return Err(bad());
    };
    let Some(last) = path.path.segments.last().filter(|s| s.ident == "Result") else {
        return Err(bad());
    };
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return Err(bad());
    };
    let types: Vec<&Type> = args
        .args
        .iter()
        .filter_map(|a| match a {
            GenericArgument::Type(t) => Some(t),
            _ => None,
        })
        .collect();
    match types.as_slice() {
        [ok, Type::Path(err)]
            if err
                .path
                .segments
                .last()
                .is_some_and(|s| s.ident == "BridgeError") =>
        {
            Ok((*ok).clone())
        }
        _ => Err(bad()),
    }
}

fn method(plugin: &str, f: &TraitItemFn, vis: &syn::Visibility) -> TokenStream {
    let sig = &f.sig;
    if sig.asyncness.is_none() {
        return error(sig, "a plugin method is an `async fn`");
    }
    if f.default.is_some() {
        return error(f, "a plugin method has no body: the host runs it");
    }
    if !sig.generics.params.is_empty() {
        return error(&sig.generics, "a plugin method cannot be generic");
    }
    let ok = match ok_type(&sig.output) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let mut params = Vec::new();
    let mut names = Vec::new();
    for input in &sig.inputs {
        let FnArg::Typed(arg) = input else {
            return error(
                input,
                "a plugin method has no `self`: call it as `Plugin::method(..)`",
            );
        };
        let Pat::Ident(ident) = &*arg.pat else {
            return error(&arg.pat, "name each argument: it travels by name");
        };
        let (name, ty) = (&ident.ident, &arg.ty);
        params.push(quote! { #name: #ty });
        names.push(name.clone());
    }
    let keys: Vec<String> = names.iter().map(Ident::to_string).collect();
    let ident = &sig.ident;
    let wire = ident.to_string();
    let attrs = &f.attrs;
    let args = if names.is_empty() {
        quote! { ::quench_ui_web::__private::json!({}) }
    } else {
        quote! { ::quench_ui_web::__private::json!({ #( #keys : #names ),* }) }
    };
    quote! {
        #(#attrs)*
        #[allow(clippy::future_not_send)] // the browser runs one thread
        #vis async fn #ident( #(#params),* ) -> ::std::result::Result<#ok, ::quench_ui_web::BridgeError> {
            ::quench_ui_web::bridge::call(#plugin, #wire, &#args).await
        }
    }
}

pub fn plugin(attr: TokenStream, item: TokenStream) -> TokenStream {
    let name = match plugin_name(attr) {
        Ok(n) => n,
        Err(e) => return e.to_compile_error(),
    };
    let tr = match syn::parse2::<ItemTrait>(item) {
        Ok(t) => t,
        Err(e) => return e.to_compile_error(),
    };
    let mut methods = Vec::new();
    for item in &tr.items {
        match item {
            TraitItem::Fn(f) => methods.push(method(&name, f, &tr.vis)),
            other => return error(other, "a plugin trait has only `async fn` declarations"),
        }
    }
    let (attrs, vis, ident) = (&tr.attrs, &tr.vis, &tr.ident);
    quote! {
        #(#attrs)*
        #[derive(Clone, Copy, Debug)]
        #vis struct #ident;

        impl #ident {
            /// The plugin's name on the host side.
            #vis const NAME: &'static str = #name;

            #(#methods)*
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand(attr: &str, item: &str) -> String {
        plugin(attr.parse().unwrap(), item.parse().unwrap()).to_string()
    }

    #[test]
    fn a_trait_becomes_a_struct_of_calls_by_name() {
        let out = expand(
            "\"echo\"",
            "pub trait Echo { async fn say(text: String, times: u32) -> Result<String, BridgeError>; }",
        );
        assert!(out.contains("pub struct Echo ;"), "{out}");
        assert!(
            out.contains("const NAME : & 'static str = \"echo\""),
            "{out}"
        );
        assert!(
            out.contains("pub async fn say (text : String , times : u32)"),
            "{out}"
        );
        assert!(
            out.contains("Result < String , :: quench_ui_web :: BridgeError >"),
            "{out}"
        );
        assert!(out.contains("bridge :: call (\"echo\" , \"say\""), "{out}");
        assert!(out.contains("\"text\" : text , \"times\" : times"), "{out}");
    }

    #[test]
    fn a_method_without_arguments_sends_an_empty_object() {
        let out = expand(
            "\"echo\"",
            "trait Echo { async fn device() -> Result<u32, BridgeError>; }",
        );
        assert!(out.contains("json ! ({ })"), "{out}");
        assert!(out.contains("async fn device ()"), "{out}");
    }

    #[test]
    fn docs_and_visibility_are_kept() {
        let out = expand(
            "\"echo\"",
            "/// An echo.\npub trait Echo {\n/// Says.\nasync fn say() -> Result<(), BridgeError>; }",
        );
        assert!(out.contains("An echo."), "{out}");
        assert!(out.contains("Says."), "{out}");
    }

    #[test]
    fn mistakes_are_reported_where_they_are() {
        let attr = "\"echo\"";
        for (item, want) in [
            (
                "trait E { fn say() -> Result<(), BridgeError>; }",
                "async fn",
            ),
            (
                "trait E { async fn say() -> String; }",
                "Result<T, BridgeError>",
            ),
            (
                "trait E { async fn say() -> Result<String, OtherError>; }",
                "Result<T, BridgeError>",
            ),
            (
                "trait E { async fn say(&self) -> Result<(), BridgeError>; }",
                "no `self`",
            ),
            (
                "trait E { async fn say((a, b): (u8, u8)) -> Result<(), BridgeError>; }",
                "name each argument",
            ),
            (
                "trait E { async fn say() -> Result<(), BridgeError> { Ok(()) } }",
                "no body",
            ),
            (
                "trait E { async fn say<T>() -> Result<(), BridgeError>; }",
                "generic",
            ),
            ("trait E { const X: u8; }", "only `async fn`"),
        ] {
            let out = expand(attr, item);
            assert!(
                out.contains("compile_error") && out.contains(want),
                "{item}: {out}"
            );
        }
        for attr in ["", "echo", "\"Echo\"", "\"1echo\"", "\"my-echo\""] {
            let out = expand(attr, "trait E {}");
            assert!(out.contains("compile_error"), "{attr}: {out}");
        }
        assert!(expand("\"echo\"", "struct S;").contains("compile_error"));
    }
}
