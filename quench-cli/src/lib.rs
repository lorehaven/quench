#[cfg(feature = "repl")]
pub mod editor;
pub mod graphics;
#[cfg(all(unix, feature = "tui"))]
pub mod keys;
pub mod prelude;
#[cfg(feature = "repl")]
pub mod repl;
pub mod require;
pub mod style;
#[cfg(all(unix, feature = "tui"))]
pub mod term;
pub mod terminal;
#[cfg(all(unix, feature = "tui"))]
pub mod tty;

#[cfg(test)]
quench_test::main!();
