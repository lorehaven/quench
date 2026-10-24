//! A Jinja-compatible template engine over [`quench_serde::Value`].
//!
//! ```
//! use quench_serde::json;
//! use quench_template::Environment;
//!
//! let mut env = Environment::new();
//! env.add_template("hello", "Hello {{ user.name|upper }}!{% for t in tags %} #{{ t }}{% endfor %}").unwrap();
//! let out = env.render("hello", &json!({ "user": { "name": "ada" }, "tags": ["a", "b"] })).unwrap();
//! assert_eq!(out, "Hello ADA! #a #b");
//! ```
//!
//! Supported: `{{ }}`, `{% %}` and `{# #}` with `-` whitespace control, `raw`; `if`/`elif`/`else`, `for`
//! (with `else`, an `if` filter, tuple targets and the `loop` variable), `set` (also as a block), `with`,
//! `macro`, `import`/`from ... import`, `include`; expressions with Python-like operators, conditional
//! expressions, slices, filters, `is` tests, and method calls on strings and maps. A missing variable is
//! undefined: it prints as nothing and is false, but reading an attribute of it is an error (`default` and
//! `is defined` accept it). Output is never escaped.

// Numeric filters convert between the JSON number types; the single-letter names are the operands of
// arithmetic and the argument lists of the built-ins, which read better short.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::format_push_string,
    clippy::many_single_char_names,
    clippy::nonminimal_bool,
    clippy::option_if_let_else,
    clippy::or_fun_call,
    clippy::struct_field_names,
    clippy::unnecessary_box_returns,
    clippy::useless_let_if_seq,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::unnecessary_wraps
)]

mod builtins;
mod error;
mod render;
mod syntax;
mod value;

pub use builtins::Args;
pub use error::{Error, ErrorKind, Result};
pub use quench_serde::Value;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type FilterFn = Box<dyn Fn(&Value, &Args) -> Result<Value> + Send + Sync>;
type TestFn = Box<dyn Fn(&Value, &[Value]) -> Result<bool> + Send + Sync>;
type FunctionFn = Box<dyn Fn(&Args) -> Result<Value> + Send + Sync>;
type LoaderFn = Box<dyn Fn(&str) -> Result<Option<String>> + Send + Sync>;
type FormatterFn = Box<dyn Fn(&Value) -> Option<String> + Send + Sync>;

/// A parsed template.
#[derive(Debug)]
pub struct Template {
    name: String,
    body: Vec<syntax::Stmt>,
}

impl Template {
    pub(crate) fn body(&self) -> &[syntax::Stmt] {
        &self.body
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Templates, plus the globals, filters, tests and functions they can use.
#[derive(Default)]
pub struct Environment {
    templates: HashMap<String, Arc<Template>>,
    loader: Option<LoaderFn>,
    loaded: Mutex<HashMap<String, Arc<Template>>>,
    globals: HashMap<String, Value>,
    filters: HashMap<String, FilterFn>,
    tests: HashMap<String, TestFn>,
    functions: HashMap<String, FunctionFn>,
    formatter: Option<FormatterFn>,
}

impl std::fmt::Debug for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Environment({} templates)", self.templates.len())
    }
}

impl Environment {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses and registers a template under `name`.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Syntax`] for an invalid template.
    pub fn add_template(&mut self, name: &str, source: &str) -> Result<()> {
        let template = parse(name, source)?;
        self.templates.insert(name.to_string(), Arc::new(template));
        Ok(())
    }

    /// Sets how templates not added with [`add_template`](Self::add_template) are found by `import` and
    /// `include` (and [`render`](Self::render)): the source text, or `None` when there is none.
    pub fn set_loader<F>(&mut self, loader: F)
    where
        F: Fn(&str) -> Result<Option<String>> + Send + Sync + 'static,
    {
        self.loader = Some(Box::new(loader));
        self.loaded.lock().expect("template cache").clear();
    }

    /// A variable every template can read (a render context of the same name wins).
    pub fn add_global(&mut self, name: &str, value: impl Into<Value>) {
        self.globals.insert(name.to_string(), value.into());
    }

    /// Registers a filter: `{{ value|name(args) }}` calls `filter(value, args)`.
    pub fn add_filter<F>(&mut self, name: &str, filter: F)
    where
        F: Fn(&Value, &Args) -> Result<Value> + Send + Sync + 'static,
    {
        self.filters.insert(name.to_string(), Box::new(filter));
    }

    /// Registers a test: `{{ value is name(args) }}`.
    pub fn add_test<F>(&mut self, name: &str, test: F)
    where
        F: Fn(&Value, &[Value]) -> Result<bool> + Send + Sync + 'static,
    {
        self.tests.insert(name.to_string(), Box::new(test));
    }

    /// Registers a function: `{{ name(args) }}`.
    pub fn add_function<F>(&mut self, name: &str, function: F)
    where
        F: Fn(&Args) -> Result<Value> + Send + Sync + 'static,
    {
        self.functions.insert(name.to_string(), Box::new(function));
    }

    /// Sets how `{{ value }}` prints: the text to print, or `None` for the default (strings as they are,
    /// numbers and booleans in their usual form, `none`, lists and maps in a literal-like form).
    pub fn set_formatter<F>(&mut self, formatter: F)
    where
        F: Fn(&Value) -> Option<String> + Send + Sync + 'static,
    {
        self.formatter = Some(Box::new(formatter));
    }

    /// The template `name`: registered, or found by the loader.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::TemplateNotFound`], or the loader's or parser's error.
    pub fn template(&self, name: &str) -> Result<Arc<Template>> {
        if let Some(t) = self.templates.get(name) {
            return Ok(Arc::clone(t));
        }
        if let Some(t) = self.loaded.lock().expect("template cache").get(name) {
            return Ok(Arc::clone(t));
        }
        let source = match &self.loader {
            Some(loader) => loader(name)?,
            None => None,
        };
        let source =
            source.ok_or_else(|| Error::new(ErrorKind::TemplateNotFound, format!("`{name}`")))?;
        let template = Arc::new(parse(name, &source)?);
        self.loaded
            .lock()
            .expect("template cache")
            .insert(name.to_string(), Arc::clone(&template));
        Ok(template)
    }

    /// Renders the template `name` with `context` (a map; anything else means no variables).
    ///
    /// # Errors
    ///
    /// Any [`Error`] raised while rendering.
    pub fn render(&self, name: &str, context: &Value) -> Result<String> {
        let template = self.template(name)?;
        self.run(&template, context)
    }

    /// Parses `source` and renders it once.
    ///
    /// # Errors
    ///
    /// A syntax error, or any [`Error`] raised while rendering.
    pub fn render_str(&self, source: &str, context: &Value) -> Result<String> {
        let template = parse("<string>", source)?;
        self.run(&template, context)
    }

    fn run(&self, template: &Template, context: &Value) -> Result<String> {
        let mut runner = render::Runner::new(self, context, &template.name);
        let mut out = String::new();
        runner.run(&template.body, &mut out)?;
        Ok(out)
    }

    pub(crate) fn global(&self, name: &str) -> Option<&Value> {
        self.globals.get(name)
    }

    pub(crate) fn format(&self, v: &Value) -> String {
        self.formatter
            .as_ref()
            .and_then(|f| f(v))
            .unwrap_or_else(|| value::display(v))
    }

    pub(crate) fn apply_filter(&self, name: &str, v: &Value, args: &Args) -> Result<Value> {
        if let Some(f) = self.filters.get(name) {
            return f(v, args);
        }
        builtins::filter(self, name, v, args).unwrap_or_else(|| {
            Err(Error::new(
                ErrorKind::UnknownName,
                format!("no filter named `{name}`"),
            ))
        })
    }

    pub(crate) fn apply_test(
        &self,
        name: &str,
        subject: Option<&Value>,
        args: &[Value],
    ) -> Result<bool> {
        match (name, subject) {
            ("defined", s) => return Ok(s.is_some()),
            ("undefined", s) => return Ok(s.is_none()),
            _ => {}
        }
        let Some(subject) = subject else {
            return Err(Error::new(
                ErrorKind::Undefined,
                format!("`{name}` was applied to an undefined value"),
            ));
        };
        if let Some(t) = self.tests.get(name) {
            return t(subject, args);
        }
        builtins::test(self, name, subject, args).unwrap_or_else(|| {
            Err(Error::new(
                ErrorKind::UnknownName,
                format!("no test named `{name}`"),
            ))
        })
    }

    pub(crate) fn apply_function(&self, name: &str, args: &Args) -> Result<Value> {
        if let Some(f) = self.functions.get(name) {
            return f(args);
        }
        builtins::function(name, args).unwrap_or_else(|| {
            Err(Error::new(
                ErrorKind::UnknownName,
                format!("no function named `{name}`"),
            ))
        })
    }
}

fn parse(name: &str, source: &str) -> Result<Template> {
    let body = syntax::parse(source).map_err(|e| e.at(name, 0))?;
    Ok(Template {
        name: name.to_string(),
        body,
    })
}
