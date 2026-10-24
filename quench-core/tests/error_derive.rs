//! `#[derive(Error)]` as the rest of quench uses it.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use std::error::Error as StdError;
use std::io;

use quench_core::Error;

const MAX_BYTES: usize = 1024;

fn hint(found: char) -> String {
    format!(" (try {found}{found})")
}

#[derive(Debug, Error)]
enum Sample {
    #[error("plain message")]
    Unit,
    #[error("miss for key: {0}")]
    Tuple(String),
    #[error("two parts: {0} / {1:?}")]
    Pair(String, u8),
    #[error("`{path}` is {size} bytes, over the {MAX_BYTES}-byte limit")]
    Named { path: String, size: usize },
    #[error("dependency cycle between: {}", modules.join(", "))]
    BareArgs { modules: Vec<String> },
    #[error("dependency cycle detected among: {}", .0.join(" -> "))]
    DotTuple(Vec<String>),
    #[error("stages form a cycle: {}", .path.join(" -> "))]
    DotNamed { path: Vec<String> },
    #[error("unexpected character '{found}'{}", hint(*found))]
    Deref { found: char },
    #[error("module '{name}' defined twice: {first} and {second}", first = first.display(), second = second.display())]
    NamedArgs {
        name: String,
        first: std::path::PathBuf,
        second: std::path::PathBuf,
    },
    #[error("unknown module '{name}'{}", required_by.as_ref().map_or_else(String::new, |m| format!(" required by '{m}'")))]
    OptionalPart {
        name: String,
        required_by: Option<String>,
    },
    #[error("io failed")]
    WithSource(#[source] io::Error),
    #[error("while reading: {source}")]
    NamedSource { source: io::Error },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("{{literal}} braces and {0}")]
    Braces(u8),
}

fn text(e: &impl StdError) -> String {
    e.to_string()
}

#[quench_test::test]
fn display_formats() {
    assert_eq!(text(&Sample::Unit), "plain message");
    assert_eq!(text(&Sample::Tuple("k".into())), "miss for key: k");
    assert_eq!(text(&Sample::Pair("a".into(), 7)), "two parts: a / 7");
    assert_eq!(
        text(&Sample::Named {
            path: "/x".into(),
            size: 2000
        }),
        "`/x` is 2000 bytes, over the 1024-byte limit"
    );
    assert_eq!(
        text(&Sample::BareArgs {
            modules: vec!["a".into(), "b".into()]
        }),
        "dependency cycle between: a, b"
    );
    assert_eq!(
        text(&Sample::DotTuple(vec!["a".into(), "b".into()])),
        "dependency cycle detected among: a -> b"
    );
    assert_eq!(
        text(&Sample::DotNamed {
            path: vec!["x".into(), "y".into()]
        }),
        "stages form a cycle: x -> y"
    );
    assert_eq!(
        text(&Sample::Deref { found: '?' }),
        "unexpected character '?' (try ??)"
    );
    assert_eq!(
        text(&Sample::NamedArgs {
            name: "m".into(),
            first: "a.toml".into(),
            second: "b.toml".into()
        }),
        "module 'm' defined twice: a.toml and b.toml"
    );
    assert_eq!(
        text(&Sample::OptionalPart {
            name: "n".into(),
            required_by: None
        }),
        "unknown module 'n'"
    );
    assert_eq!(
        text(&Sample::OptionalPart {
            name: "n".into(),
            required_by: Some("p".into())
        }),
        "unknown module 'n' required by 'p'"
    );
    assert_eq!(text(&Sample::Braces(3)), "{literal} braces and 3");
}

#[quench_test::test]
fn sources() {
    let source = |e: &Sample| e.source().map(ToString::to_string);
    assert_eq!(source(&Sample::Unit), None);
    assert_eq!(source(&Sample::Tuple("k".into())), None);
    assert_eq!(
        source(&Sample::WithSource(io::Error::other("disk"))),
        Some("disk".into())
    );
    assert_eq!(
        source(&Sample::NamedSource {
            source: io::Error::other("net")
        }),
        Some("net".into())
    );
    // transparent: Display and source both come from the inner error
    let transparent = Sample::Io(io::Error::other("inner"));
    assert_eq!(transparent.to_string(), "inner");
    assert!(transparent.source().is_none());
}

#[quench_test::test]
fn from_enables_question_mark() {
    fn read() -> Result<(), Sample> {
        Err(io::Error::new(io::ErrorKind::NotFound, "gone"))?;
        Ok(())
    }
    let e = read().unwrap_err();
    assert!(matches!(e, Sample::Io(_)));
    assert_eq!(e.to_string(), "gone");
}

#[derive(Debug, Error)]
#[error("struct error {code}: {message}")]
struct Coded {
    code: u16,
    message: String,
}

#[derive(Debug, Error)]
#[error("wrapped: {0}")]
struct Wrapped(#[source] Coded);

#[derive(Debug, Error)]
#[error("fixed")]
struct Fixed;

#[derive(Debug, Error)]
#[error(transparent)]
struct Passthrough(#[from] Coded);

#[quench_test::test]
fn structs() {
    let coded = Coded {
        code: 404,
        message: "nope".into(),
    };
    assert_eq!(coded.to_string(), "struct error 404: nope");
    let wrapped = Wrapped(coded);
    assert_eq!(wrapped.to_string(), "wrapped: struct error 404: nope");
    assert_eq!(
        wrapped.source().unwrap().to_string(),
        "struct error 404: nope"
    );
    assert_eq!(Fixed.to_string(), "fixed");
    let pass: Passthrough = Coded {
        code: 1,
        message: "m".into(),
    }
    .into();
    assert_eq!(pass.to_string(), "struct error 1: m");
}

#[derive(Debug, Error)]
#[error("generic {0:?}")]
struct Generic<T: std::fmt::Debug>(T);

#[derive(Debug, Error)]
enum Never {}

#[quench_test::test]
fn generics_and_empty_enums() {
    assert_eq!(Generic(vec![1, 2]).to_string(), "generic [1, 2]");
    let _: Option<Never> = None;
}

#[quench_test::test]
fn works_as_a_boxed_error_and_with_the_catch_all() {
    let boxed: Box<dyn StdError + Send + Sync> = Box::new(Sample::Tuple("x".into()));
    assert_eq!(boxed.to_string(), "miss for key: x");
    let any: quench_core::error::Error = Sample::Unit.into();
    assert_eq!(any.to_string(), "plain message");
    assert!(any.downcast_ref::<Sample>().is_some());
}

#[derive(Debug, Error)]
enum WithPaths {
    #[error("cannot read {path}: {source}")]
    Read {
        path: std::path::PathBuf,
        source: io::Error,
    },
    #[error("bad file {0}")]
    Bad(std::path::PathBuf),
    #[error("twice: {first} and {second}", first = first.display(), second = second.display())]
    Twice {
        first: std::path::PathBuf,
        second: std::path::PathBuf,
    },
    #[error("debug form: {path:?}")]
    Debugged { path: std::path::PathBuf },
}

#[quench_test::test]
fn path_fields_display_like_thiserror() {
    let e = WithPaths::Read {
        path: "/etc/x.toml".into(),
        source: io::Error::other("denied"),
    };
    assert_eq!(e.to_string(), "cannot read /etc/x.toml: denied");
    assert!(e.source().is_some());
    assert_eq!(WithPaths::Bad("a/b".into()).to_string(), "bad file a/b");
    assert_eq!(
        WithPaths::Twice {
            first: "a".into(),
            second: "b".into()
        }
        .to_string(),
        "twice: a and b"
    );
    assert_eq!(
        WithPaths::Debugged { path: "p".into() }.to_string(),
        "debug form: \"p\""
    );
}

quench_test::main!();
