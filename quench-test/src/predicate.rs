//! Predicates: checks on values with a description, for assertions on command output.
//!
//! ```
//! use quench_test::predicate::prelude::*;
//!
//! let has_greeting = predicate::str::contains("hello").and(predicate::str::ends_with("!"));
//! assert!(has_greeting.eval("say hello world!"));
//! assert!(predicate::str::contains("bye").not().eval("hello"));
//! ```

use std::fmt;

/// A check on a value of type `T`.
pub trait Predicate<T: ?Sized> {
    /// Whether the value passes.
    fn eval(&self, value: &T) -> bool;

    /// What the predicate requires, for failure messages.
    fn describe(&self) -> String;

    /// Passes when both pass.
    fn and<P: Predicate<T>>(self, other: P) -> And<Self, P>
    where
        Self: Sized,
    {
        And(self, other)
    }

    /// Passes when either passes.
    fn or<P: Predicate<T>>(self, other: P) -> Or<Self, P>
    where
        Self: Sized,
    {
        Or(self, other)
    }

    /// Passes when this does not.
    fn not(self) -> Not<Self>
    where
        Self: Sized,
    {
        Not(self)
    }
}

/// Both predicates must pass.
#[derive(Debug, Clone)]
pub struct And<A, B>(A, B);

/// Either predicate must pass.
#[derive(Debug, Clone)]
pub struct Or<A, B>(A, B);

/// The predicate must not pass.
#[derive(Debug, Clone)]
pub struct Not<A>(A);

impl<T: ?Sized, A: Predicate<T>, B: Predicate<T>> Predicate<T> for And<A, B> {
    fn eval(&self, value: &T) -> bool {
        self.0.eval(value) && self.1.eval(value)
    }

    fn describe(&self) -> String {
        format!("({} and {})", self.0.describe(), self.1.describe())
    }
}

impl<T: ?Sized, A: Predicate<T>, B: Predicate<T>> Predicate<T> for Or<A, B> {
    fn eval(&self, value: &T) -> bool {
        self.0.eval(value) || self.1.eval(value)
    }

    fn describe(&self) -> String {
        format!("({} or {})", self.0.describe(), self.1.describe())
    }
}

impl<T: ?Sized, A: Predicate<T>> Predicate<T> for Not<A> {
    fn eval(&self, value: &T) -> bool {
        !self.0.eval(value)
    }

    fn describe(&self) -> String {
        format!("not {}", self.0.describe())
    }
}

/// Always passes.
#[derive(Debug, Clone, Copy)]
pub struct Always;

impl<T: ?Sized> Predicate<T> for Always {
    fn eval(&self, _: &T) -> bool {
        true
    }

    fn describe(&self) -> String {
        "anything".to_string()
    }
}

#[must_use]
pub const fn always() -> Always {
    Always
}

/// Passes when the value equals the expected one.
#[derive(Debug, Clone)]
pub struct Equals<T>(T);

impl<T: PartialEq + fmt::Debug> Predicate<T> for Equals<T> {
    fn eval(&self, value: &T) -> bool {
        self.0 == *value
    }

    fn describe(&self) -> String {
        format!("equal to {:?}", self.0)
    }
}

impl Predicate<str> for Equals<&str> {
    fn eval(&self, value: &str) -> bool {
        self.0 == value
    }

    fn describe(&self) -> String {
        format!("equal to {:?}", self.0)
    }
}

#[must_use]
pub const fn eq<T>(expected: T) -> Equals<T> {
    Equals(expected)
}

/// A predicate from a closure.
pub struct FromFn<F>(F, &'static str);

impl<T: ?Sized, F: Fn(&T) -> bool> Predicate<T> for FromFn<F> {
    fn eval(&self, value: &T) -> bool {
        (self.0)(value)
    }

    fn describe(&self) -> String {
        self.1.to_string()
    }
}

impl<F> fmt::Debug for FromFn<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FromFn({})", self.1)
    }
}

#[must_use]
pub const fn function<F>(description: &'static str, check: F) -> FromFn<F> {
    FromFn(check, description)
}

/// Predicates on strings.
pub mod str {
    use super::Predicate;
    use quench_regex::Regex;

    #[derive(Debug, Clone)]
    pub struct Contains(String);

    impl Predicate<str> for Contains {
        fn eval(&self, value: &str) -> bool {
            value.contains(&self.0)
        }

        fn describe(&self) -> String {
            format!("contains {:?}", self.0)
        }
    }

    #[derive(Debug, Clone)]
    pub struct StartsWith(String);

    impl Predicate<str> for StartsWith {
        fn eval(&self, value: &str) -> bool {
            value.starts_with(&self.0)
        }

        fn describe(&self) -> String {
            format!("starts with {:?}", self.0)
        }
    }

    #[derive(Debug, Clone)]
    pub struct EndsWith(String);

    impl Predicate<str> for EndsWith {
        fn eval(&self, value: &str) -> bool {
            value.ends_with(&self.0)
        }

        fn describe(&self) -> String {
            format!("ends with {:?}", self.0)
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub struct IsEmpty;

    impl Predicate<str> for IsEmpty {
        fn eval(&self, value: &str) -> bool {
            value.is_empty()
        }

        fn describe(&self) -> String {
            "is empty".to_string()
        }
    }

    #[derive(Debug, Clone)]
    pub struct IsMatch(Regex);

    impl Predicate<str> for IsMatch {
        fn eval(&self, value: &str) -> bool {
            self.0.is_match(value)
        }

        fn describe(&self) -> String {
            format!("matches /{}/", self.0.as_str())
        }
    }

    pub fn contains(needle: impl Into<String>) -> Contains {
        Contains(needle.into())
    }

    pub fn starts_with(prefix: impl Into<String>) -> StartsWith {
        StartsWith(prefix.into())
    }

    pub fn ends_with(suffix: impl Into<String>) -> EndsWith {
        EndsWith(suffix.into())
    }

    #[must_use]
    pub const fn is_empty() -> IsEmpty {
        IsEmpty
    }

    /// # Errors
    ///
    /// When `pattern` is not a valid regular expression.
    pub fn is_match(pattern: &str) -> Result<IsMatch, quench_regex::Error> {
        Regex::new(pattern).map(IsMatch)
    }
}

/// `use quench_test::predicate::prelude::*;` brings in [`Predicate`] and the `predicate` module name.
pub mod prelude {
    pub use super::Predicate;
    pub use crate::predicate;
}

#[cfg(test)]
mod tests {
    use super::prelude::*;

    #[test]
    fn strings() {
        assert!(predicate::str::contains("lo w").eval("hello world"));
        assert!(
            predicate::str::starts_with("he")
                .and(predicate::str::ends_with("ld"))
                .eval("hello world")
        );
        assert!(predicate::str::is_empty().eval(""));
        assert!(
            predicate::str::is_match("^h.*d$")
                .unwrap()
                .eval("hello world")
        );
        assert!(predicate::str::is_match("(").is_err());
        assert!(
            predicate::str::contains("x")
                .or(predicate::str::contains("o"))
                .eval("o")
        );
        assert!(predicate::str::contains("x").not().eval("o"));
        let p = predicate::str::contains("a").and(predicate::str::contains("b").not());
        assert_eq!(p.describe(), "(contains \"a\" and not contains \"b\")");
        assert_eq!(
            predicate::str::is_match("a+").unwrap().describe(),
            "matches /a+/"
        );
        assert_eq!(
            predicate::str::starts_with("a")
                .or(predicate::str::ends_with("b"))
                .describe(),
            "(starts with \"a\" or ends with \"b\")"
        );
        assert_eq!(predicate::str::is_empty().describe(), "is empty");
    }

    #[test]
    fn values_and_functions() {
        assert!(predicate::eq(3).eval(&3));
        assert!(!predicate::eq(3).eval(&4));
        assert!(predicate::eq("a").eval("a"));
        assert_eq!(predicate::eq(3).describe(), "equal to 3");
        assert!(predicate::always().eval("anything"));
        assert_eq!(Predicate::<str>::describe(&predicate::always()), "anything");
        let even = predicate::function("is even", |n: &i32| n % 2 == 0);
        assert!(even.eval(&4) && !even.eval(&3));
        assert_eq!(even.describe(), "is even");
        assert!(format!("{even:?}").contains("is even"));
    }
}
