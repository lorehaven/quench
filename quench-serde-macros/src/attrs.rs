//! Reading `#[serde(..)]` attributes into plain structs.
//!
//! Anything this derive does not implement is a compile error naming what is
//! supported, rather than an attribute that silently does nothing.

use syn::meta::ParseNestedMeta;
use syn::{Attribute, ExprPath, LitStr, Token};

use crate::case::RenameRule;

const CONTAINER: &str = "`rename_all`, `deny_unknown_fields`, `default`, `tag`, `untagged`";
const VARIANT: &str = "`rename`, `alias`, `rename_all`";
const FIELD: &str = "`rename`, `alias`, `default`, `skip`, `skip_serializing`, `skip_deserializing`, `skip_serializing_if`, `flatten`";

#[derive(Clone)]
pub enum DefaultKind {
    /// `Default::default()`
    Trait,
    /// `default = "path"`: a `fn() -> T`
    Path(ExprPath),
}

#[derive(Default)]
pub struct Container {
    pub rename_all: Option<RenameRule>,
    pub deny_unknown_fields: bool,
    pub default: Option<DefaultKind>,
    pub tag: Option<String>,
    pub untagged: bool,
}

#[derive(Default)]
pub struct Variant {
    pub rename: Option<String>,
    pub aliases: Vec<String>,
    pub rename_all: Option<RenameRule>,
}

#[derive(Default)]
pub struct Field {
    pub rename: Option<String>,
    pub aliases: Vec<String>,
    pub default: Option<DefaultKind>,
    pub skip_serializing: bool,
    pub skip_deserializing: bool,
    pub skip_serializing_if: Option<ExprPath>,
    pub flatten: bool,
}

/// Calls `each` for every item inside every `#[serde(..)]` on `attrs`.
fn for_each_meta(
    attrs: &[Attribute],
    mut each: impl FnMut(ParseNestedMeta<'_>) -> syn::Result<()>,
) -> syn::Result<()> {
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        attr.parse_nested_meta(&mut each)?;
    }
    Ok(())
}

fn string(meta: &ParseNestedMeta<'_>) -> syn::Result<String> {
    Ok(meta.value()?.parse::<LitStr>()?.value())
}

fn path(meta: &ParseNestedMeta<'_>) -> syn::Result<ExprPath> {
    meta.value()?.parse::<LitStr>()?.parse()
}

fn rule(meta: &ParseNestedMeta<'_>) -> syn::Result<RenameRule> {
    let name = meta.value()?.parse::<LitStr>()?;
    RenameRule::parse(&name.value()).ok_or_else(|| {
        let known: Vec<_> = RenameRule::ALL
            .iter()
            .map(|(n, _)| format!("`{n}`"))
            .collect();
        syn::Error::new(
            name.span(),
            format!("unknown rename rule, expected one of {}", known.join(", ")),
        )
    })
}

/// `default` or `default = "path"`.
fn default(meta: &ParseNestedMeta<'_>) -> syn::Result<DefaultKind> {
    if meta.input.peek(Token![=]) {
        path(meta).map(DefaultKind::Path)
    } else {
        Ok(DefaultKind::Trait)
    }
}

fn unsupported(meta: &ParseNestedMeta<'_>, what: &str, supported: &str) -> syn::Error {
    meta.error(format!(
        "unsupported serde {what} attribute; supported: {supported}"
    ))
}

pub fn container(attrs: &[Attribute]) -> syn::Result<Container> {
    let mut out = Container::default();
    for_each_meta(attrs, |meta| {
        if meta.path.is_ident("rename_all") {
            out.rename_all = Some(rule(&meta)?);
        } else if meta.path.is_ident("deny_unknown_fields") {
            out.deny_unknown_fields = true;
        } else if meta.path.is_ident("default") {
            out.default = Some(default(&meta)?);
        } else if meta.path.is_ident("tag") {
            out.tag = Some(string(&meta)?);
        } else if meta.path.is_ident("untagged") {
            out.untagged = true;
        } else {
            return Err(unsupported(&meta, "container", CONTAINER));
        }
        Ok(())
    })?;
    if out.tag.is_some() && out.untagged {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "`tag` and `untagged` cannot be combined",
        ));
    }
    Ok(out)
}

pub fn variant(attrs: &[Attribute]) -> syn::Result<Variant> {
    let mut out = Variant::default();
    for_each_meta(attrs, |meta| {
        if meta.path.is_ident("rename") {
            out.rename = Some(string(&meta)?);
        } else if meta.path.is_ident("alias") {
            out.aliases.push(string(&meta)?);
        } else if meta.path.is_ident("rename_all") {
            out.rename_all = Some(rule(&meta)?);
        } else {
            return Err(unsupported(&meta, "variant", VARIANT));
        }
        Ok(())
    })?;
    Ok(out)
}

pub fn field(attrs: &[Attribute]) -> syn::Result<Field> {
    let mut out = Field::default();
    for_each_meta(attrs, |meta| {
        if meta.path.is_ident("rename") {
            out.rename = Some(string(&meta)?);
        } else if meta.path.is_ident("alias") {
            out.aliases.push(string(&meta)?);
        } else if meta.path.is_ident("default") {
            out.default = Some(default(&meta)?);
        } else if meta.path.is_ident("skip") {
            out.skip_serializing = true;
            out.skip_deserializing = true;
        } else if meta.path.is_ident("skip_serializing") {
            out.skip_serializing = true;
        } else if meta.path.is_ident("skip_deserializing") {
            out.skip_deserializing = true;
        } else if meta.path.is_ident("skip_serializing_if") {
            out.skip_serializing_if = Some(path(&meta)?);
        } else if meta.path.is_ident("flatten") {
            out.flatten = true;
        } else {
            return Err(unsupported(&meta, "field", FIELD));
        }
        Ok(())
    })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::{DeriveInput, parse_str};

    fn attrs(source: &str) -> Vec<Attribute> {
        let input: DeriveInput = parse_str(&format!("{source} struct S;")).unwrap();
        input.attrs
    }

    #[test]
    fn container_attributes() {
        let c = container(&attrs(
            r#"#[serde(rename_all = "camelCase", deny_unknown_fields, default, tag = "type")]"#,
        ))
        .unwrap();
        assert_eq!(c.rename_all, Some(RenameRule::Camel));
        assert!(c.deny_unknown_fields);
        assert!(matches!(c.default, Some(DefaultKind::Trait)));
        assert_eq!(c.tag.as_deref(), Some("type"));
        assert!(!c.untagged);

        let c = container(&attrs(
            r#"#[serde(default = "make", untagged)] #[derive(Debug)]"#,
        ))
        .unwrap();
        assert!(matches!(c.default, Some(DefaultKind::Path(_))));
        assert!(c.untagged);

        let none = container(&attrs("#[derive(Debug)]")).unwrap();
        assert!(none.rename_all.is_none() && !none.untagged && none.tag.is_none());
    }

    #[test]
    fn container_errors() {
        let e = container(&attrs("#[serde(bogus)]"))
            .err()
            .unwrap()
            .to_string();
        assert!(
            e.contains("unsupported serde container attribute") && e.contains("rename_all"),
            "{e}"
        );
        let e = container(&attrs(r#"#[serde(rename_all = "Title Case")]"#))
            .err()
            .unwrap()
            .to_string();
        assert!(
            e.contains("unknown rename rule") && e.contains("kebab-case"),
            "{e}"
        );
        let e = container(&attrs(r#"#[serde(tag = "t", untagged)]"#))
            .err()
            .unwrap()
            .to_string();
        assert!(e.contains("cannot be combined"), "{e}");
        assert!(container(&attrs("#[serde(rename_all = 5)]")).is_err());
    }

    #[test]
    fn variant_attributes() {
        let v = variant(&attrs(
            r#"#[serde(rename = "x", alias = "y", alias = "z", rename_all = "snake_case")]"#,
        ))
        .unwrap();
        assert_eq!(v.rename.as_deref(), Some("x"));
        assert_eq!(v.aliases, ["y", "z"]);
        assert_eq!(v.rename_all, Some(RenameRule::Snake));
        let e = variant(&attrs("#[serde(skip)]")).err().unwrap().to_string();
        assert!(e.contains("unsupported serde variant attribute"), "{e}");
    }

    #[test]
    fn field_attributes() {
        let f = field(&attrs(r#"#[serde(rename = "n", alias = "a", default, skip_serializing_if = "Option::is_none", flatten)]"#)).unwrap();
        assert_eq!(f.rename.as_deref(), Some("n"));
        assert_eq!(f.aliases, ["a"]);
        assert!(matches!(f.default, Some(DefaultKind::Trait)));
        assert!(f.skip_serializing_if.is_some());
        assert!(f.flatten);
        assert!(!f.skip_serializing && !f.skip_deserializing);

        let f = field(&attrs(
            r#"#[serde(default = "crate::dflt")] #[serde(skip)]"#,
        ))
        .unwrap();
        assert!(matches!(f.default, Some(DefaultKind::Path(_))));
        assert!(f.skip_serializing && f.skip_deserializing);

        let f = field(&attrs("#[serde(skip_serializing)]")).unwrap();
        assert!(f.skip_serializing && !f.skip_deserializing);
        let f = field(&attrs("#[serde(skip_deserializing)]")).unwrap();
        assert!(!f.skip_serializing && f.skip_deserializing);

        let e = field(&attrs(r#"#[serde(with = "x")]"#))
            .err()
            .unwrap()
            .to_string();
        assert!(
            e.contains("unsupported serde field attribute") && e.contains("flatten"),
            "{e}"
        );
        assert!(field(&attrs("#[serde(default = 5)]")).is_err());
    }

    #[test]
    fn other_attributes_are_ignored() {
        let c = container(&attrs(r#"#[doc = "x"] #[allow(dead_code)]"#)).unwrap();
        assert!(!c.deny_unknown_fields);
    }
}
