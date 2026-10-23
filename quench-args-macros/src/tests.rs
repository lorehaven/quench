use proc_macro2::TokenStream;
use syn::{DeriveInput, parse_quote};

use crate::{attrs, expand};

fn render(result: syn::Result<TokenStream>) -> String {
    match result {
        Ok(tokens) => tokens
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        Err(error) => format!("ERROR: {error}"),
    }
}

fn parser(input: &DeriveInput) -> String {
    render(expand::expand_struct(input, true))
}

fn args(input: &DeriveInput) -> String {
    render(expand::expand_struct(input, false))
}

fn subcommand(input: &DeriveInput) -> String {
    render(expand::expand_subcommand(input))
}

fn value_enum(input: &DeriveInput) -> String {
    render(expand::expand_value_enum(input))
}

#[test]
fn a_parser_gets_all_four_impls_and_its_attributes() {
    let out = parser(&parse_quote! {
        /// The tool.
        ///
        /// Longer text.
        #[command(name = "tool", version, help_template = "x", disable_help_subcommand = true, alias = "t", visible_alias = "tt")]
        struct Cli {
            /// Verbose output.
            #[arg(long, short)]
            verbose: bool,
            #[command(subcommand)]
            cmd: Option<Cmd>,
        }
    });
    for needle in [
        "impl :: quench_args :: Args for Cli",
        "impl :: quench_args :: FromMatches for Cli",
        "impl :: quench_args :: CommandFactory for Cli",
        "impl :: quench_args :: Parser for Cli",
        "Command :: new (\"tool\")",
        "cmd . about (\"The tool\")",
        "cmd . long_about (\"The tool.\\n\\nLonger text.\")",
        "cmd . version (:: std :: env ! (\"CARGO_PKG_VERSION\"))",
        "cmd . help_template (\"x\")",
        "cmd . disable_help_subcommand (true)",
        "cmd . alias (\"t\")",
        "cmd . visible_alias (\"tt\")",
        "arg . long (\"verbose\")",
        "arg . short ('v')",
        "arg . help (\"Verbose output\")",
        "augment_subcommands (cmd)",
        "m . flag (\"verbose\")",
    ] {
        assert!(out.contains(needle), "missing `{needle}` in {out}");
    }
    assert!(
        !out.contains("subcommand_required"),
        "an optional subcommand is not required"
    );
}

#[test]
fn the_default_command_name_comes_from_the_crate() {
    let out = parser(&parse_quote! { struct Cli {} });
    assert!(
        out.contains(":: std :: env ! (\"CARGO_PKG_NAME\")"),
        "{out}"
    );
}

#[test]
fn field_shapes_and_attributes_become_arg_calls() {
    let out = args(&parse_quote! {
        struct A {
            plain: String,
            #[arg(long = "wait-for", short = 'w', value_name = "SECS", default_value = "10", global = true, conflicts_with = "other", hide = true)]
            wait: String,
            #[arg(long, default_value_t = 5)]
            n: u32,
            #[arg(long, value_enum, default_value_t = Mode::A)]
            mode: Mode,
            #[arg(long, value_enum)]
            maybe: Option<Mode>,
            #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
            rest: Vec<String>,
            #[arg(required = true)]
            need: Vec<String>,
            #[arg(long, help = HELP, long_help = LONG)]
            documented: Option<String>,
            #[arg(long)]
            r#type: Option<String>,
            #[arg(short)]
            q: Option<String>,
        }
    });
    for needle in [
        "Arg :: new (\"plain\")",
        "arg . required (true)",
        "arg . long (\"wait-for\")",
        "arg . short ('w')",
        "arg . value_name (\"SECS\")",
        "arg . default_value (\"10\")",
        "arg . global (true)",
        "arg . conflicts_with (\"other\")",
        "arg . hide (true)",
        "ToString :: to_string (& (5))",
        "ValueEnum :: to_possible_value (& (Mode :: A))",
        "possible_values :: < Mode > ()",
        "value_enum_arg :: < Mode > (\"--mode <MODE>\"",
        "arg . trailing_var_arg (true)",
        "arg . allow_hyphen_values (true)",
        "arg . help (HELP)",
        "arg . long_help (LONG)",
        "Arg :: new (\"type\")",
        "arg . long (\"type\")",
        "Kind :: Many",
        "Kind :: Single",
        "m . get_required (\"plain\") ?",
        "FromArg > :: from_arg (\"<PLAIN>\"",
        "from_arg (\"-q <Q>\"",
        "\"--wait-for <SECS>\"",
    ] {
        assert!(out.contains(needle), "missing `{needle}` in {out}");
    }
}

#[test]
fn flatten_and_subcommand_fields_delegate() {
    let out = args(&parse_quote! {
        struct A {
            #[command(flatten)]
            common: Common,
            #[command(subcommand)]
            cmd: Cmd,
        }
    });
    assert!(
        out.contains("< Common as :: quench_args :: Args > :: augment_args (cmd)"),
        "{out}"
    );
    assert!(
        out.contains("< Cmd as :: quench_args :: Subcommand > :: augment_subcommands (cmd)"),
        "{out}"
    );
    assert!(
        out.contains("subcommand_required (true)"),
        "a plain subcommand field is required: {out}"
    );
}

#[test]
fn subcommand_enums_cover_every_variant_shape() {
    let out = subcommand(&parse_quote! {
        enum Cmd {
            /// Does a thing.
            #[command(visible_aliases = ["t", "th"], aliases = ["x"], hide = true)]
            DoThing { #[arg(long)] now: bool },
            HTTPServer,
            #[command(name = "custom")]
            Named(Inner),
            #[command(subcommand)]
            Nested(Deeper),
            Group { #[command(subcommand)] cmd: Deeper },
        }
    });
    for needle in [
        "Command :: new (\"do-thing\")",
        "cmd . about (\"Does a thing\")",
        "cmd . visible_alias (\"t\")",
        "cmd . visible_alias (\"th\")",
        "cmd . alias (\"x\")",
        "cmd . hide (true)",
        "Command :: new (\"http-server\")",
        "Command :: new (\"custom\")",
        "< Inner as :: quench_args :: Args > :: augment_args (cmd)",
        "< Deeper as :: quench_args :: Subcommand > :: augment_subcommands (cmd) . subcommand_required (true)",
    ] {
        assert!(out.contains(needle), "missing `{needle}` in {out}");
    }
    assert!(
        out.contains(
            "Some ((\"do-thing\" , m)) => :: std :: result :: Result :: Ok (Self :: DoThing"
        ),
        "{out}"
    );
    assert!(out.contains("missing_subcommand"), "{out}");
}

#[test]
fn value_enums_name_their_variants() {
    let out = value_enum(&parse_quote! {
        enum Mode {
            /// Fast mode.
            Fast,
            #[value(name = "slo", help = "Slow")]
            SlowMode,
            #[value(skip)]
            Secret,
        }
    });
    assert!(
        out.contains("PossibleValue :: new (\"fast\") . help (\"Fast mode\")"),
        "{out}"
    );
    assert!(
        out.contains("PossibleValue :: new (\"slo\") . help (\"Slow\")"),
        "{out}"
    );
    assert!(
        out.contains("Self :: Secret => :: std :: option :: Option :: None"),
        "{out}"
    );
    assert!(
        out.contains("& [Self :: Fast , Self :: SlowMode]"),
        "skipped variants are not listed: {out}"
    );
}

#[test]
fn mistakes_are_compile_errors_that_say_what_is_supported() {
    let e = |out: String| out.trim_start_matches("ERROR: ").to_string();
    assert!(
        e(args(
            &parse_quote! { struct A { #[arg(long, nope = 1)] x: u8 } }
        ))
        .contains("supported: `long`")
    );
    assert!(
        e(parser(&parse_quote! { #[command(nope)] struct A {} })).contains("supported: `name`")
    );
    assert!(
        e(value_enum(&parse_quote! { enum E { #[value(nope)] A } })).contains("supported: `name`")
    );
    assert!(e(parser(&parse_quote! { enum E { A } })).contains("derive on structs"));
    assert!(e(subcommand(&parse_quote! { struct S {} })).contains("derives on enums"));
    assert!(e(value_enum(&parse_quote! { struct S {} })).contains("derives on enums"));
    assert!(e(value_enum(&parse_quote! { enum E { A(u8) } })).contains("carry no data"));
    assert!(e(args(&parse_quote! { struct S(u8); })).contains("tuple fields"));
    assert!(
        e(args(&parse_quote! { struct S { flag: bool } })).contains("give it `long` or `short`")
    );
    assert!(e(subcommand(&parse_quote! { enum E { A(u8, u8) } })).contains("exactly one field"));
    assert!(
        e(subcommand(
            &parse_quote! { enum E { #[command(version)] A } }
        ))
        .contains("top-level")
    );
}

#[test]
fn doc_comments_split_into_a_short_and_a_long_text() {
    let doc = |input: DeriveInput| attrs::doc(&input.attrs);
    let none = doc(parse_quote! { struct S; });
    assert!(none.short.is_none() && none.long.is_none());

    let one = doc(parse_quote! {
        /// Render manifests
        /// into place.
        struct S;
    });
    assert_eq!(one.short.as_deref(), Some("Render manifests into place"));
    assert!(one.long.is_none());

    let two = doc(parse_quote! {
        /// First sentence. Second.
        ///
        /// Another paragraph
        /// over two lines.
        struct S;
    });
    assert_eq!(two.short.as_deref(), Some("First sentence. Second."));
    assert_eq!(
        two.long.as_deref(),
        Some("First sentence. Second.\n\nAnother paragraph over two lines.")
    );

    let ellipsis = doc(parse_quote! {
        /// Wait...
        struct S;
    });
    assert_eq!(ellipsis.short.as_deref(), Some("Wait..."));
}

#[test]
fn names_are_kebab_cased() {
    for (from, to) in [
        ("SetBranch", "set-branch"),
        ("HTTPServer", "http-server"),
        ("dry_run", "dry-run"),
        ("r#use", "use"),
        ("List", "list"),
        ("V2Api", "v2-api"),
    ] {
        assert_eq!(attrs::kebab(from), to, "{from}");
    }
}

#[test]
fn long_about_none_suppresses_the_doc_text() {
    let out = parser(&parse_quote! {
        /// Short.
        ///
        /// Long.
        #[command(about = "Explicit", long_about = None)]
        struct Cli {}
    });
    assert!(out.contains("cmd . about (\"Explicit\")"), "{out}");
    assert!(!out.contains("long_about"), "{out}");
    let explicit = parser(&parse_quote! {
        #[command(long_about = "Details", version = "2")]
        struct Cli {}
    });
    assert!(
        explicit.contains("cmd . long_about (\"Details\")")
            && explicit.contains("cmd . version (\"2\")"),
        "{explicit}"
    );
}
