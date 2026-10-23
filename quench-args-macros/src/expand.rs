//! The code the derives write.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    Data, DataEnum, DeriveInput, Field, Fields, GenericArgument, Ident, PathArguments, Type,
    Variant,
};

use crate::attrs::{self, ArgAttrs, CommandAttrs, Doc, Given, LongAbout, Version};

const A: &str = "::quench_args";

fn krate() -> TokenStream {
    A.parse().expect("a valid path")
}

/// `Command` statements for the container-level attributes and doc comment, applied to `cmd`.
fn command_settings(
    attrs: &CommandAttrs,
    doc: &Doc,
    version_allowed: bool,
) -> syn::Result<TokenStream> {
    let mut out = TokenStream::new();
    if let Some(about) = &attrs.about {
        out.extend(quote!(let cmd = cmd.about(#about);));
    } else if let Some(short) = &doc.short {
        out.extend(quote!(let cmd = cmd.about(#short);));
    }
    match &attrs.long_about {
        LongAbout::Text(long) => out.extend(quote!(let cmd = cmd.long_about(#long);)),
        LongAbout::Suppressed => {}
        LongAbout::Unset => {
            if let Some(long) = &doc.long {
                out.extend(quote!(let cmd = cmd.long_about(#long);));
            }
        }
    }
    if let Some(version) = &attrs.version {
        if !version_allowed {
            return Err(syn::Error::new(
                Span::call_site(),
                "`version` only applies to the top-level command",
            ));
        }
        match version {
            Version::Cargo => {
                out.extend(quote!(let cmd = cmd.version(::std::env!("CARGO_PKG_VERSION"));));
            }
            Version::Expr(e) => out.extend(quote!(let cmd = cmd.version(#e);)),
        }
    }
    if let Some(t) = &attrs.help_template {
        out.extend(quote!(let cmd = cmd.help_template(#t);));
    }
    if attrs.disable_help_subcommand {
        out.extend(quote!(let cmd = cmd.disable_help_subcommand(true);));
    }
    for alias in &attrs.aliases {
        out.extend(quote!(let cmd = cmd.alias(#alias);));
    }
    for alias in &attrs.visible_aliases {
        out.extend(quote!(let cmd = cmd.visible_alias(#alias);));
    }
    if attrs.hide {
        out.extend(quote!(let cmd = cmd.hide(true);));
    }
    Ok(out)
}

/// How a field's type reads: `bool`, `Option<T>`, `Vec<T>` or a plain `T`.
enum Shape {
    Flag,
    Optional(Type),
    Many(Type),
    Plain(Type),
}

fn last_segment_arg(ty: &Type, name: &str) -> Option<Type> {
    let Type::Path(p) = ty else {
        return None;
    };
    let seg = p.path.segments.last()?;
    if seg.ident != name {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    args.args.iter().find_map(|a| match a {
        GenericArgument::Type(t) => Some(t.clone()),
        _ => None,
    })
}

fn shape(ty: &Type) -> Shape {
    if let Type::Path(p) = ty
        && p.path.is_ident("bool")
    {
        return Shape::Flag;
    }
    if let Some(inner) = last_segment_arg(ty, "Option") {
        return Shape::Optional(inner);
    }
    if let Some(inner) = last_segment_arg(ty, "Vec") {
        return Shape::Many(inner);
    }
    Shape::Plain(ty.clone())
}

/// What a field adds to the command (`cmd` statements) and how it is read back from `m`.
struct Piece {
    augment: TokenStream,
    build: TokenStream,
}

fn field_piece(field: &Field) -> syn::Result<Piece> {
    let a = krate();
    let ident = field
        .ident
        .clone()
        .ok_or_else(|| syn::Error::new_spanned(field, "fields must be named"))?;
    let cmd_attrs = attrs::command(&field.attrs)?;
    let ty = &field.ty;

    if cmd_attrs.subcommand {
        let (inner, optional) = match shape(ty) {
            Shape::Optional(inner) => (inner, true),
            _ => (ty.clone(), false),
        };
        let required = if optional {
            TokenStream::new()
        } else {
            quote!(let cmd = cmd.subcommand_required(true);)
        };
        let build = if optional {
            quote!(if m.subcommand().is_some() {
                ::std::option::Option::Some(<#inner as #a::FromMatches>::from_matches(m)?)
            } else {
                ::std::option::Option::None
            })
        } else {
            quote!(<#inner as #a::FromMatches>::from_matches(m)?)
        };
        return Ok(Piece {
            augment: quote!(let cmd = <#inner as #a::Subcommand>::augment_subcommands(cmd); #required),
            build,
        });
    }
    if cmd_attrs.flatten {
        return Ok(Piece {
            augment: quote!(let cmd = <#ty as #a::Args>::augment_args(cmd);),
            build: quote!(<#ty as #a::FromMatches>::from_matches(m)?),
        });
    }

    let attrs = attrs::arg(&field.attrs)?;
    let doc = attrs::doc(&field.attrs);
    arg_piece(&ident, ty, &attrs, &doc)
}

// One statement per attribute, in the order the attributes are documented: splitting it would only scatter
// that mapping.
#[allow(clippy::too_many_lines)]
fn arg_piece(ident: &Ident, ty: &Type, attrs: &ArgAttrs, doc: &Doc) -> syn::Result<Piece> {
    let a = krate();
    let raw = ident.to_string();
    let id = raw.trim_start_matches("r#").to_string();
    let shape = shape(ty);
    let positional = matches!(attrs.long, Given::Unset) && matches!(attrs.short, Given::Unset);

    let mut build_arg = quote!(let arg = #a::Arg::new(#id););
    let long_name = match &attrs.long {
        Given::Unset => None,
        Given::Bare => Some(attrs::kebab(&id)),
        Given::Value(name) => Some(name.clone()),
    };
    if let Some(long) = &long_name {
        build_arg.extend(quote!(let arg = arg.long(#long);));
    }
    let short_char = match &attrs.short {
        Given::Unset => None,
        Given::Bare => id.chars().next(),
        Given::Value(c) => Some(*c),
    };
    if let Some(short) = short_char {
        build_arg.extend(quote!(let arg = arg.short(#short);));
    }
    if let Some(name) = &attrs.value_name {
        build_arg.extend(quote!(let arg = arg.value_name(#name);));
    }
    if let Some(help) = &attrs.help {
        build_arg.extend(quote!(let arg = arg.help(#help);));
    } else if let Some(short) = &doc.short {
        build_arg.extend(quote!(let arg = arg.help(#short);));
    }
    if let Some(long_help) = &attrs.long_help {
        build_arg.extend(quote!(let arg = arg.long_help(#long_help);));
    } else if attrs.help.is_none()
        && let Some(long) = &doc.long
    {
        build_arg.extend(quote!(let arg = arg.long_help(#long);));
    }

    let (kind, inner) = match &shape {
        Shape::Flag => (quote!(#a::Kind::Flag), None),
        Shape::Optional(t) | Shape::Plain(t) => (quote!(#a::Kind::Single), Some(t.clone())),
        Shape::Many(t) => (quote!(#a::Kind::Many), Some(t.clone())),
    };
    build_arg.extend(quote!(let arg = arg.kind(#kind);));
    if matches!(shape, Shape::Flag) && positional {
        return Err(syn::Error::new(
            ident.span(),
            "a `bool` field is a switch: give it `long` or `short`",
        ));
    }

    let has_default = attrs.default_value.is_some() || attrs.default_value_t.is_some();
    let required = attrs
        .required
        .unwrap_or(matches!(shape, Shape::Plain(_)) && !has_default);
    if required {
        build_arg.extend(quote!(let arg = arg.required(true);));
    }
    if attrs.global {
        build_arg.extend(quote!(let arg = arg.global(true);));
    }
    if let Some(default) = &attrs.default_value {
        build_arg.extend(quote!(let arg = arg.default_value(#default);));
    }
    if let Some(default) = &attrs.default_value_t {
        if attrs.value_enum {
            build_arg.extend(quote!(
                let arg = arg.default_value(
                    #a::ValueEnum::to_possible_value(&(#default))
                        .map(|p| p.get_name().to_string())
                        .unwrap_or_default()
                );
            ));
        } else {
            build_arg.extend(quote!(let arg = arg.default_value(::std::string::ToString::to_string(&(#default)));));
        }
    }
    for other in &attrs.conflicts_with {
        build_arg.extend(quote!(let arg = arg.conflicts_with(#other);));
    }
    if attrs.optional_value {
        let Some(missing) = &attrs.default_missing_value else {
            return Err(syn::Error::new(
                ident.span(),
                "`num_args = 0..=1` needs a `default_missing_value` (what the option means when given no value)",
            ));
        };
        build_arg.extend(quote!(let arg = arg.optional_value(#missing);));
    }
    if attrs.require_equals {
        build_arg.extend(quote!(let arg = arg.require_equals(true);));
    }
    if attrs.trailing_var_arg {
        build_arg.extend(quote!(let arg = arg.trailing_var_arg(true);));
    }
    if attrs.allow_hyphen_values {
        build_arg.extend(quote!(let arg = arg.allow_hyphen_values(true);));
    }
    if attrs.hide {
        build_arg.extend(quote!(let arg = arg.hide(true);));
    }
    if attrs.value_enum
        && let Some(inner) = &inner
    {
        build_arg.extend(quote!(let arg = arg.possible_values(#a::possible_values::<#inner>());));
    }

    // The name used in error messages: `--long <NAME>` or `<NAME>`.
    let placeholder = attrs
        .value_name
        .as_ref()
        .and_then(|e| match e {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) => Some(s.value()),
            _ => None,
        })
        .unwrap_or_else(|| id.to_uppercase().replace('-', "_"));
    let shown = match (&long_name, short_char) {
        (Some(long), _) => format!("--{long} <{placeholder}>"),
        (None, Some(short)) => format!("-{short} <{placeholder}>"),
        (None, None) => format!("<{placeholder}>"),
    };
    // The `Result` of reading one value; callers add the `?`.
    let parse = |raw: TokenStream, inner: &Type| -> TokenStream {
        if attrs.value_enum {
            quote!(#a::value_enum_arg::<#inner>(#shown, #raw))
        } else {
            quote!(<#inner as #a::FromArg>::from_arg(#shown, #raw))
        }
    };

    let build = match &shape {
        Shape::Flag => quote!(m.flag(#id)),
        Shape::Optional(inner) => {
            let parsed = parse(quote!(raw), inner);
            quote!(match m.get_one(#id) {
                ::std::option::Option::Some(raw) => ::std::option::Option::Some(#parsed?),
                ::std::option::Option::None => ::std::option::Option::None,
            })
        }
        Shape::Many(inner) => {
            let parsed = parse(quote!(raw), inner);
            quote!(m
                .get_many(#id)
                .iter()
                .map(|raw| #parsed)
                .collect::<::std::result::Result<::std::vec::Vec<_>, #a::Error>>()?)
        }
        Shape::Plain(inner) => {
            let parsed = parse(quote!(m.get_required(#id)?), inner);
            quote!(#parsed?)
        }
    };
    Ok(Piece {
        augment: quote!(let cmd = { #build_arg cmd.arg(arg) };),
        build,
    })
}

fn named_fields(fields: &Fields) -> syn::Result<Vec<(Ident, Piece)>> {
    match fields {
        Fields::Named(named) => named
            .named
            .iter()
            .map(|f| {
                let ident = f.ident.clone().expect("named");
                field_piece(f).map(|p| (ident, p))
            })
            .collect(),
        Fields::Unit => Ok(Vec::new()),
        Fields::Unnamed(u) => Err(syn::Error::new_spanned(
            u,
            "tuple fields are not supported here; use named fields",
        )),
    }
}

/// Statements that add every field to `cmd`, then `cmd` itself.
fn augment_body(pieces: &[(Ident, Piece)]) -> TokenStream {
    let steps = pieces.iter().map(|(_, p)| &p.augment);
    quote!(#(#steps)* cmd)
}

fn from_matches_fields(pieces: &[(Ident, Piece)]) -> TokenStream {
    let names = pieces.iter().map(|(i, _)| i);
    let builds = pieces.iter().map(|(_, p)| &p.build);
    quote!(#(#names: #builds,)*)
}

/// `#[derive(Parser)]` and `#[derive(Args)]` on a struct.
pub fn expand_struct(input: &DeriveInput, is_parser: bool) -> syn::Result<TokenStream> {
    let a = krate();
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "`Parser` and `Args` derive on structs; use `Subcommand` for an enum",
        ));
    };
    let name = &input.ident;
    let (impl_g, ty_g, where_g) = input.generics.split_for_impl();
    let pieces = named_fields(&data.fields)?;
    let body = augment_body(&pieces);
    let fields = from_matches_fields(&pieces);

    let mut out = quote! {
        impl #impl_g #a::Args for #name #ty_g #where_g {
            fn augment_args(cmd: #a::Command) -> #a::Command { #body }
        }
        impl #impl_g #a::FromMatches for #name #ty_g #where_g {
            fn from_matches(m: &#a::Matches) -> ::std::result::Result<Self, #a::Error> {
                ::std::result::Result::Ok(Self { #fields })
            }
        }
    };
    if is_parser {
        let attrs = attrs::command(&input.attrs)?;
        let doc = attrs::doc(&input.attrs);
        let command_name = attrs
            .name
            .as_ref()
            .map_or_else(|| quote!(::std::env!("CARGO_PKG_NAME")), |n| quote!(#n));
        let settings = command_settings(&attrs, &doc, true)?;
        out.extend(quote! {
            impl #impl_g #a::CommandFactory for #name #ty_g #where_g {
                fn command() -> #a::Command {
                    let cmd = #a::Command::new(#command_name);
                    #settings
                    <Self as #a::Args>::augment_args(cmd)
                }
            }
            impl #impl_g #a::Parser for #name #ty_g #where_g {}
        });
    }
    Ok(out)
}

fn variant_name(variant: &Variant, attrs: &CommandAttrs) -> TokenStream {
    attrs.name.as_ref().map_or_else(
        || {
            let n = attrs::kebab(&variant.ident.to_string());
            quote!(#n)
        },
        |e| quote!(#e),
    )
}

/// `#[derive(Subcommand)]` on an enum.
pub fn expand_subcommand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let a = krate();
    let Data::Enum(DataEnum { variants, .. }) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "`Subcommand` derives on enums",
        ));
    };
    let name = &input.ident;
    let (impl_g, ty_g, where_g) = input.generics.split_for_impl();

    let mut augments = Vec::new();
    let mut arms = Vec::new();
    for variant in variants {
        let attrs = attrs::command(&variant.attrs)?;
        let doc = attrs::doc(&variant.attrs);
        let command_name = variant_name(variant, &attrs);
        let settings = command_settings(&attrs, &doc, false)?;
        let vident = &variant.ident;

        let (augment_body, construct) = match &variant.fields {
            Fields::Unit => (
                quote!(cmd),
                quote!(::std::result::Result::Ok(Self::#vident)),
            ),
            Fields::Named(_) => {
                let pieces = named_fields(&variant.fields)?;
                let body = augment_body(&pieces);
                let fields = from_matches_fields(&pieces);
                (
                    body,
                    quote!(::std::result::Result::Ok(Self::#vident { #fields })),
                )
            }
            Fields::Unnamed(u) => {
                if u.unnamed.len() != 1 {
                    return Err(syn::Error::new_spanned(
                        u,
                        "a tuple variant takes exactly one field: an `Args` struct (or, with `#[command(subcommand)]`, a `Subcommand` enum)",
                    ));
                }
                let ty = &u.unnamed[0].ty;
                if attrs.subcommand {
                    (
                        quote!(<#ty as #a::Subcommand>::augment_subcommands(cmd).subcommand_required(true)),
                        quote!(<#ty as #a::FromMatches>::from_matches(m).map(Self::#vident)),
                    )
                } else {
                    (
                        quote!(<#ty as #a::Args>::augment_args(cmd)),
                        quote!(<#ty as #a::FromMatches>::from_matches(m).map(Self::#vident)),
                    )
                }
            }
        };
        augments.push(quote! {
            let sub = {
                let cmd = #a::Command::new(#command_name);
                #settings
                #augment_body
            };
            cmd = cmd.subcommand(sub);
        });
        arms.push(quote! {
            ::std::option::Option::Some((#command_name, m)) => #construct,
        });
    }
    Ok(quote! {
        impl #impl_g #a::Subcommand for #name #ty_g #where_g {
            fn augment_subcommands(cmd: #a::Command) -> #a::Command {
                let mut cmd = cmd;
                #(#augments)*
                cmd
            }
        }
        impl #impl_g #a::FromMatches for #name #ty_g #where_g {
            fn from_matches(m: &#a::Matches) -> ::std::result::Result<Self, #a::Error> {
                match m.subcommand() {
                    #(#arms)*
                    _ => ::std::result::Result::Err(#a::Error::missing_subcommand()),
                }
            }
        }
    })
}

/// `#[derive(ValueEnum)]` on a fieldless enum.
pub fn expand_value_enum(input: &DeriveInput) -> syn::Result<TokenStream> {
    let a = krate();
    let Data::Enum(DataEnum { variants, .. }) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "`ValueEnum` derives on enums",
        ));
    };
    let name = &input.ident;
    let mut listed = Vec::new();
    let mut arms = Vec::new();
    for variant in variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(
                variant,
                "`ValueEnum` variants carry no data",
            ));
        }
        let value = attrs::value(&variant.attrs)?;
        let vident = &variant.ident;
        if value.skip {
            arms.push(quote!(Self::#vident => ::std::option::Option::None,));
            continue;
        }
        let spelled = value
            .name
            .unwrap_or_else(|| attrs::kebab(&vident.to_string()));
        let doc = attrs::doc(&variant.attrs);
        let help = value.help.or(doc.short);
        let help = help.map(|h| quote!(.help(#h)));
        listed.push(quote!(Self::#vident));
        arms.push(quote!(Self::#vident => ::std::option::Option::Some(#a::PossibleValue::new(#spelled) #help),));
    }
    Ok(quote! {
        impl #a::ValueEnum for #name {
            fn value_variants() -> &'static [Self] { &[#(#listed),*] }
            fn to_possible_value(&self) -> ::std::option::Option<#a::PossibleValue> {
                match self { #(#arms)* }
            }
        }
    })
}
