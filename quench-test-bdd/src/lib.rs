//! A light Gherkin test runtime.
//!
//! Write feature files, give their steps Rust functions with [`given`], [`when`] and [`then`], and run them
//! with a [`Runner`]. Scenarios run concurrently (those tagged `@serial` one by one, afterwards), a failed
//! step skips the rest of its scenario, and the results are printed and written as JSON and HTML reports
//! through [`quench_test::report`].
//!
//! ```ignore
//! use quench_test_bdd::{World, given, then, when};
//!
//! #[derive(World, Default)]
//! struct Calc { total: i64 }
//!
//! #[given("a total of {int}")]
//! fn start(world: &mut Calc, n: i64) { world.total = n; }
//!
//! #[when("I add {int}")]
//! fn add(world: &mut Calc, n: i64) { world.total += n; }
//!
//! #[then("the total is {int}")]
//! fn check(world: &mut Calc, n: i64) { assert_eq!(world.total, n); }
//!
//! #[tokio::main]
//! async fn main() {
//!     let summary = quench_test_bdd::Runner::<Calc>::new().run("tests/features").await;
//!     std::process::exit(i32::from(summary.failed()));
//! }
//! ```

use std::future::Future;
use std::pin::Pin;

pub mod expression;
pub mod gherkin;
mod options;
mod runner;
pub mod step;
pub mod tags;

pub use gherkin::{Feature, Rule, Scenario, Step, Table};
pub use options::Options;
#[doc(hidden)]
pub use quench_core as __core;
pub use quench_test_bdd_macros::{World, given, then, when};
pub use runner::{Runner, Stats, Summary};

/// A boxed future, as step functions return.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The registry the step attributes add to.
pub static STEPS: quench_core::registry::Registry<step::Definition> =
    quench_core::registry::Registry::new();

/// The state of one scenario, created fresh for each. Usually derived (`#[derive(World)]`, with
/// `#[world(init = path)]` for an `async fn() -> Self`, else `Default`).
pub trait World: Sized + Send + 'static {
    /// A new world.
    fn new() -> BoxFuture<'static, Self>;
}
