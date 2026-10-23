//! Derive macros re-exported by `quench-args`. Use them through that crate.
//!
//! `#[derive(Parser)]` and `#[derive(Args)]` on structs, `#[derive(Subcommand)]` on enums of subcommands and
//! `#[derive(ValueEnum)]` on fieldless enums, with the attributes `#[command(..)]`, `#[arg(..)]` and
//! `#[value(..)]` named as clap names them. Doc comments become help text.

mod attrs;
mod expand;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse2};

fn run(
    input: TokenStream,
    f: impl FnOnce(&DeriveInput) -> syn::Result<proc_macro2::TokenStream>,
) -> TokenStream {
    parse2::<DeriveInput>(input.into())
        .and_then(|input| f(&input))
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// The whole command line: arguments and (optionally) subcommands.
#[proc_macro_derive(Parser, attributes(command, arg))]
pub fn derive_parser(input: TokenStream) -> TokenStream {
    run(input, |i| expand::expand_struct(i, true))
}

/// A reusable group of arguments.
#[proc_macro_derive(Args, attributes(command, arg))]
pub fn derive_args(input: TokenStream) -> TokenStream {
    run(input, |i| expand::expand_struct(i, false))
}

/// An enum of subcommands.
#[proc_macro_derive(Subcommand, attributes(command, arg))]
pub fn derive_subcommand(input: TokenStream) -> TokenStream {
    run(input, expand::expand_subcommand)
}

/// An enum whose variants are the accepted spellings of a value.
#[proc_macro_derive(ValueEnum, attributes(value))]
pub fn derive_value_enum(input: TokenStream) -> TokenStream {
    run(input, expand::expand_value_enum)
}

#[cfg(test)]
mod tests;
