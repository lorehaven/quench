//! `#[derive(Serialize)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use crate::model::{self, Body, Field, Input, Style, Variant};

pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let model = model::parse(input)?;
    let body = match &model.body {
        Body::Struct(style, fields) => struct_body(*style, fields),
        Body::Enum(variants) => enum_body(&model, variants),
    };
    let name = model.ident;
    let generics = crate::bounded(model.generics, &quote!(::quench_serde::Serialize));
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::quench_serde::Serialize for #name #ty_generics #where_clause {
            #[allow(clippy::too_many_lines)]
            fn serialize(&self) -> ::quench_serde::Value {
                #body
            }
        }
    })
}

/// Statements adding each field to `__map`. `access` gives an expression of
/// type `&Field` for a field.
fn insertions(fields: &[Field], access: &dyn Fn(&Field) -> TokenStream) -> TokenStream {
    let steps = fields.iter().filter(|f| !f.attrs.skip_serializing).map(|f| {
        let value = access(f);
        let key = &f.key;
        if f.attrs.flatten {
            quote!(::quench_serde::__private::flatten_into(&mut __map, ::quench_serde::Serialize::serialize(#value));)
        } else if let Some(skip_if) = &f.attrs.skip_serializing_if {
            quote! {
                if !#skip_if(#value) {
                    __map.insert(#key, ::quench_serde::Serialize::serialize(#value));
                }
            }
        } else {
            quote!(__map.insert(#key, ::quench_serde::Serialize::serialize(#value));)
        }
    });
    quote!(#(#steps)*)
}

fn array(fields: &[Field], access: &dyn Fn(&Field) -> TokenStream) -> TokenStream {
    let items = fields.iter().map(|f| {
        let value = access(f);
        quote!(::quench_serde::Serialize::serialize(#value))
    });
    quote!(::quench_serde::Value::Array(::std::vec![#(#items),*]))
}

/// Serialised form of a struct's or variant's fields on their own.
fn content(style: Style, fields: &[Field], access: &dyn Fn(&Field) -> TokenStream) -> TokenStream {
    match (style, fields) {
        (Style::Unit, _) => quote!(::quench_serde::Value::Null),
        (Style::Tuple, [only]) => {
            let value = access(only);
            quote!(::quench_serde::Serialize::serialize(#value))
        }
        (Style::Tuple, _) => array(fields, access),
        (Style::Named, _) => {
            let steps = insertions(fields, access);
            quote! {{
                let mut __map = ::quench_serde::Map::new();
                #steps
                ::quench_serde::Value::Object(__map)
            }}
        }
    }
}

fn struct_body(style: Style, fields: &[Field]) -> TokenStream {
    content(style, fields, &|f| {
        let member = &f.member;
        quote!(&self.#member)
    })
}

fn enum_body(model: &Input<'_>, variants: &[Variant<'_>]) -> TokenStream {
    if variants.is_empty() {
        return quote!(match *self {});
    }
    let arms = variants.iter().map(|v| {
        let ident = v.ident;
        let pattern = pattern(v);
        let access = |f: &Field| {
            let binding = &f.binding;
            quote!(#binding)
        };
        let name = &v.name;
        let value = match &model.container.tag {
            Some(tag) => tagged(tag, name, v, &access),
            None if model.container.untagged => content(v.style, &v.fields, &access),
            None => external(name, v, &access),
        };
        quote!(Self::#ident #pattern => #value,)
    });
    quote!(match self { #(#arms)* })
}

fn pattern(variant: &Variant<'_>) -> TokenStream {
    let bindings = variant.fields.iter().map(|f| &f.binding);
    match variant.style {
        Style::Unit => quote!(),
        Style::Tuple => quote!((#(#bindings),*)),
        Style::Named => quote!({ #(#bindings),* }),
    }
}

/// `"Name"` for unit variants, `{"Name": content}` otherwise.
fn external(
    name: &str,
    variant: &Variant<'_>,
    access: &dyn Fn(&Field) -> TokenStream,
) -> TokenStream {
    if variant.style == Style::Unit {
        return quote!(::quench_serde::Value::String(::std::string::String::from(#name)));
    }
    let inner = content(variant.style, &variant.fields, access);
    quote! {{
        let mut __map = ::quench_serde::Map::new();
        __map.insert(#name, #inner);
        ::quench_serde::Value::Object(__map)
    }}
}

/// `{"tag": "Name", ...fields}`.
fn tagged(
    tag: &str,
    name: &str,
    variant: &Variant<'_>,
    access: &dyn Fn(&Field) -> TokenStream,
) -> TokenStream {
    let body = match (variant.style, variant.fields.as_slice()) {
        (Style::Named, fields) => insertions(fields, access),
        (Style::Tuple, [only]) => {
            let value = access(only);
            quote!(::quench_serde::__private::flatten_into(&mut __map, ::quench_serde::Serialize::serialize(#value));)
        }
        _ => quote!(),
    };
    quote! {{
        let mut __map = ::quench_serde::Map::new();
        __map.insert(#tag, ::quench_serde::Value::String(::std::string::String::from(#name)));
        #body
        ::quench_serde::Value::Object(__map)
    }}
}
