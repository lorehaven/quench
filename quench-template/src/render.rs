//! Running a parsed template.

use crate::Environment;
use crate::builtins::{self, Args, items};
use crate::error::{Error, ErrorKind, Result};
use crate::syntax::{BinOp, Expr, MacroDef, Stmt};
use crate::value::{self, compare, display, eq, truthy, type_name};
use quench_serde::{Map, Value};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

const MAX_DEPTH: usize = 32;

pub type Vars = HashMap<String, Val>;
type Scope = Rc<RefCell<Vars>>;

/// What an expression evaluates to: data, or something only templates handle.
#[derive(Clone)]
pub enum Val {
    Undefined,
    Data(Value),
    Macro(Rc<Callable>),
    Module(Rc<Vars>),
}

pub struct Callable {
    def: Arc<MacroDef>,
    scope: Scope,
    template: String,
}

fn undefined(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Undefined, message)
}

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidOperation, message)
}

pub struct Runner<'e> {
    env: &'e Environment,
    root: &'e Value,
    scopes: Vec<Scope>,
    name: String,
    depth: usize,
}

impl<'e> Runner<'e> {
    pub fn new(env: &'e Environment, root: &'e Value, name: &str) -> Self {
        Self {
            env,
            root,
            scopes: vec![Rc::new(RefCell::new(Vars::new()))],
            name: name.to_string(),
            depth: 0,
        }
    }

    /// The variables the template set at its top level (macros and `set`s), for imports.
    pub fn exports(&self) -> Vars {
        self.scopes[0].borrow().clone()
    }

    fn lookup(&self, name: &str) -> Val {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.borrow().get(name) {
                return v.clone();
            }
        }
        if let Value::Object(map) = self.root
            && let Some(v) = map.get(name)
        {
            return Val::Data(v.clone());
        }
        self.env
            .global(name)
            .map_or(Val::Undefined, |v| Val::Data(v.clone()))
    }

    fn set(&self, name: &str, value: Val) {
        self.scopes
            .last()
            .expect("a scope")
            .borrow_mut()
            .insert(name.to_string(), value);
    }

    fn push(&mut self) {
        self.scopes.push(Rc::new(RefCell::new(Vars::new())));
    }

    pub fn run(&mut self, body: &[Stmt], out: &mut String) -> Result<()> {
        for stmt in body {
            self.statement(stmt, out)?;
        }
        Ok(())
    }

    fn data(&mut self, expr: &Expr) -> Result<Value> {
        match self.eval(expr)? {
            Val::Data(v) => Ok(v),
            Val::Undefined => Err(undefined("the value is undefined")),
            _ => Err(invalid("a macro or module cannot be used as a value")),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn statement(&mut self, stmt: &Stmt, out: &mut String) -> Result<()> {
        match stmt {
            Stmt::Text(text) => out.push_str(text),
            Stmt::Output(expr, line) => {
                let value = self.eval(expr).map_err(|e| e.at(&self.name, *line))?;
                match value {
                    Val::Undefined => {}
                    Val::Data(v) => out.push_str(&self.env.format(&v)),
                    _ => {
                        return Err(
                            invalid("a macro or module cannot be printed").at(&self.name, *line)
                        );
                    }
                }
            }
            Stmt::If(branches, otherwise) => {
                for (condition, body) in branches {
                    if self.truth(condition)? {
                        return self.run(body, out);
                    }
                }
                self.run(otherwise, out)?;
            }
            Stmt::For {
                targets,
                iter,
                filter,
                body,
                otherwise,
                line,
            } => {
                let list = match self.eval(iter).map_err(|e| e.at(&self.name, *line))? {
                    Val::Undefined => Vec::new(),
                    Val::Data(v) => items(&v).map_err(|e| e.at(&self.name, *line))?,
                    _ => return Err(invalid("a macro is not iterable").at(&self.name, *line)),
                };
                self.push();
                let result =
                    self.for_loop(targets, list, filter.as_ref(), body, otherwise, out, *line);
                self.scopes.pop();
                result?;
            }
            Stmt::Set(names, expr, line) => {
                let value = self.eval(expr).map_err(|e| e.at(&self.name, *line))?;
                if let [name] = names.as_slice() {
                    self.set(name, value);
                } else {
                    let Val::Data(Value::Array(parts)) = value else {
                        return Err(invalid("cannot unpack a value that is not a sequence")
                            .at(&self.name, *line));
                    };
                    if parts.len() != names.len() {
                        return Err(
                            invalid("wrong number of values to unpack").at(&self.name, *line)
                        );
                    }
                    for (name, part) in names.iter().zip(parts) {
                        self.set(name, Val::Data(part));
                    }
                }
            }
            Stmt::SetBlock(name, body) => {
                let mut text = String::new();
                self.run(body, &mut text)?;
                self.set(name, Val::Data(Value::String(text)));
            }
            Stmt::Macro(def) => {
                let callable = Callable {
                    def: Arc::clone(def),
                    scope: Rc::clone(&self.scopes[0]),
                    template: self.name.clone(),
                };
                self.set(&def.name, Val::Macro(Rc::new(callable)));
            }
            Stmt::Import(template, alias, line) => {
                let module = self.import(template).map_err(|e| e.at(&self.name, *line))?;
                self.set(alias, Val::Module(Rc::new(module)));
            }
            Stmt::FromImport(template, names, line) => {
                let module = self.import(template).map_err(|e| e.at(&self.name, *line))?;
                for (name, alias) in names {
                    let value = module.get(name).cloned().ok_or_else(|| {
                        invalid(format!("the template has no `{name}` to import"))
                            .at(&self.name, *line)
                    })?;
                    self.set(alias, value);
                }
            }
            Stmt::Include(template, ignore_missing, line) => {
                let name = self.data(template).map_err(|e| e.at(&self.name, *line))?;
                let name = display(&name);
                match self.env.template(&name) {
                    Ok(tpl) => {
                        let mut child = Runner {
                            env: self.env,
                            root: self.root,
                            scopes: self.scopes.clone(),
                            name: name.clone(),
                            depth: self.depth + 1,
                        };
                        if child.depth > MAX_DEPTH {
                            return Err(invalid("templates include each other too deeply")
                                .at(&self.name, *line));
                        }
                        child.push();
                        child.run(tpl.body(), out)?;
                    }
                    Err(e) if *ignore_missing && e.kind() == ErrorKind::TemplateNotFound => {}
                    Err(e) => return Err(e.at(&self.name, *line)),
                }
            }
            Stmt::With(bindings, body) => {
                let mut values = Vec::new();
                for (name, expr) in bindings {
                    values.push((name.clone(), self.eval(expr)?));
                }
                self.push();
                for (name, value) in values {
                    self.set(&name, value);
                }
                let result = self.run(body, out);
                self.scopes.pop();
                result?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn for_loop(
        &mut self,
        targets: &[String],
        list: Vec<Value>,
        filter: Option<&Expr>,
        body: &[Stmt],
        otherwise: &[Stmt],
        out: &mut String,
        line: usize,
    ) -> Result<()> {
        let mut kept = Vec::new();
        for item in list {
            if let Some(condition) = filter {
                self.bind(targets, &item, line)?;
                if !self.truth(condition)? {
                    continue;
                }
            }
            kept.push(item);
        }
        if kept.is_empty() {
            return self.run(otherwise, out);
        }
        let length = kept.len();
        let mut previous = Value::Null;
        for (index, item) in kept.iter().enumerate() {
            self.bind(targets, item, line)?;
            let mut state = Map::new();
            state.insert("index", Value::from(index as u64 + 1));
            state.insert("index0", Value::from(index as u64));
            state.insert("first", Value::Bool(index == 0));
            state.insert("last", Value::Bool(index + 1 == length));
            state.insert("length", Value::from(length as u64));
            state.insert("revindex", Value::from((length - index) as u64));
            state.insert("revindex0", Value::from((length - index - 1) as u64));
            state.insert("previtem", previous.clone());
            state.insert(
                "nextitem",
                kept.get(index + 1).cloned().unwrap_or(Value::Null),
            );
            self.set("loop", Val::Data(Value::Object(state)));
            previous = item.clone();
            self.run(body, out)?;
        }
        Ok(())
    }

    fn bind(&self, targets: &[String], item: &Value, line: usize) -> Result<()> {
        if let [name] = targets {
            self.set(name, Val::Data(item.clone()));
            return Ok(());
        }
        let Value::Array(parts) = item else {
            return Err(
                invalid("cannot unpack a value that is not a sequence").at(&self.name, line)
            );
        };
        if parts.len() != targets.len() {
            return Err(invalid("wrong number of values to unpack").at(&self.name, line));
        }
        for (name, part) in targets.iter().zip(parts) {
            self.set(name, Val::Data(part.clone()));
        }
        Ok(())
    }

    fn import(&mut self, template: &Expr) -> Result<Vars> {
        let name = self.data(template)?;
        let name = display(&name);
        let tpl = self.env.template(&name)?;
        if self.depth >= MAX_DEPTH {
            return Err(invalid("templates import each other too deeply"));
        }
        let none = Value::Null;
        let mut child = Runner::new(self.env, &none, &name);
        child.depth = self.depth + 1;
        let mut sink = String::new();
        child.run(tpl.body(), &mut sink)?;
        Ok(child.exports())
    }

    fn truth(&mut self, expr: &Expr) -> Result<bool> {
        Ok(match self.eval(expr)? {
            Val::Undefined => false,
            Val::Data(v) => truthy(&v),
            _ => true,
        })
    }

    /// The value, or `None` if it is undefined (including an attribute of an undefined value).
    fn eval_defined(&mut self, expr: &Expr) -> Result<Option<Value>> {
        match self.eval(expr) {
            Ok(Val::Data(v)) => Ok(Some(v)),
            Ok(Val::Undefined) => Ok(None),
            Ok(_) => Ok(Some(Value::Null)),
            Err(e) if e.kind() == ErrorKind::Undefined => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Like `eval`, but an attribute of an undefined value is undefined rather than an error.
    fn eval_lenient(&mut self, expr: &Expr) -> Result<Val> {
        match self.eval(expr) {
            Err(e) if e.kind() == ErrorKind::Undefined => Ok(Val::Undefined),
            other => other,
        }
    }

    fn arguments(&mut self, args: &[Expr], kwargs: &[(String, Expr)]) -> Result<Args> {
        let mut out = Args::default();
        for arg in args {
            out.positional.push(self.data(arg)?);
        }
        for (name, arg) in kwargs {
            out.keyword.push((name.clone(), self.data(arg)?));
        }
        Ok(out)
    }

    #[allow(clippy::too_many_lines)]
    pub fn eval(&mut self, expr: &Expr) -> Result<Val> {
        let data = |v: Value| Ok(Val::Data(v));
        match expr {
            Expr::Const(v) => data(v.clone()),
            Expr::Var(name) => Ok(self.lookup(name)),
            Expr::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.data(item)?);
                }
                data(Value::Array(out))
            }
            Expr::Dict(pairs) => {
                let mut map = Map::new();
                for (k, v) in pairs {
                    let key = self.data(k)?;
                    map.insert(display(&key), self.data(v)?);
                }
                data(Value::Object(map))
            }
            Expr::Attr(base, name) => match self.eval(base)? {
                Val::Undefined => Err(undefined(format!(
                    "cannot read `{name}` of an undefined value"
                ))),
                Val::Module(vars) => Ok(vars.get(name).cloned().unwrap_or(Val::Undefined)),
                Val::Data(Value::Object(map)) => Ok(map
                    .get(name)
                    .map_or(Val::Undefined, |v| Val::Data(v.clone()))),
                _ => Ok(Val::Undefined),
            },
            Expr::Index(base, index) => {
                let Val::Data(base) = self.eval(base)? else {
                    return Err(undefined("cannot index an undefined value"));
                };
                let index = self.data(index)?;
                Ok(subscript(&base, &index).map_or(Val::Undefined, Val::Data))
            }
            Expr::Slice(base, parts) => {
                let base = self.data(base)?;
                let mut bounds = [None, None, None];
                for (slot, part) in bounds.iter_mut().zip(parts) {
                    if let Some(e) = part {
                        *slot = match self.data(e)? {
                            Value::Null => None,
                            other => Some(match value::num(&other) {
                                Some(value::Num::Int(i)) => i64::try_from(i)
                                    .map_err(|_| invalid("slice bound too large"))?,
                                _ => return Err(invalid("slice bounds must be integers")),
                            }),
                        };
                    }
                }
                data(slice(&base, bounds)?)
            }
            Expr::Call(callee, args, kwargs) => self.call(callee, args, kwargs),
            Expr::Filter(inner, name, args, kwargs) => {
                if name == "default" || name == "d" {
                    // The fallback may itself be undefined, which then stays undefined.
                    let mut fallback = Val::Data(Value::String(String::new()));
                    if let Some(first) = args.first() {
                        fallback = self.eval_lenient(first)?;
                    } else if let Some((_, e)) = kwargs.iter().find(|(k, _)| k == "default_value") {
                        fallback = self.eval_lenient(e)?;
                    }
                    let boolean = match args.get(1) {
                        Some(e) => self.truth(e)?,
                        None => match kwargs.iter().find(|(k, _)| k == "boolean") {
                            Some((_, e)) => self.truth(e)?,
                            None => false,
                        },
                    };
                    return Ok(match self.eval_defined(inner)? {
                        Some(v) if !(boolean && !truthy(&v)) => Val::Data(v),
                        _ => fallback,
                    });
                }
                let subject = self.data(inner)?;
                let args = self.arguments(args, kwargs)?;
                data(self.env.apply_filter(name, &subject, &args)?)
            }
            Expr::Test(inner, name, args, negate) => {
                let subject = self.eval_defined(inner)?;
                let args = self.arguments(args, &[])?.positional;
                let result = self.env.apply_test(name, subject.as_ref(), &args)?;
                data(Value::Bool(result != *negate))
            }
            Expr::Neg(inner) => {
                let v = self.data(inner)?;
                data(value::arithmetic('-', &Value::from(0), &v)?)
            }
            Expr::Not(inner) => Ok(Val::Data(Value::Bool(!self.truth(inner)?))),
            Expr::And(a, b) => {
                let left = self.eval(a)?;
                if !val_truthy(&left) {
                    return Ok(left);
                }
                self.eval(b)
            }
            Expr::Or(a, b) => {
                let left = self.eval(a)?;
                if val_truthy(&left) {
                    return Ok(left);
                }
                self.eval(b)
            }
            Expr::Cond(test, yes, no) => {
                if self.truth(test)? {
                    self.eval(yes)
                } else if let Some(no) = no {
                    self.eval(no)
                } else {
                    Ok(Val::Undefined)
                }
            }
            Expr::Binary(op, a, b) => {
                let left = self.data(a)?;
                let right = self.data(b)?;
                data(binary(*op, &left, &right)?)
            }
        }
    }

    fn call(&mut self, callee: &Expr, args: &[Expr], kwargs: &[(String, Expr)]) -> Result<Val> {
        // A name that is not a variable may be a registered or built-in function.
        if let Expr::Var(name) = callee
            && matches!(self.lookup(name), Val::Undefined)
        {
            let args = self.arguments(args, kwargs)?;
            return self.env.apply_function(name, &args).map(Val::Data);
        }
        if let Expr::Attr(base, name) = callee {
            match self.eval(base)? {
                Val::Data(v) => {
                    let args = self.arguments(args, kwargs)?;
                    return match builtins::method(&v, name, &args) {
                        Some(result) => result.map(Val::Data),
                        None => Err(invalid(format!("{} has no method `{name}`", type_name(&v)))),
                    };
                }
                Val::Module(vars) => {
                    let target = vars.get(name).cloned().unwrap_or(Val::Undefined);
                    return self.invoke(&target, name, args, kwargs);
                }
                Val::Undefined => {
                    return Err(undefined(format!(
                        "cannot call `{name}` on an undefined value"
                    )));
                }
                Val::Macro(_) => return Err(invalid("a macro has no methods")),
            }
        }
        let target = self.eval(callee)?;
        self.invoke(&target, "value", args, kwargs)
    }

    fn invoke(
        &mut self,
        target: &Val,
        name: &str,
        args: &[Expr],
        kwargs: &[(String, Expr)],
    ) -> Result<Val> {
        let Val::Macro(callable) = target else {
            return Err(invalid(format!("`{name}` is not callable")));
        };
        if self.depth >= MAX_DEPTH {
            return Err(invalid("macros call each other too deeply"));
        }
        let def = &callable.def;
        if args.len() > def.params.len() {
            return Err(invalid(format!(
                "macro `{}` takes {} arguments",
                def.name,
                def.params.len()
            )));
        }
        let mut given: Vec<Option<Val>> = vec![None; def.params.len()];
        for (slot, arg) in given.iter_mut().zip(args) {
            *slot = Some(self.eval(arg)?);
        }
        for (key, arg) in kwargs {
            let at = def
                .params
                .iter()
                .position(|p| &p.name == key)
                .ok_or_else(|| invalid(format!("macro `{}` has no argument `{key}`", def.name)))?;
            given[at] = Some(self.eval(arg)?);
        }

        let mut inner = Runner {
            env: self.env,
            root: self.root,
            scopes: vec![Rc::clone(&callable.scope)],
            name: callable.template.clone(),
            depth: self.depth + 1,
        };
        inner.push();
        for (param, value) in def.params.iter().zip(given) {
            let value = match (value, &param.default) {
                (Some(v), _) => v,
                (None, Some(default)) => inner.eval(default)?,
                (None, None) => Val::Undefined,
            };
            inner.set(&param.name, value);
        }
        let mut text = String::new();
        inner.run(&def.body, &mut text)?;
        Ok(Val::Data(Value::String(text)))
    }
}

fn val_truthy(v: &Val) -> bool {
    match v {
        Val::Undefined => false,
        Val::Data(d) => truthy(d),
        _ => true,
    }
}

fn subscript(base: &Value, index: &Value) -> Option<Value> {
    match (base, index) {
        (Value::Array(items), idx) => {
            let Some(value::Num::Int(i)) = value::num(idx) else {
                return None;
            };
            let at = if i < 0 { items.len() as i128 + i } else { i };
            usize::try_from(at)
                .ok()
                .and_then(|at| items.get(at))
                .cloned()
        }
        (Value::Object(map), Value::String(key)) => map.get(key).cloned(),
        (Value::String(s), idx) => {
            let Some(value::Num::Int(i)) = value::num(idx) else {
                return None;
            };
            let chars: Vec<char> = s.chars().collect();
            let at = if i < 0 { chars.len() as i128 + i } else { i };
            usize::try_from(at)
                .ok()
                .and_then(|at| chars.get(at))
                .map(|c| Value::String(c.to_string()))
        }
        _ => None,
    }
}

fn slice(base: &Value, [start, stop, step]: [Option<i64>; 3]) -> Result<Value> {
    let step = step.unwrap_or(1);
    if step == 0 {
        return Err(invalid("slice step cannot be zero"));
    }
    let list: Vec<Value> = match base {
        Value::Array(a) => a.clone(),
        Value::String(s) => s.chars().map(|c| Value::String(c.to_string())).collect(),
        other => return Err(invalid(format!("cannot slice a {}", type_name(other)))),
    };
    let len = list.len() as i64;
    let clamp = |bound: Option<i64>, default: i64, low: i64, high: i64| {
        bound.map_or(default, |b| {
            if b < 0 {
                (b + len).max(low)
            } else {
                b.min(high)
            }
        })
    };
    let picked: Vec<Value> = if step > 0 {
        let (from, to) = (clamp(start, 0, 0, len), clamp(stop, len, 0, len));
        (from..to)
            .step_by(step as usize)
            .map(|i| list[i as usize].clone())
            .collect()
    } else {
        let (from, to) = (
            clamp(start, len - 1, -1, len - 1),
            clamp(stop, -1, -1, len - 1),
        );
        let mut out = Vec::new();
        let mut i = from;
        while i > to && i >= 0 {
            out.push(list[i as usize].clone());
            i += step;
        }
        out
    };
    Ok(if matches!(base, Value::String(_)) {
        Value::String(picked.iter().map(display).collect())
    } else {
        Value::Array(picked)
    })
}

fn binary(op: BinOp, a: &Value, b: &Value) -> Result<Value> {
    let ordering = |wanted: fn(Ordering) -> bool| -> Result<Value> {
        compare(a, b)
            .map(|o| Value::Bool(wanted(o)))
            .ok_or_else(|| {
                invalid(format!(
                    "cannot compare {} and {}",
                    type_name(a),
                    type_name(b)
                ))
            })
    };
    match op {
        BinOp::Add => value::add(a, b),
        BinOp::Sub => value::arithmetic('-', a, b),
        BinOp::Mul => value::arithmetic('*', a, b),
        BinOp::Div => value::arithmetic('/', a, b),
        BinOp::FloorDiv => value::arithmetic('d', a, b),
        BinOp::Rem => value::arithmetic('%', a, b),
        BinOp::Pow => value::arithmetic('p', a, b),
        BinOp::Concat => Ok(Value::String(format!("{}{}", display(a), display(b)))),
        BinOp::Eq => Ok(Value::Bool(eq(a, b))),
        BinOp::Ne => Ok(Value::Bool(!eq(a, b))),
        BinOp::Lt => ordering(|o| o == Ordering::Less),
        BinOp::Le => ordering(|o| o != Ordering::Greater),
        BinOp::Gt => ordering(|o| o == Ordering::Greater),
        BinOp::Ge => ordering(|o| o != Ordering::Less),
        BinOp::In => value::contains(b, a).map(Value::Bool),
        BinOp::NotIn => value::contains(b, a).map(|c| Value::Bool(!c)),
    }
}
