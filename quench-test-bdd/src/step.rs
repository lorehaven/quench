//! Step definitions: what `#[given]`, `#[when]` and `#[then]` register, and the context a step runs with.

use crate::BoxFuture;
use crate::gherkin::Step;
use std::any::{Any, TypeId};
use std::fmt::Display;
use std::str::FromStr;

pub use crate::gherkin::StepKind as Kind;

/// What a running step knows about itself.
#[derive(Debug)]
pub struct Context<'a> {
    /// The step as written in the feature file (with its doc string and table).
    pub step: &'a Step,
    /// The arguments the pattern captured, in order.
    pub args: Vec<String>,
}

/// One registered step function.
pub struct Definition {
    pub kind: Kind,
    pub pattern: &'static str,
    /// Whether `pattern` is a regex instead of a cucumber expression.
    pub regex: bool,
    pub file: &'static str,
    pub line: u32,
    /// The `TypeId` of the world the step belongs to.
    pub world: fn() -> TypeId,
    pub run:
        for<'a> fn(&'a mut (dyn Any + Send), &'a Context<'a>) -> BoxFuture<'a, Result<(), String>>,
}

/// Reads capture `index` as a `T`; `label` names the parameter in the error.
///
/// # Errors
///
/// A message when the pattern captured fewer arguments or the text is not a `T`.
pub fn parse_argument<T>(context: &Context<'_>, index: usize, label: &str) -> Result<T, String>
where
    T: FromStr,
    T::Err: Display,
{
    let raw = context
        .args
        .get(index)
        .ok_or_else(|| format!("the pattern captures no argument for `{label}`"))?;
    raw.parse::<T>()
        .map_err(|e| format!("cannot read `{raw}` as `{label}`: {e}"))
}

/// What a step function may return: nothing, or a `Result` whose error fails the step.
pub trait StepOutcome {
    /// # Errors
    ///
    /// The failure message.
    fn into_result(self) -> Result<(), String>;
}

impl StepOutcome for () {
    fn into_result(self) -> Result<(), String> {
        Ok(())
    }
}

impl<E: Display> StepOutcome for Result<(), E> {
    fn into_result(self) -> Result<(), String> {
        self.map_err(|e| e.to_string())
    }
}

impl std::fmt::Debug for Definition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Definition")
            .field("kind", &self.kind)
            .field("pattern", &self.pattern)
            .field("regex", &self.regex)
            .field("at", &format_args!("{}:{}", self.file, self.line))
            .finish_non_exhaustive()
    }
}
