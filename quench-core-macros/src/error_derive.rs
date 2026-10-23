//! `#[derive(Error)]`: `Display`, `Error` and `From` from attributes
//! (`thiserror`'s role).
//!
//! ```ignore
//! #[derive(Debug, Error)]
//! enum LoadError {
//!     #[error("cannot read {path}: {source}")]
//!     Read { path: String, source: std::io::Error },
//!     #[error("bad value {0}")]
//!     Bad(String),
//!     #[error(transparent)]
//!     Io(#[from] std::io::Error),
//! }
//! ```
//!
//! In the message, `{name}` and `{0}` name fields; extra format arguments may
//! use `.name` / `.0` for a field (`#[error("{}", .items.join(", "))]`).

use proc_macro2::{Group, Ident, Span, TokenStream, TokenTree};
use quote::{ToTokens, format_ident, quote};
use syn::{Data, DeriveInput, Fields, Index, LitStr, Member, Type, parse2};

pub fn expand(input: TokenStream) -> TokenStream {
    try_expand(input).unwrap_or_else(|e| e.to_compile_error())
}

enum Display {
    /// `#[error("..", args)]`
    Format { text: LitStr, args: TokenStream },
    /// `#[error(transparent)]`
    Transparent,
}

struct Field {
    member: Member,
    binding: Ident,
    ty: Type,
    from: bool,
    source: bool,
}

struct Shape {
    /// `Self::Variant`, or `Self` for a struct.
    path: TokenStream,
    named: Option<bool>,
    fields: Vec<Field>,
    display: Display,
}

fn try_expand(input: TokenStream) -> syn::Result<TokenStream> {
    let input: DeriveInput = parse2(input)?;
    let shapes = match &input.data {
        Data::Struct(data) => {
            let display = display_attr(&input.attrs, input.ident.span())?;
            vec![shape(quote!(Self), &data.fields, display)]
        }
        Data::Enum(data) => data
            .variants
            .iter()
            .map(|variant| {
                let display = display_attr(&variant.attrs, variant.ident.span())?;
                let name = &variant.ident;
                Ok(shape(quote!(Self::#name), &variant.fields, display))
            })
            .collect::<syn::Result<Vec<_>>>()?,
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "#[derive(Error)] does not support unions",
            ));
        }
    };

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let display_arms = shapes
        .iter()
        .map(display_arm)
        .collect::<syn::Result<Vec<_>>>()?;
    let source_arms: Vec<TokenStream> = shapes.iter().filter_map(source_arm).collect();
    let from_impls = shapes
        .iter()
        .filter_map(|s| from_impl(s, name, &input.generics))
        .collect::<syn::Result<Vec<_>>>()?;

    let empty_enum = matches!(&input.data, Data::Enum(data) if data.variants.is_empty());
    let display_body = if empty_enum {
        quote!(match *self {})
    } else {
        quote!(match self { #(#display_arms)* })
    };
    let source_body = if source_arms.is_empty() {
        quote!(::core::option::Option::None)
    } else {
        quote!(match self { #(#source_arms)* #[allow(unreachable_patterns)] _ => ::core::option::Option::None })
    };

    Ok(quote! {
        impl #impl_generics ::core::fmt::Display for #name #ty_generics #where_clause {
            #[allow(unused_variables, clippy::used_underscore_binding)]
            fn fmt(&self, __formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #display_body
            }
        }

        impl #impl_generics ::std::error::Error for #name #ty_generics #where_clause {
            #[allow(clippy::used_underscore_binding)]
            fn source(&self) -> ::core::option::Option<&(dyn ::std::error::Error + 'static)> {
                #source_body
            }
        }

        #(#from_impls)*
    })
}

fn display_attr(attrs: &[syn::Attribute], span: Span) -> syn::Result<Display> {
    let attr = attrs
        .iter()
        .find(|a| a.path().is_ident("error"))
        .ok_or_else(|| syn::Error::new(span, "missing #[error(\"..\")] display message"))?;
    let tokens = attr.meta.require_list()?.tokens.clone();
    let mut iter = tokens.clone().into_iter();
    if let (Some(TokenTree::Ident(word)), None) = (iter.next(), iter.next())
        && word == "transparent"
    {
        return Ok(Display::Transparent);
    }
    let mut iter = tokens.into_iter();
    let Some(TokenTree::Literal(literal)) = iter.next() else {
        return Err(syn::Error::new_spanned(
            attr,
            "expected #[error(\"message\", args..)] or #[error(transparent)]",
        ));
    };
    let text: LitStr = parse2(literal.into_token_stream())
        .map_err(|_| syn::Error::new_spanned(attr, "the message must be a string literal"))?;
    let rest: TokenStream = iter.collect();
    Ok(Display::Format { text, args: rest })
}

fn shape(path: TokenStream, fields: &Fields, display: Display) -> Shape {
    let (named, list): (Option<bool>, Vec<&syn::Field>) = match fields {
        Fields::Named(f) => (Some(true), f.named.iter().collect()),
        Fields::Unnamed(f) => (Some(false), f.unnamed.iter().collect()),
        Fields::Unit => (None, Vec::new()),
    };
    let fields = list
        .into_iter()
        .enumerate()
        .map(|(i, field)| {
            let (member, binding) = field.ident.as_ref().map_or_else(
                || (Member::Unnamed(Index::from(i)), format_ident!("_{i}")),
                |ident| (Member::Named(ident.clone()), ident.clone()),
            );
            let has = |name: &str| field.attrs.iter().any(|a| a.path().is_ident(name));
            let from = has("from");
            let source =
                from || has("source") || matches!(&member, Member::Named(n) if n == "source");
            Field {
                member,
                binding,
                ty: field.ty.clone(),
                from,
                source,
            }
        })
        .collect();
    Shape {
        path,
        named,
        fields,
        display,
    }
}

fn pattern(shape: &Shape) -> TokenStream {
    let path = &shape.path;
    let bindings = shape.fields.iter().map(|f| &f.binding);
    match shape.named {
        None => quote!(#path),
        Some(true) => quote!(#path { #(#bindings),* }),
        Some(false) => quote!(#path ( #(#bindings),* )),
    }
}

fn display_arm(shape: &Shape) -> syn::Result<TokenStream> {
    let pattern = pattern(shape);
    let body = match &shape.display {
        Display::Transparent => {
            let [field] = shape.fields.as_slice() else {
                return Err(syn::Error::new(
                    Span::call_site(),
                    "#[error(transparent)] needs exactly one field",
                ));
            };
            let binding = &field.binding;
            quote!(::core::fmt::Display::fmt(#binding, __formatter))
        }
        Display::Format { text, args } => {
            let named = positional_to_named(&text.value());
            let rewritten = LitStr::new(&named, text.span());
            let args = rewrite_dots(args.clone());
            // Paths have no `Display`; `{path}` prints `path.display()`, as thiserror does.
            let given = explicit_names(&args);
            let plain = plain_placeholders(&named);
            let displays: Vec<TokenStream> = shape
                .fields
                .iter()
                .filter(|f| {
                    is_path_type(&f.ty)
                        && plain.contains(&f.binding.to_string())
                        && !given.contains(&f.binding.to_string())
                })
                .map(|f| {
                    let binding = &f.binding;
                    quote!(, #binding = #binding.display())
                })
                .collect();
            if args.is_empty() {
                quote!(::core::write!(__formatter, #rewritten #(#displays)*))
            } else {
                quote!(::core::write!(__formatter, #rewritten #args #(#displays)*))
            }
        }
    };
    Ok(quote!(#pattern => #body,))
}

/// The pattern that binds only `used`, ignoring the other fields.
fn pattern_binding_only(shape: &Shape, used: &Ident) -> TokenStream {
    let path = &shape.path;
    match shape.named {
        None => quote!(#path),
        Some(true) => quote!(#path { #used, .. }),
        Some(false) => {
            let slots = shape.fields.iter().map(|f| {
                if f.binding == *used {
                    quote!(#used)
                } else {
                    quote!(_)
                }
            });
            quote!(#path ( #(#slots),* ))
        }
    }
}

fn source_arm(shape: &Shape) -> Option<TokenStream> {
    if matches!(shape.display, Display::Transparent) {
        let binding = &shape.fields.first()?.binding;
        let pattern = pattern_binding_only(shape, binding);
        return Some(quote!(#pattern => ::std::error::Error::source(#binding),));
    }
    let field = shape.fields.iter().find(|f| f.source)?;
    let binding = &field.binding;
    let pattern = pattern_binding_only(shape, binding);
    Some(
        quote!(#pattern => ::core::option::Option::Some(#binding as &(dyn ::std::error::Error + 'static)),),
    )
}

fn from_impl(
    shape: &Shape,
    name: &Ident,
    generics: &syn::Generics,
) -> Option<syn::Result<TokenStream>> {
    let field = shape.fields.iter().find(|f| f.from)?;
    if shape.fields.len() != 1 {
        return Some(Err(syn::Error::new_spanned(
            &field.ty,
            "#[from] needs the variant to have exactly one field",
        )));
    }
    let path = &shape.path;
    let ty = &field.ty;
    let build = match &field.member {
        Member::Named(field_name) => quote!(#path { #field_name: source }),
        Member::Unnamed(_) => quote!(#path(source)),
    };
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    Some(Ok(quote! {
        impl #impl_generics ::core::convert::From<#ty> for #name #ty_generics #where_clause {
            fn from(source: #ty) -> Self {
                #build
            }
        }
    }))
}

fn is_path_type(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|s| s.ident == "PathBuf" || s.ident == "Path"),
        Type::Reference(reference) => is_path_type(&reference.elem),
        _ => false,
    }
}

/// Names used as `{name}` with no format spec.
fn plain_placeholders(format: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '{' {
            continue;
        }
        if chars.peek() == Some(&'{') {
            chars.next();
            continue;
        }
        let inner: String = chars.by_ref().take_while(|&d| d != '}').collect();
        if !inner.contains(':') && !inner.is_empty() {
            names.push(inner.trim().to_string());
        }
    }
    names
}

/// Names bound by `name = expr` arguments at the top level of `args`.
fn explicit_names(args: &TokenStream) -> Vec<String> {
    let tokens: Vec<TokenTree> = args.clone().into_iter().collect();
    tokens
        .windows(2)
        .filter_map(|pair| match (&pair[0], &pair[1]) {
            (TokenTree::Ident(name), TokenTree::Punct(eq)) if eq.as_char() == '=' => {
                Some(name.to_string())
            }
            _ => None,
        })
        .collect()
}

/// `{0}` / `{0:?}` become `{_0}` / `{_0:?}`, so tuple fields can be named.
fn positional_to_named(format: &str) -> String {
    let mut out = String::with_capacity(format.len() + 4);
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '{' {
            if chars.peek() == Some(&'{') {
                out.extend(chars.next());
            } else if chars.peek().is_some_and(char::is_ascii_digit) {
                out.push('_');
            }
        }
    }
    out
}

/// `.name` / `.0` at the start of an argument becomes the field's binding.
fn rewrite_dots(args: TokenStream) -> TokenStream {
    let mut out: Vec<TokenTree> = Vec::new();
    let mut tokens = args.into_iter().peekable();
    while let Some(token) = tokens.next() {
        match token {
            TokenTree::Punct(dot) if dot.as_char() == '.' && starts_expression(out.last()) => {
                match tokens.peek() {
                    Some(TokenTree::Ident(field)) => {
                        out.push(TokenTree::Ident(field.clone()));
                        tokens.next();
                    }
                    Some(TokenTree::Literal(index)) => {
                        out.push(TokenTree::Ident(Ident::new(
                            &format!("_{index}"),
                            index.span(),
                        )));
                        tokens.next();
                    }
                    _ => out.push(TokenTree::Punct(dot)),
                }
            }
            TokenTree::Group(group) => {
                let mut rewritten = Group::new(group.delimiter(), rewrite_dots(group.stream()));
                rewritten.set_span(group.span());
                out.push(TokenTree::Group(rewritten));
            }
            other => out.push(other),
        }
    }
    out.into_iter().collect()
}

/// A `.` begins a field reference when nothing before it could be the
/// expression it would select from: argument start, or an operator.
const fn starts_expression(previous: Option<&TokenTree>) -> bool {
    matches!(previous, None | Some(TokenTree::Punct(_)))
}

// The test inputs are format strings by design.
#[cfg(test)]
#[allow(clippy::literal_string_with_formatting_args)]
mod tests {
    use super::*;

    fn run(input: &str) -> String {
        expand(input.parse().unwrap())
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn unit_struct_and_enum_variants_display() {
        let out = run(r#"#[error("boom")] struct E;"#);
        assert!(out.contains("write ! (__formatter , \"boom\")"), "{out}");
        assert!(out.contains("impl :: std :: error :: Error for E"), "{out}");
        assert!(out.contains(":: core :: option :: Option :: None"), "{out}");
    }

    #[test]
    fn tuple_fields_are_named_underscore_n() {
        let out = run(r#"enum E { #[error("bad {0} and {1:?}")] Bad(String, u8) }"#);
        assert!(out.contains("Self :: Bad (_0 , _1) => :: core :: write ! (__formatter , \"bad {_0} and {_1:?}\")"), "{out}");
    }

    #[test]
    fn named_fields_bind_by_name() {
        let out = run(r#"enum E { #[error("{path}: {size}")] Big { path: String, size: u64 } }"#);
        assert!(out.contains("Self :: Big { path , size }"), "{out}");
        assert!(out.contains("\"{path}: {size}\""), "{out}");
    }

    #[test]
    fn path_fields_print_through_display() {
        let out = run(
            r#"enum E { #[error("cannot read {path}: {why}")] Io { path: std::path::PathBuf, why: String } }"#,
        );
        assert!(out.contains(", path = path . display ()"), "{out}");
        assert!(!out.contains("why = "), "{out}");
        let tuple = run(r#"enum E { #[error("at {0}")] At(&'static std::path::Path) }"#);
        assert!(tuple.contains("_0 = _0 . display ()"), "{tuple}");
        // an explicit argument wins, and a debug spec keeps Debug
        let explicit = run(
            r#"enum E { #[error("{path}", path = path.to_string_lossy())] A { path: PathBuf } }"#,
        );
        assert!(!explicit.contains("path = path . display"), "{explicit}");
        let debug = run(r#"enum E { #[error("{path:?}")] A { path: PathBuf } }"#);
        assert!(!debug.contains("display"), "{debug}");
        // a Path field that the message does not mention adds nothing
        let unused = run(r#"enum E { #[error("plain")] A { path: PathBuf } }"#);
        assert!(!unused.contains("display"), "{unused}");
        assert_eq!(plain_placeholders("{{x}} {a} {b:?} {} {c}"), ["a", "c"]);
    }

    #[test]
    fn escaped_braces_are_left_alone() {
        assert_eq!(
            positional_to_named("{{0}} {0} {name} {0:>4}"),
            "{{0}} {_0} {name} {_0:>4}"
        );
        assert_eq!(positional_to_named("{"), "{");
    }

    #[test]
    fn extra_arguments_may_use_leading_dots() {
        let out = run(
            r#"enum E { #[error("{}", .0.join(", "))] Cycle(Vec<String>), #[error("{}", .path.join("->"))] Path { path: Vec<String> } }"#,
        );
        assert!(out.contains("_0 . join (\", \")"), "{out}");
        assert!(out.contains("path . join (\"->\")"), "{out}");
        assert!(!out.contains(". _0"), "{out}");
    }

    #[test]
    fn dots_inside_groups_and_after_operators_are_rewritten_but_method_dots_are_not() {
        let out = run(
            r#"enum E { #[error("{} {}", format!("{}", .0), &.1)] A(u8, u8), #[error("{}", name.as_ref().map(|n| n.len()).unwrap_or(0))] B { name: Option<String> } }"#,
        );
        assert!(out.contains("format ! (\"{}\" , _0)"), "{out}");
        assert!(out.contains("&_1"), "{out}");
        assert!(out.contains("name . as_ref () . map"), "{out}");
    }

    #[test]
    fn a_lone_dot_with_nothing_after_it_is_left_as_is() {
        let rewritten = rewrite_dots(quote!(.)).to_string();
        assert_eq!(rewritten, ".");
    }

    #[test]
    fn transparent_delegates_display_and_source() {
        let out = run("enum E { #[error(transparent)] Io(#[from] std::io::Error) }");
        assert!(
            out.contains(":: core :: fmt :: Display :: fmt (_0 , __formatter)"),
            "{out}"
        );
        assert!(
            out.contains(":: std :: error :: Error :: source (_0)"),
            "{out}"
        );
        assert!(
            out.contains("impl :: core :: convert :: From < std :: io :: Error > for E"),
            "{out}"
        );
    }

    #[test]
    fn source_comes_from_from_source_or_a_field_called_source() {
        let from = run(r#"enum E { #[error("x")] A(#[from] std::io::Error) }"#);
        assert!(
            from.contains("Some (_0 as & (dyn :: std :: error :: Error + 'static))"),
            "{from}"
        );
        let attr = run(r#"enum E { #[error("x")] A(#[source] std::io::Error, u8) }"#);
        assert!(
            attr.contains("Self :: A (_0 , _) => :: core :: option :: Option :: Some (_0 as"),
            "{attr}"
        );
        let named = run(r#"enum E { #[error("x")] A { source: std::io::Error } }"#);
        assert!(
            named.contains(
                "Self :: A { source , .. } => :: core :: option :: Option :: Some (source as"
            ),
            "{named}"
        );
        let none = run(r#"enum E { #[error("x")] A(u8) }"#);
        assert!(!none.contains("as & (dyn"), "{none}");
    }

    #[test]
    fn from_with_named_field() {
        let out = run(r#"enum E { #[error("x")] A { #[from] inner: std::io::Error } }"#);
        assert!(out.contains("Self :: A { inner : source }"), "{out}");
    }

    #[test]
    fn struct_with_fields() {
        let out = run(r#"#[error("{code}")] struct E { code: u8 }"#);
        assert!(out.contains("Self { code } =>"), "{out}");
        let tuple = run(r#"#[error("{0}")] struct E(u8);"#);
        assert!(tuple.contains("Self (_0) =>"), "{tuple}");
    }

    #[test]
    fn generics_are_carried_over() {
        let out = run(r#"#[error("x")] struct E<T: std::fmt::Debug> { v: T }"#);
        assert!(
            out.contains("impl < T : std :: fmt :: Debug > :: core :: fmt :: Display for E < T >"),
            "{out}"
        );
    }

    #[test]
    fn empty_enums_compile() {
        let out = run("enum E {}");
        assert!(out.contains("match * self { }"), "{out}");
    }

    #[test]
    fn errors_are_reported() {
        assert!(run("enum E { A }").contains("missing #[error"));
        assert!(run("#[error(transparent)] struct E;").contains("exactly one field"));
        assert!(
            run(r#"enum E { #[error("x")] A(#[from] std::io::Error, u8) }"#)
                .contains("exactly one field")
        );
        assert!(run("union U { a: u8 }").contains("does not support unions"));
        assert!(run("#[error(42)] struct E;").contains("string literal"));
        assert!(run("#[error(a b)] struct E;").contains("expected #[error"));
        assert!(run("#[error] struct E;").contains("compile_error"));
        assert!(run("not rust").contains("compile_error"));
    }
}
