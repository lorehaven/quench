//! Reading `#[command(..)]`, `#[arg(..)]`, `#[value(..)]` and doc comments.
//!
//! Anything not implemented is a compile error naming what is supported, never an attribute that silently does
//! nothing.

use syn::meta::ParseNestedMeta;
use syn::{Attribute, Expr, ExprArray, ExprLit, Lit, LitStr, Token};

const COMMAND: &str = "`name`, `version`, `about`, `long_about`, `help_template`, `disable_help_subcommand`, \
`alias`, `aliases`, `visible_alias`, `visible_aliases`, `hide`, `subcommand`, `flatten`";
const ARG: &str = "`long`, `short`, `value_name`, `help`, `long_help`, `default_value`, `default_value_t`, \
`default_missing_value`, `num_args = 0..=1`, `require_equals`, `value_enum`, `global`, `required`, `conflicts_with`, \
`trailing_var_arg`, `allow_hyphen_values`, `hide`";
const VALUE: &str = "`name`, `help`, `skip`";

/// What a doc comment says: the first paragraph, and the whole text when there is more.
#[derive(Default)]
pub struct Doc {
    pub short: Option<String>,
    pub long: Option<String>,
}

pub fn doc(attrs: &[Attribute]) -> Doc {
    let mut lines = Vec::new();
    for attr in attrs.iter().filter(|a| a.path().is_ident("doc")) {
        if let syn::Meta::NameValue(nv) = &attr.meta
            && let Expr::Lit(ExprLit {
                lit: Lit::Str(text),
                ..
            }) = &nv.value
        {
            let value = text.value();
            lines.push(
                value
                    .strip_prefix(' ')
                    .unwrap_or(&value)
                    .trim_end()
                    .to_string(),
            );
        }
    }
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for line in lines {
        if line.is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.join(" "));
                current.clear();
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        paragraphs.push(current.join(" "));
    }
    let Some(first) = paragraphs.first() else {
        return Doc::default();
    };
    let mut short = first.clone();
    if short.ends_with('.') && !short.ends_with("..") && short.matches(". ").count() == 0 {
        short.pop();
    }
    let long = (paragraphs.len() > 1).then(|| paragraphs.join("\n\n"));
    Doc {
        short: Some(short),
        long,
    }
}

/// `version` alone (the crate's version) or `version = expr`.
pub enum Version {
    Cargo,
    Expr(Expr),
}

/// An attribute that may be absent, given bare, or given a value.
#[derive(Default)]
pub enum Given<T> {
    #[default]
    Unset,
    Bare,
    Value(T),
}

/// `long_about` unset, `long_about = None`, or text.
#[derive(Default)]
pub enum LongAbout {
    #[default]
    Unset,
    Suppressed,
    Text(Expr),
}

#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct CommandAttrs {
    pub name: Option<Expr>,
    pub version: Option<Version>,
    pub about: Option<Expr>,
    pub long_about: LongAbout,
    pub help_template: Option<Expr>,
    pub disable_help_subcommand: bool,
    pub aliases: Vec<Expr>,
    pub visible_aliases: Vec<Expr>,
    pub hide: bool,
    pub subcommand: bool,
    pub flatten: bool,
}

fn expr(meta: &ParseNestedMeta<'_>) -> syn::Result<Expr> {
    meta.value()?.parse()
}

fn boolean(meta: &ParseNestedMeta<'_>) -> syn::Result<bool> {
    if meta.input.peek(Token![=]) {
        let lit: syn::LitBool = meta.value()?.parse()?;
        Ok(lit.value)
    } else {
        Ok(true)
    }
}

fn is_none(e: &Expr) -> bool {
    matches!(e, Expr::Path(p) if p.path.is_ident("None"))
}

fn expr_list(e: Expr) -> Vec<Expr> {
    match e {
        Expr::Array(ExprArray { elems, .. }) => elems.into_iter().collect(),
        other => vec![other],
    }
}

pub fn command(attrs: &[Attribute]) -> syn::Result<CommandAttrs> {
    let mut out = CommandAttrs::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("command")) {
        attr.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .map(ToString::to_string)
                .unwrap_or_default();
            match key.as_str() {
                "name" => out.name = Some(expr(&meta)?),
                "version" => {
                    out.version = Some(if meta.input.peek(Token![=]) {
                        Version::Expr(expr(&meta)?)
                    } else {
                        Version::Cargo
                    });
                }
                "about" => out.about = Some(expr(&meta)?),
                "long_about" => {
                    let e = expr(&meta)?;
                    out.long_about = if is_none(&e) {
                        LongAbout::Suppressed
                    } else {
                        LongAbout::Text(e)
                    };
                }
                "help_template" => out.help_template = Some(expr(&meta)?),
                "disable_help_subcommand" => out.disable_help_subcommand = boolean(&meta)?,
                "alias" => out.aliases.push(expr(&meta)?),
                "aliases" => out.aliases.extend(expr_list(expr(&meta)?)),
                "visible_alias" => out.visible_aliases.push(expr(&meta)?),
                "visible_aliases" => out.visible_aliases.extend(expr_list(expr(&meta)?)),
                "hide" => out.hide = boolean(&meta)?,
                "subcommand" => out.subcommand = boolean(&meta)?,
                "flatten" => out.flatten = boolean(&meta)?,
                _ => {
                    return Err(meta.error(format!(
                        "`{key}` is not supported in `#[command(..)]`; supported: {COMMAND}"
                    )));
                }
            }
            Ok(())
        })?;
    }
    Ok(out)
}

#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ArgAttrs {
    pub long: Given<String>,
    pub short: Given<char>,
    pub value_name: Option<Expr>,
    pub help: Option<Expr>,
    pub long_help: Option<Expr>,
    pub default_value: Option<Expr>,
    pub default_value_t: Option<Expr>,
    pub default_missing_value: Option<Expr>,
    pub optional_value: bool,
    pub require_equals: bool,
    pub value_enum: bool,
    pub global: bool,
    pub required: Option<bool>,
    pub conflicts_with: Vec<String>,
    pub trailing_var_arg: bool,
    pub allow_hyphen_values: bool,
    pub hide: bool,
}

pub fn arg(attrs: &[Attribute]) -> syn::Result<ArgAttrs> {
    let mut out = ArgAttrs::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("arg")) {
        attr.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .map(ToString::to_string)
                .unwrap_or_default();
            match key.as_str() {
                "long" => {
                    out.long = if meta.input.peek(Token![=]) {
                        Given::Value(meta.value()?.parse::<LitStr>()?.value())
                    } else {
                        Given::Bare
                    };
                }
                "short" => {
                    out.short = if meta.input.peek(Token![=]) {
                        Given::Value(meta.value()?.parse::<syn::LitChar>()?.value())
                    } else {
                        Given::Bare
                    };
                }
                "value_name" => out.value_name = Some(expr(&meta)?),
                "help" => out.help = Some(expr(&meta)?),
                "long_help" => out.long_help = Some(expr(&meta)?),
                "default_value" => out.default_value = Some(expr(&meta)?),
                "default_value_t" => out.default_value_t = Some(expr(&meta)?),
                "default_missing_value" => out.default_missing_value = Some(expr(&meta)?),
                "num_args" => {
                    let range = expr(&meta)?;
                    let text = quote::ToTokens::to_token_stream(&range)
                        .to_string()
                        .replace(' ', "");
                    if text != "0..=1" {
                        return Err(
                            meta.error("only `num_args = 0..=1` (an optional value) is supported")
                        );
                    }
                    out.optional_value = true;
                }
                "require_equals" => out.require_equals = boolean(&meta)?,
                "value_enum" => out.value_enum = boolean(&meta)?,
                "global" => out.global = boolean(&meta)?,
                "required" => out.required = Some(boolean(&meta)?),
                "conflicts_with" => out
                    .conflicts_with
                    .push(meta.value()?.parse::<LitStr>()?.value()),
                "trailing_var_arg" => out.trailing_var_arg = boolean(&meta)?,
                "allow_hyphen_values" => out.allow_hyphen_values = boolean(&meta)?,
                "hide" => out.hide = boolean(&meta)?,
                _ => {
                    return Err(meta.error(format!(
                        "`{key}` is not supported in `#[arg(..)]`; supported: {ARG}"
                    )));
                }
            }
            Ok(())
        })?;
    }
    Ok(out)
}

#[derive(Default)]
pub struct ValueAttrs {
    pub name: Option<String>,
    pub help: Option<String>,
    pub skip: bool,
}

pub fn value(attrs: &[Attribute]) -> syn::Result<ValueAttrs> {
    let mut out = ValueAttrs::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("value")) {
        attr.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .map(ToString::to_string)
                .unwrap_or_default();
            match key.as_str() {
                "name" => out.name = Some(meta.value()?.parse::<LitStr>()?.value()),
                "help" => out.help = Some(meta.value()?.parse::<LitStr>()?.value()),
                "skip" => out.skip = boolean(&meta)?,
                _ => {
                    return Err(meta.error(format!(
                        "`{key}` is not supported in `#[value(..)]`; supported: {VALUE}"
                    )));
                }
            }
            Ok(())
        })?;
    }
    Ok(out)
}

/// `PascalCase` or `snake_case` to `kebab-case`.
pub fn kebab(name: &str) -> String {
    let chars: Vec<char> = name.trim_start_matches("r#").chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' {
            out.push('-');
        } else if c.is_uppercase() {
            let prev = i.checked_sub(1).map(|p| chars[p]);
            let next = chars.get(i + 1).copied();
            let boundary = prev.is_some_and(|p| {
                p.is_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_uppercase() && next.is_some_and(char::is_lowercase))
            });
            if boundary {
                out.push('-');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
