//! A catch-all error type for application code (`anyhow`'s role).
//!
//! Library crates define their own error enums; binaries and glue code that
//! only need "something went wrong, here is the story" use [`Error`], add
//! context as the error travels up, and print the whole chain at the top.

use std::error::Error as StdError;
use std::fmt::{self, Debug, Display};

/// `Result` with [`Error`] as the default error type.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Any error, plus the context added on its way up.
pub struct Error {
    /// Innermost first; displayed outermost (last) first.
    contexts: Vec<String>,
    root: Box<dyn StdError + Send + Sync + 'static>,
}

struct Message(String);

impl Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl StdError for Message {}

impl Error {
    /// An error from a message alone.
    pub fn msg(message: impl Display) -> Self {
        Self {
            contexts: Vec::new(),
            root: Box::new(Message(message.to_string())),
        }
    }

    /// Wraps a concrete error, keeping it for [`downcast_ref`](Self::downcast_ref).
    pub fn new(error: impl StdError + Send + Sync + 'static) -> Self {
        Self {
            contexts: Vec::new(),
            root: Box::new(error),
        }
    }

    /// Adds a layer of context in front of the message.
    #[must_use]
    pub fn context(mut self, context: impl Display) -> Self {
        self.contexts.push(context.to_string());
        self
    }

    /// Every message from the outermost context down to the root cause and
    /// whatever *it* was caused by.
    #[must_use]
    pub fn chain(&self) -> Vec<String> {
        let mut out: Vec<String> = self.contexts.iter().rev().cloned().collect();
        out.push(self.root.to_string());
        let mut source = self.root.source();
        while let Some(cause) = source {
            out.push(cause.to_string());
            source = cause.source();
        }
        out
    }

    /// The error this one started as.
    #[must_use]
    pub fn root_cause(&self) -> &(dyn StdError + Send + Sync + 'static) {
        &*self.root
    }

    /// The root cause as `E`, if that is what it is.
    #[must_use]
    pub fn downcast_ref<E: StdError + 'static>(&self) -> Option<&E> {
        self.root.downcast_ref::<E>()
    }
}

impl Display for Error {
    /// The outermost message; `{:#}` joins the whole chain with `: `.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            return f.write_str(&self.chain().join(": "));
        }
        match self.contexts.last() {
            Some(context) => f.write_str(context),
            None => Display::fmt(&self.root, f),
        }
    }
}

impl Debug for Error {
    /// The message, then a numbered `Caused by:` list (what `main` prints
    /// when it returns `Err`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let chain = self.chain();
        f.write_str(&chain[0])?;
        if chain.len() > 1 {
            f.write_str("\n\nCaused by:")?;
            for (i, cause) in chain[1..].iter().enumerate() {
                write!(f, "\n    {i}: {cause}")?;
            }
        }
        Ok(())
    }
}

impl<E: StdError + Send + Sync + 'static> From<E> for Error {
    fn from(error: E) -> Self {
        Self::new(error)
    }
}

mod private {
    use super::{Error, StdError};

    pub trait IntoError {
        fn into_error(self) -> Error;
    }

    impl<E: StdError + Send + Sync + 'static> IntoError for E {
        fn into_error(self) -> Error {
            Error::new(self)
        }
    }

    impl IntoError for Error {
        fn into_error(self) -> Error {
            self
        }
    }
}

/// `.context(..)` / `.with_context(..)` on `Result` and `Option`.
pub trait Context<T> {
    /// Adds `context` to the error; for `Option`, turns `None` into an error
    /// carrying it.
    fn context(self, context: impl Display) -> Result<T>;

    /// Like [`context`](Self::context), built only on the failure path.
    fn with_context<C: Display>(self, context: impl FnOnce() -> C) -> Result<T>;
}

impl<T, E: private::IntoError> Context<T> for std::result::Result<T, E> {
    fn context(self, context: impl Display) -> Result<T> {
        self.map_err(|e| e.into_error().context(context))
    }

    fn with_context<C: Display>(self, context: impl FnOnce() -> C) -> Result<T> {
        self.map_err(|e| e.into_error().context(context()))
    }
}

impl<T> Context<T> for Option<T> {
    fn context(self, context: impl Display) -> Result<T> {
        self.ok_or_else(|| Error::msg(context))
    }

    fn with_context<C: Display>(self, context: impl FnOnce() -> C) -> Result<T> {
        self.ok_or_else(|| Error::msg(context()))
    }
}

/// Builds an [`Error`] from a format string (or any displayable value).
#[macro_export]
macro_rules! err {
    ($message:literal $(,)?) => {
        $crate::error::Error::msg(format!($message))
    };
    ($fmt:literal, $($arg:tt)*) => {
        $crate::error::Error::msg(format!($fmt, $($arg)*))
    };
    ($error:expr $(,)?) => {
        $crate::error::Error::msg($error)
    };
}

/// Returns early with an [`Error`].
#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return ::std::result::Result::Err($crate::err!($($arg)*))
    };
}

/// Returns early with an [`Error`] unless the condition holds.
#[macro_export]
macro_rules! ensure {
    ($condition:expr $(,)?) => {
        if !$condition {
            $crate::bail!(concat!("condition failed: `", stringify!($condition), "`"));
        }
    };
    ($condition:expr, $($arg:tt)*) => {
        if !$condition {
            $crate::bail!($($arg)*);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    fn failing_io() -> std::result::Result<(), io::Error> {
        Err(io::Error::new(io::ErrorKind::NotFound, "no such file"))
    }

    #[test]
    fn message_errors() {
        let e = Error::msg("boom");
        assert_eq!(e.to_string(), "boom");
        assert_eq!(format!("{e:?}"), "boom");
        assert_eq!(e.chain(), ["boom"]);
    }

    #[test]
    fn wraps_std_errors_and_downcasts() {
        let e: Error = failing_io().unwrap_err().into();
        assert_eq!(e.to_string(), "no such file");
        let io = e.downcast_ref::<io::Error>().expect("io error");
        assert_eq!(io.kind(), io::ErrorKind::NotFound);
        assert!(e.downcast_ref::<std::fmt::Error>().is_none());
        assert_eq!(e.root_cause().to_string(), "no such file");
    }

    #[test]
    fn context_stacks_outermost_first() {
        let e = failing_io()
            .context("reading config")
            .context("starting up")
            .unwrap_err();
        assert_eq!(e.to_string(), "starting up");
        assert_eq!(
            format!("{e:#}"),
            "starting up: reading config: no such file"
        );
        assert_eq!(e.chain(), ["starting up", "reading config", "no such file"]);
        assert_eq!(
            format!("{e:?}"),
            "starting up\n\nCaused by:\n    0: reading config\n    1: no such file"
        );
    }

    #[test]
    fn context_on_our_own_error_and_lazily() {
        let calls = std::cell::Cell::new(0);
        let ok: Result<u8> = Ok::<u8, Error>(1).with_context(|| {
            calls.set(calls.get() + 1);
            "unused"
        });
        assert_eq!(ok.unwrap(), 1);
        assert_eq!(calls.get(), 0);

        let e = Err::<u8, _>(Error::msg("inner"))
            .with_context(|| format!("n={}", 7))
            .unwrap_err();
        assert_eq!(format!("{e:#}"), "n=7: inner");
    }

    #[test]
    fn option_context() {
        assert_eq!(Some(3).context("missing").unwrap(), 3);
        assert_eq!(
            None::<u8>.context("missing").unwrap_err().to_string(),
            "missing"
        );
        assert_eq!(
            None::<u8>.with_context(|| "lazy").unwrap_err().to_string(),
            "lazy"
        );
        assert_eq!(Some(1).with_context(|| "lazy").unwrap(), 1);
    }

    #[test]
    fn includes_the_sources_of_wrapped_errors() {
        #[derive(Debug)]
        struct Outer(io::Error);
        impl Display for Outer {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("outer failed")
            }
        }
        impl StdError for Outer {
            fn source(&self) -> Option<&(dyn StdError + 'static)> {
                Some(&self.0)
            }
        }
        let e = Error::new(Outer(failing_io().unwrap_err())).context("top");
        assert_eq!(e.chain(), ["top", "outer failed", "no such file"]);
    }

    #[test]
    fn macros() {
        fn plain() -> Result<()> {
            bail!("plain");
        }
        fn formatted(n: u8) -> Result<()> {
            bail!("n was {n}, then {}", n + 1);
        }
        fn expression() -> Result<()> {
            bail!(io::Error::other("from value"));
        }
        fn checked(ok: bool) -> Result<()> {
            ensure!(ok);
            ensure!(ok, "never shown {}", 1);
            Ok(())
        }
        fn checked_msg(ok: bool) -> Result<()> {
            ensure!(ok, "needs {}", "ok");
            Ok(())
        }
        assert_eq!(plain().unwrap_err().to_string(), "plain");
        assert_eq!(formatted(1).unwrap_err().to_string(), "n was 1, then 2");
        assert_eq!(expression().unwrap_err().to_string(), "from value");
        assert!(checked(true).is_ok());
        assert_eq!(
            checked(false).unwrap_err().to_string(),
            "condition failed: `ok`"
        );
        assert_eq!(checked_msg(false).unwrap_err().to_string(), "needs ok");
        assert_eq!(err!("x{}", 1).to_string(), "x1");
        assert_eq!(err!("x",).to_string(), "x");
        assert_eq!(err!(String::from("owned")).to_string(), "owned");
    }

    #[test]
    fn question_mark_converts() {
        fn run() -> Result<()> {
            failing_io()?;
            Ok(())
        }
        assert_eq!(run().unwrap_err().to_string(), "no such file");
    }

    #[test]
    fn error_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<Error>();
    }
}
