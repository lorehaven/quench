---
title: Command runner and predicates
parent: quench-test
grand_parent: Crates
nav_order: 3
---

Modules `cmd` (a replacement for `assert_cmd`) and `predicate` (for `predicates`).

## `cmd`

```rust,no_run
use quench_test::cmd::Command;
use quench_test::predicate::prelude::*;

Command::cargo_bin("mytool")
    .unwrap()
    .args(["--version"])
    .assert()
    .success()
    .stdout(predicate::str::contains("mytool"));
```

### `struct CargoBinError`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoBinError(/* private String */);   // Display = the message; std::error::Error
```

### `struct Command`

`#[derive(Debug, Clone)]`. A program with arguments, environment, working directory, standard input and an optional timeout. Setters take `&mut self` and return `&mut Self`, so a command is built on a `let mut` or as a temporary chain.

```rust
impl Command {
    #[must_use] pub fn new(program: impl AsRef<OsStr>) -> Self
    pub fn cargo_bin(name: &str) -> Result<Self, CargoBinError>
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self
    pub fn args<I, S>(&mut self, args: I) -> &mut Self  where I: IntoIterator<Item = S>, S: AsRef<OsStr>
    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self
    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    pub fn env_remove(&mut self, key: impl AsRef<OsStr>) -> &mut Self
    pub fn current_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self
    pub fn write_stdin(&mut self, data: impl Into<Vec<u8>>) -> &mut Self
    pub const fn timeout(&mut self, timeout: Duration) -> &mut Self
    pub fn output(&mut self) -> std::io::Result<Output>
    pub fn assert(&mut self) -> Assert
}
```

| Method | Behaviour |
|---|---|
| `new(program)` | any program name or path (looked up on `PATH` by the OS) |
| `cargo_bin(name)` | the binary target `name` of the package under test: the path in `CARGO_BIN_EXE_<name>` if set and a file; else `<dir of the test executable, one level up when it is a `deps` directory>/<name>` (with `.exe` on Windows) if it is a file. Error text: ``no binary `name` next to <test executable>`` |
| `arg`, `args` | append arguments |
| `env`, `envs` | set variables on top of the inherited environment (applied in call order) |
| `env_remove` | remove a variable from the child's environment |
| `current_dir` | working directory |
| `write_stdin(data)` | bytes written to the child's standard input from a helper thread, then the pipe is closed. Without this call, standard input is `/dev/null` (closed immediately). |
| `timeout(d)` | kill the child (and fail) if it runs longer |
| `output()` | run to completion: stdout and stderr are read on helper threads, the child is polled every 2 ms. Error: cannot start, I/O failure, or `ErrorKind::TimedOut` with the message `` `prog args` did not finish within <duration> `` (the child is killed and reaped first). |
| `assert()` | `output()` then an `Assert`; **panics** with ``could not run `prog args`: <error>`` if `output()` fails |

### `struct Output`

```rust
#[derive(Debug, Clone)]
pub struct Output { pub status: ExitStatus, pub stdout: Vec<u8>, pub stderr: Vec<u8> }
```

### `trait OutputPredicate`

```rust
pub trait OutputPredicate {
    fn check(&self, bytes: &[u8]) -> bool;
    fn describe(&self) -> String;
}
```

What `Assert::stdout` and `Assert::stderr` accept. Implemented for: every `Predicate<str>` (the bytes are decoded lossily as UTF-8), `&str` and `String` (**exact** equality with the whole output), and `Exactly`.

### `struct Exactly`

```rust
#[derive(Debug, Clone)]
pub struct Exactly(pub Vec<u8>);   // the output must be exactly these bytes
```

### `struct Assert`

`#[derive(Debug, Clone)]`. A finished program with chained assertions that consume and return `self`; a failure **panics** with `assertion failed for `<prog args>`: <expectation>`, then `status:`, `stdout:` and `stderr:` with the full output.

```rust
impl Assert {
    #[must_use] pub const fn get_output(&self) -> &Output
    pub fn success(self) -> Self                 // exit status 0               ("expected success")
    pub fn failure(self) -> Self                 // non-zero or killed          ("expected failure")
    pub fn code(self, expected: i32) -> Self     // exactly this code           ("expected exit code N")
    pub fn stdout(self, predicate: impl OutputPredicate) -> Self   // "expected stdout to be <description>"
    pub fn stderr(self, predicate: impl OutputPredicate) -> Self
}
```

Example (verified): `Command::new("sh").args(["-c", "echo hi; echo err >&2; exit 3"]).assert().code(3).failure().stdout("hi\n").stderr(predicate::str::contains("err"))`.

## `predicate`

### `trait Predicate<T: ?Sized>`

```rust
pub trait Predicate<T: ?Sized> {
    fn eval(&self, value: &T) -> bool;
    fn describe(&self) -> String;
    fn and<P: Predicate<T>>(self, other: P) -> And<Self, P> where Self: Sized
    fn or<P: Predicate<T>>(self, other: P) -> Or<Self, P> where Self: Sized
    fn not(self) -> Not<Self> where Self: Sized
}
```

Combinators: `And<A, B>`, `Or<A, B>`, `Not<A>` (public tuple-less structs with private fields); `describe` of a combination is `(A and B)`, `(A or B)`, `not A`.

### Functions and types

| Item | Behaviour |
|---|---|
| `struct Always` / `const fn always() -> Always` | true for any value; describes as `anything` |
| `struct Equals<T>` / `const fn eq<T>(expected: T) -> Equals<T>` | equality (`T: PartialEq + Debug`); also `Predicate<str>` for `Equals<&str>`; describes `equal to {expected:?}` |
| `struct FromFn<F>` / `const fn function<F>(description: &'static str, check: F) -> FromFn<F>` | a predicate from a closure `Fn(&T) -> bool`; its description is the string; `Debug` prints `FromFn(description)` |

### `predicate::str`

```rust
pub struct Contains; pub struct StartsWith; pub struct EndsWith; pub struct IsEmpty; pub struct IsMatch;  // all Predicate<str>
pub fn contains(needle: impl Into<String>) -> Contains          // describes: contains "x"
pub fn starts_with(prefix: impl Into<String>) -> StartsWith     // starts with "x"
pub fn ends_with(suffix: impl Into<String>) -> EndsWith         // ends with "x"
#[must_use] pub const fn is_empty() -> IsEmpty                  // is empty
pub fn is_match(pattern: &str) -> Result<IsMatch, quench_regex::Error>   // matches /pattern/
```

`is_match` uses [quench-regex](../quench-regex/index.md) and is unanchored; it returns the regex error for a bad pattern.

### `predicate::prelude`

```rust
pub use super::Predicate;
pub use crate::predicate;
```

`use quench_test::predicate::prelude::*;` brings the trait (for `.and`, `.not`, `.eval`) and the module name `predicate` into scope, so `predicate::str::contains("x")` works as it does with the `predicates` crate.
