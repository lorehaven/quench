//! `#[derive(Deserialize)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use crate::attrs::{Container, DefaultKind};
use crate::model::{self, Body, Field, Input, Style, Variant};

pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let model = model::parse(input)?;
    let body = match &model.body {
        Body::Struct(style, fields) => struct_body(&model, *style, fields),
        Body::Enum(variants) => enum_body(&model, variants),
    };
    let name = model.ident;
    let generics = crate::bounded(model.generics, &quote!(::quench_serde::Deserialize));
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::quench_serde::Deserialize for #name #ty_generics #where_clause {
            #[allow(clippy::too_many_lines, clippy::redundant_closure_call)]
            fn deserialize(__value: &::quench_serde::Value) -> ::quench_serde::Result<Self> {
                #body
            }
        }
    })
}

fn name_list(names: &[String]) -> TokenStream {
    quote!(&[#(#names),*])
}

/// A struct's or variant's fields read from a JSON object. `object` is an
/// expression of type `&Map`; `extra_known` are keys that belong to something
/// else (the enum tag) and are not "unknown".
fn named(
    ctor: &TokenStream,
    fields: &[Field],
    container: &Container,
    object: &TokenStream,
    extra_known: &[String],
) -> TokenStream {
    let known: Vec<String> = fields
        .iter()
        .filter(|f| !f.attrs.flatten && !f.attrs.skip_deserializing)
        .flat_map(Field::names)
        .chain(extra_known.iter().cloned())
        .collect();
    let has_flatten = fields.iter().any(|f| f.attrs.flatten);
    let known = name_list(&known);

    // With flatten, "unknown" has no meaning: the flattened fields take the rest.
    let check = (container.deny_unknown_fields && !has_flatten)
        .then(|| quote!(::quench_serde::__private::check_unknown(__object, #known)?;));
    let rest = has_flatten
        .then(|| quote!(let __rest = ::quench_serde::__private::rest(__object, #known);));
    let type_default = container.default.as_ref().map(|d| match d {
        DefaultKind::Trait => quote!(let __default: Self = ::core::default::Default::default();),
        DefaultKind::Path(path) => quote!(let __default: Self = #path();),
    });

    let assignments = fields.iter().map(|f| {
        let member = &f.member;
        let names = name_list(&f.names());
        let from_type_default = || quote!(__default.#member);
        let value = if f.attrs.skip_deserializing {
            if container.default.is_some() { from_type_default() } else { quote!(::core::default::Default::default()) }
        } else if f.attrs.flatten {
            quote!(::quench_serde::__private::flatten(&__rest)?)
        } else {
            match (&f.attrs.default, &container.default) {
                (Some(DefaultKind::Trait), _) => quote!(::quench_serde::__private::field_or(__object, #names, ::core::default::Default::default)?),
                (Some(DefaultKind::Path(path)), _) => quote!(::quench_serde::__private::field_or(__object, #names, #path)?),
                (None, Some(_)) => {
                    let fallback = from_type_default();
                    quote!(::quench_serde::__private::field_or(__object, #names, || #fallback)?)
                }
                (None, None) => quote!(::quench_serde::__private::field(__object, #names)?),
            }
        };
        quote!(#member: #value)
    });

    quote! {{
        let __object: &::quench_serde::Map = #object;
        #check
        #rest
        #type_default
        ::core::result::Result::Ok(#ctor { #(#assignments),* })
    }}
}

/// A tuple struct/variant read from a JSON array (or the bare inner value for a newtype).
fn tuple(ctor: &TokenStream, fields: &[Field], value: &TokenStream) -> TokenStream {
    if let [_] = fields {
        return quote!(::core::result::Result::Ok(#ctor(::quench_serde::Deserialize::deserialize(#value)?)));
    }
    let len = fields.len();
    let elements = (0..len).map(|i| quote!(::quench_serde::__private::element(__items, #i)?));
    quote! {{
        let __items = ::quench_serde::__private::expect_seq(#value, #len)?;
        ::core::result::Result::Ok(#ctor(#(#elements),*))
    }}
}

fn unit(ctor: &TokenStream, value: &TokenStream) -> TokenStream {
    quote! {{
        ::quench_serde::__private::expect_null(#value)?;
        ::core::result::Result::Ok(#ctor)
    }}
}

fn struct_body(model: &Input<'_>, style: Style, fields: &[Field]) -> TokenStream {
    let ctor = quote!(Self);
    let value = quote!(__value);
    match style {
        Style::Named => {
            let object = quote!(::quench_serde::__private::expect_object(__value)?);
            named(&ctor, fields, &model.container, &object, &[])
        }
        Style::Tuple => tuple(&ctor, fields, &value),
        Style::Unit => unit(&ctor, &value),
    }
}

fn variant_ctor(variant: &Variant<'_>) -> TokenStream {
    let ident = variant.ident;
    quote!(Self::#ident)
}

fn enum_body(model: &Input<'_>, variants: &[Variant<'_>]) -> TokenStream {
    let enum_name = model.ident.to_string();
    let all_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
    let expected = name_list(&all_names);
    if variants.is_empty() {
        return quote!(::core::result::Result::Err(::quench_serde::Error::new(
            format!("cannot deserialize the uninhabited enum `{}`", #enum_name)
        )));
    }
    if model.container.untagged {
        return untagged(model, variants, &enum_name);
    }
    if let Some(tag) = &model.container.tag {
        return internally_tagged(model, variants, tag, &expected);
    }
    externally_tagged(model, variants, &expected)
}

/// The expression reading one variant's content from `value` (a `&Value`).
fn variant_content(
    model: &Input<'_>,
    variant: &Variant<'_>,
    value: &TokenStream,
    extra_known: &[String],
) -> TokenStream {
    let ctor = variant_ctor(variant);
    match variant.style {
        Style::Unit => unit(&ctor, value),
        Style::Tuple => tuple(&ctor, &variant.fields, value),
        Style::Named => {
            let object = quote!(::quench_serde::__private::expect_object(#value)?);
            named(
                &ctor,
                &variant.fields,
                &model.container,
                &object,
                extra_known,
            )
        }
    }
}

/// Wraps a variant body so its errors say which variant they came from.
fn in_variant(name: &str, body: &TokenStream) -> TokenStream {
    quote!((|| -> ::quench_serde::Result<Self> #body)().map_err(|__e| __e.in_field(#name)))
}

fn externally_tagged(
    model: &Input<'_>,
    variants: &[Variant<'_>],
    expected: &TokenStream,
) -> TokenStream {
    let unit_arms = variants.iter().filter(|v| v.style == Style::Unit).map(|v| {
        let names = v.names();
        let ctor = variant_ctor(v);
        quote!(#(#names)|* => ::core::result::Result::Ok(#ctor),)
    });
    let inner = quote!(__inner);
    let keyed_arms = variants.iter().map(|v| {
        let names = v.names();
        let body = variant_content(model, v, &inner, &[]);
        let wrapped = in_variant(&v.name, &quote!({ #body }));
        quote!(#(#names)|* => #wrapped,)
    });
    quote! {
        match __value {
            ::quench_serde::Value::String(__name) => match __name.as_str() {
                #(#unit_arms)*
                _ => ::quench_serde::__private::unknown_variant(__name, #expected),
            },
            ::quench_serde::Value::Object(__map) => {
                let (__key, __inner) = ::quench_serde::__private::single_entry(__map)?;
                match __key {
                    #(#keyed_arms)*
                    _ => ::quench_serde::__private::unknown_variant(__key, #expected),
                }
            }
            __other => ::core::result::Result::Err(::quench_serde::invalid_type("a string or a single-key object", __other)),
        }
    }
}

fn internally_tagged(
    model: &Input<'_>,
    variants: &[Variant<'_>],
    tag: &str,
    expected: &TokenStream,
) -> TokenStream {
    let object = quote!(__object);
    let tag_names = [tag.to_string()];
    let arms = variants.iter().map(|v| {
        let names = v.names();
        let ctor = variant_ctor(v);
        let body = match (v.style, v.fields.as_slice()) {
            (Style::Named, fields) => named(&ctor, fields, &model.container, &object, &tag_names),
            (Style::Tuple, [_]) => quote! {{
                let __content = ::quench_serde::Value::Object(::quench_serde::__private::rest(__object, &[#tag]));
                ::core::result::Result::Ok(#ctor(::quench_serde::Deserialize::deserialize(&__content)?))
            }},
            _ => quote!(::core::result::Result::Ok(#ctor)),
        };
        let wrapped = in_variant(&v.name, &quote!({ #body }));
        quote!(#(#names)|* => #wrapped,)
    });
    quote! {{
        let __object = ::quench_serde::__private::expect_object(__value)?;
        match ::quench_serde::__private::tag_of(__object, #tag)? {
            #(#arms)*
            __other => ::quench_serde::__private::unknown_variant(__other, #expected),
        }
    }}
}

fn untagged(model: &Input<'_>, variants: &[Variant<'_>], enum_name: &str) -> TokenStream {
    let value = quote!(__value);
    let attempts = variants.iter().map(|v| {
        let body = variant_content(model, v, &value, &[]);
        quote! {
            if let ::core::result::Result::Ok(__ok) = (|| -> ::quench_serde::Result<Self> { #body })() {
                return ::core::result::Result::Ok(__ok);
            }
        }
    });
    quote! {{
        #(#attempts)*
        ::core::result::Result::Err(::quench_serde::Error::new(format!(
            "data did not match any variant of untagged enum `{}`",
            #enum_name
        )))
    }}
}
