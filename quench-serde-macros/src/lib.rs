//! Derive macros re-exported by `quench-serde`. Use them through that crate.
//!
//! The helper attribute is called `serde`, so types annotated for `serde` need
//! no changes: `#[serde(rename_all, deny_unknown_fields, default, tag,
//! untagged)]` on containers, `#[serde(rename, alias, rename_all)]` on
//! variants, and `#[serde(rename, alias, default, skip, skip_serializing,
//! skip_deserializing, skip_serializing_if, flatten)]` on fields.
//!
//! The expansion lives in plain functions so it can be unit-tested.

mod attrs;
mod case;
mod de;
mod model;
mod ser;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use syn::{DeriveInput, GenericParam, Generics, parse2};

/// `impl Serialize for T`: turns the value into a `quench_serde::Value`.
#[proc_macro_derive(Serialize, attributes(serde))]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    expand_serialize(input.into()).into()
}

/// `impl Deserialize for T`: reads the value out of a `quench_serde::Value`.
#[proc_macro_derive(Deserialize, attributes(serde))]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    expand_deserialize(input.into()).into()
}

fn expand_serialize(input: TokenStream2) -> TokenStream2 {
    parse2::<DeriveInput>(input)
        .and_then(|input| ser::expand(&input))
        .unwrap_or_else(|e| e.to_compile_error())
}

fn expand_deserialize(input: TokenStream2) -> TokenStream2 {
    parse2::<DeriveInput>(input)
        .and_then(|input| de::expand(&input))
        .unwrap_or_else(|e| e.to_compile_error())
}

/// The generics with `bound` added to every type parameter.
fn bounded(generics: &Generics, bound: &TokenStream2) -> Generics {
    let mut generics = generics.clone();
    let params: Vec<_> = generics
        .params
        .iter()
        .filter_map(|p| {
            if let GenericParam::Type(t) = p {
                Some(t.ident.clone())
            } else {
                None
            }
        })
        .collect();
    let where_clause = generics.make_where_clause();
    for param in params {
        where_clause
            .predicates
            .push(syn::parse_quote!(#param: #bound));
    }
    generics
}

#[cfg(test)]
mod tests;
