//! The parsed shape of the type being derived, shared by both derives.

use proc_macro2::Span;
use syn::{Data, DeriveInput, Fields, Generics, Ident, Index, Member};

use crate::attrs;
use crate::case::RenameRule;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Named,
    Tuple,
    Unit,
}

pub struct Field {
    pub member: Member,
    /// The name this field gets in a `match` arm: `__f0`, or the field's own name.
    pub binding: Ident,
    /// The key in serialised data, after `rename` / `rename_all`.
    pub key: String,
    pub attrs: attrs::Field,
}

impl Field {
    /// The key plus its aliases, for lookups.
    pub fn names(&self) -> Vec<String> {
        std::iter::once(self.key.clone())
            .chain(self.attrs.aliases.iter().cloned())
            .collect()
    }
}

pub struct Variant<'a> {
    pub ident: &'a Ident,
    pub name: String,
    pub aliases: Vec<String>,
    pub style: Style,
    pub fields: Vec<Field>,
}

impl Variant<'_> {
    pub fn names(&self) -> Vec<String> {
        std::iter::once(self.name.clone())
            .chain(self.aliases.iter().cloned())
            .collect()
    }
}

pub enum Body<'a> {
    Struct(Style, Vec<Field>),
    Enum(Vec<Variant<'a>>),
}

pub struct Input<'a> {
    pub ident: &'a Ident,
    pub generics: &'a Generics,
    pub container: attrs::Container,
    pub body: Body<'a>,
}

pub fn parse(input: &DeriveInput) -> syn::Result<Input<'_>> {
    let container = attrs::container(&input.attrs)?;
    let body = match &input.data {
        Data::Struct(data) => {
            let (style, fields) = fields(&data.fields, container.rename_all)?;
            Body::Struct(style, fields)
        }
        Data::Enum(data) => {
            let mut variants = Vec::new();
            for variant in &data.variants {
                let attrs = attrs::variant(&variant.attrs)?;
                let name = attrs.rename.clone().unwrap_or_else(|| {
                    let ident = unraw(&variant.ident);
                    container
                        .rename_all
                        .map_or_else(|| ident.clone(), |rule| rule.apply_to_variant(&ident))
                });
                let (style, fields) = fields(&variant.fields, attrs.rename_all)?;
                variants.push(Variant {
                    ident: &variant.ident,
                    name,
                    aliases: attrs.aliases,
                    style,
                    fields,
                });
            }
            Body::Enum(variants)
        }
        Data::Union(_) => {
            return Err(syn::Error::new(
                Span::call_site(),
                "unions are not supported",
            ));
        }
    };
    if let (Some(tag), Body::Enum(variants)) = (&container.tag, &body)
        && let Some(tuple) = variants
            .iter()
            .find(|v| v.style == Style::Tuple && v.fields.len() != 1)
    {
        return Err(syn::Error::new(
            tuple.ident.span(),
            format!(
                "tuple variant `{}` cannot be internally tagged (tag = \"{tag}\"); use a struct or newtype variant",
                tuple.ident
            ),
        ));
    }
    Ok(Input {
        ident: &input.ident,
        generics: &input.generics,
        container,
        body,
    })
}

fn fields(fields: &Fields, rename_all: Option<RenameRule>) -> syn::Result<(Style, Vec<Field>)> {
    let (style, list): (Style, Vec<&syn::Field>) = match fields {
        Fields::Named(f) => (Style::Named, f.named.iter().collect()),
        Fields::Unnamed(f) => (Style::Tuple, f.unnamed.iter().collect()),
        Fields::Unit => (Style::Unit, Vec::new()),
    };
    let mut out = Vec::new();
    for (i, field) in list.into_iter().enumerate() {
        let attrs = attrs::field(&field.attrs)?;
        if attrs.flatten && style != Style::Named {
            return Err(syn::Error::new_spanned(
                field,
                "`flatten` needs a named field",
            ));
        }
        let (member, binding, key) = match &field.ident {
            Some(ident) => {
                let plain = unraw(ident);
                let key = attrs.rename.clone().unwrap_or_else(|| {
                    rename_all.map_or_else(|| plain.clone(), |rule| rule.apply_to_field(&plain))
                });
                (Member::Named(ident.clone()), ident.clone(), key)
            }
            None => (
                Member::Unnamed(Index::from(i)),
                quote::format_ident!("__f{i}"),
                i.to_string(),
            ),
        };
        out.push(Field {
            member,
            binding,
            key,
            attrs,
        });
    }
    Ok((style, out))
}

/// `r#type` -> `type`.
fn unraw(ident: &Ident) -> String {
    let text = ident.to_string();
    text.strip_prefix("r#")
        .map_or_else(|| text.clone(), str::to_string)
}
