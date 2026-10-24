#![allow(dead_code)]

use super::*;

#[derive(Parser, Debug)]
#[command(
    name = "riv",
    version = "1.2.3",
    about = "The riveter",
    disable_help_subcommand = false
)]
struct Cli {
    /// Environment to act on.
    ///
    /// A longer explanation that only `--help` shows.
    #[arg(long, short = 'e', global = true, value_name = "ENV")]
    env: Option<String>,

    #[arg(long, global = true, conflicts_with = "quiet")]
    verbose: bool,

    #[arg(long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Manage environments
    Env {
        #[command(subcommand)]
        cmd: EnvCmd,
    },
    /// List the resources
    #[command(visible_alias = "ls")]
    List {
        #[arg(long, value_enum, default_value_t = Scope::All)]
        scope: Scope,
        #[arg(value_name = "TARGET")]
        targets: Vec<String>,
    },
    /// Apply things.
    #[command(visible_aliases = ["a", "ap"], alias = "hidden-alias")]
    Apply {
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 300)]
        timeout: u64,
        #[arg(long = "wait-for", short = 'w', default_value = "10")]
        wait: String,
        #[arg(short, long)]
        force: bool,
        #[arg(long)]
        r#use: bool,
        #[arg(short = 'n')]
        count: Option<u32>,
        #[arg(long)]
        label: Vec<String>,
    },
    /// Run a program with its own flags
    Test {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Needs a name
    Get {
        name: String,
        #[arg(required = true, value_name = "KEY")]
        keys: Vec<String>,
    },
    SetBranch(BranchArgs),
    #[command(name = "renamed")]
    Original,
    #[command(subcommand)]
    Nested(Nest),
}

#[derive(Subcommand, Debug)]
enum Nest {
    Inner,
}

#[derive(Subcommand, Debug)]
enum EnvCmd {
    List,
    Set { env: String },
}

#[derive(Args, Debug)]
struct BranchArgs {
    /// Owner and name
    repo: String,
    branch: Option<String>,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    All,
    /// Only the mutable ones
    Mutable,
    #[value(name = "imm")]
    Immutable,
    #[value(skip)]
    Hidden,
}

fn parse(args: &[&str]) -> Result<Cli, Error> {
    Cli::try_parse_from(std::iter::once("riv").chain(args.iter().copied()))
}

fn expect_err(args: &[&str]) -> Error {
    parse(args).expect_err("should not parse")
}

#[quench_test::test]
fn the_definition_is_consistent() {
    Cli::command().debug_assert();
}

#[quench_test::test]
fn no_subcommand_is_fine_when_optional() {
    let cli = parse(&[]).unwrap();
    assert!(cli.cmd.is_none());
    assert_eq!(cli.env, None);
}

#[quench_test::test]
fn global_options_work_before_and_after_the_subcommand() {
    let before = parse(&["--env", "prod", "env", "list"]).unwrap();
    assert_eq!(before.env.as_deref(), Some("prod"));
    let after = parse(&["env", "list", "-e", "prod"]).unwrap();
    assert_eq!(after.env.as_deref(), Some("prod"));
    let deep = parse(&["apply", "--env=stage"]).unwrap();
    assert_eq!(deep.env.as_deref(), Some("stage"));
    let glued = parse(&["list", "-estage"]).unwrap();
    assert_eq!(glued.env.as_deref(), Some("stage"));
    assert!(parse(&["env", "list", "--verbose"]).unwrap().verbose);
}

#[quench_test::test]
fn nested_subcommands_and_aliases() {
    let cli = parse(&["env", "set", "prod"]).unwrap();
    assert!(matches!(
        cli.cmd,
        Some(Cmd::Env { cmd: EnvCmd::Set { ref env } }) if env == "prod"
    ));
    for alias in ["apply", "a", "ap", "hidden-alias"] {
        assert!(matches!(
            parse(&[alias]).unwrap().cmd,
            Some(Cmd::Apply { .. })
        ));
    }
    assert!(matches!(
        parse(&["ls"]).unwrap().cmd,
        Some(Cmd::List { .. })
    ));
    assert!(matches!(
        parse(&["renamed"]).unwrap().cmd,
        Some(Cmd::Original)
    ));
    assert!(matches!(
        parse(&["nested", "inner"]).unwrap().cmd,
        Some(Cmd::Nested(Nest::Inner))
    ));
}

#[quench_test::test]
fn value_enums_have_defaults_and_validate() {
    let Some(Cmd::List { scope, targets }) = parse(&["list"]).unwrap().cmd else {
        panic!("list");
    };
    assert_eq!(scope, Scope::All);
    assert!(targets.is_empty());

    let Some(Cmd::List { scope, targets }) =
        parse(&["list", "--scope", "imm", "a/b", "c"]).unwrap().cmd
    else {
        panic!("list");
    };
    assert_eq!(scope, Scope::Immutable);
    assert_eq!(targets, ["a/b", "c"]);

    let error = expect_err(&["list", "--scope", "bogus"]);
    assert_eq!(error.kind(), ErrorKind::InvalidValue);
    assert!(
        error
            .message()
            .contains("[possible values: all, mutable, imm]")
    );
    assert!(
        expect_err(&["list", "--scope", "hidden"])
            .message()
            .contains("hidden")
    );
}

#[quench_test::test]
fn flags_options_defaults_and_clusters() {
    let Some(Cmd::Apply {
        dry_run,
        timeout,
        wait,
        force,
        r#use,
        count,
        label,
    }) = parse(&["apply"]).unwrap().cmd
    else {
        panic!("apply");
    };
    assert!(!dry_run && !force && !r#use);
    assert_eq!((timeout, wait.as_str(), count), (300, "10", None));
    assert!(label.is_empty());

    let Some(Cmd::Apply {
        dry_run,
        timeout,
        wait,
        force,
        r#use,
        count,
        label,
    }) = parse(&[
        "apply",
        "--dry-run",
        "--timeout=5",
        "-w",
        "30",
        "-fn7",
        "--use",
        "--label",
        "a",
        "--label=b",
    ])
    .unwrap()
    .cmd
    else {
        panic!("apply");
    };
    assert!(dry_run && force && r#use);
    assert_eq!((timeout, wait.as_str(), count), (5, "30", Some(7)));
    assert_eq!(label, ["a", "b"]);

    let Some(Cmd::Apply { count, .. }) = parse(&["apply", "-n=9"]).unwrap().cmd else {
        panic!("apply");
    };
    assert_eq!(count, Some(9));
}

#[quench_test::test]
fn typed_values_report_what_was_wrong() {
    let error = expect_err(&["apply", "--timeout", "soon"]);
    assert_eq!(error.kind(), ErrorKind::InvalidValue);
    assert!(
        error
            .message()
            .contains("invalid value 'soon' for '--timeout <TIMEOUT>'")
    );
}

#[quench_test::test]
fn trailing_var_args_swallow_flags() {
    let Some(Cmd::Test { args }) = parse(&["test", "--release", "-p", "x", "--", "y"])
        .unwrap()
        .cmd
    else {
        panic!("test");
    };
    assert_eq!(args, ["--release", "-p", "x", "--", "y"]);
    let Some(Cmd::Test { args }) = parse(&["test"]).unwrap().cmd else {
        panic!("test");
    };
    assert!(args.is_empty());
}

#[quench_test::test]
fn double_dash_ends_options() {
    let Some(Cmd::List { targets, .. }) = parse(&["list", "--", "--weird", "-x"]).unwrap().cmd
    else {
        panic!("list");
    };
    assert_eq!(targets, ["--weird", "-x"]);
}

#[quench_test::test]
fn required_arguments_and_unknown_things_are_errors() {
    let error = expect_err(&["get"]);
    assert_eq!(error.kind(), ErrorKind::MissingRequired);
    assert!(error.message().contains("<NAME>") && error.message().contains("<KEY>..."));
    assert!(error.render().contains("Usage: riv get"));
    assert!(
        error
            .render()
            .contains("For more information, try '--help'.")
    );

    let ok = parse(&["get", "n", "k1", "k2"]).unwrap();
    assert!(
        matches!(ok.cmd, Some(Cmd::Get { ref name, ref keys }) if name == "n" && keys.len() == 2)
    );

    assert_eq!(expect_err(&["--nope"]).kind(), ErrorKind::UnknownArgument);
    assert_eq!(expect_err(&["-z"]).kind(), ErrorKind::UnknownArgument);
    assert_eq!(expect_err(&["nope"]).kind(), ErrorKind::UnknownSubcommand);
    assert_eq!(
        expect_err(&["apply", "extra"]).kind(),
        ErrorKind::UnknownArgument
    );
    assert_eq!(
        expect_err(&["apply", "--dry-run=yes"]).kind(),
        ErrorKind::UnexpectedValue
    );
    assert_eq!(
        expect_err(&["apply", "--timeout"]).kind(),
        ErrorKind::MissingValue
    );
    assert_eq!(
        expect_err(&["apply", "--timeout", "--force"]).kind(),
        ErrorKind::MissingValue
    );
    assert_eq!(expect_err(&["env"]).kind(), ErrorKind::MissingSubcommand);
}

#[quench_test::test]
fn conflicting_arguments_are_rejected() {
    let error = expect_err(&["--verbose", "--quiet"]);
    assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    assert!(
        error
            .message()
            .contains("'--verbose' cannot be used with '--quiet'")
    );
    assert!(parse(&["--quiet"]).is_ok());
}

#[quench_test::test]
fn tuple_variants_flatten_their_args() {
    let cli = parse(&["set-branch", "owner/name", "main"]).unwrap();
    let Some(Cmd::SetBranch(args)) = cli.cmd else {
        panic!("set-branch");
    };
    assert_eq!(
        (args.repo.as_str(), args.branch.as_deref()),
        ("owner/name", Some("main"))
    );
    assert!(parse(&["set-branch"]).is_err());
}

#[quench_test::test]
fn help_and_version_are_not_failures() {
    let help = expect_err(&["-h"]);
    assert_eq!(help.kind(), ErrorKind::DisplayHelp);
    assert!(help.is_display());
    let text = help.render();
    assert!(
        text.starts_with("The riveter\n\nUsage: riv [OPTIONS] [COMMAND]\n"),
        "{text}"
    );
    assert!(text.contains("Commands:\n"));
    assert!(
        text.contains("  list, ls      List the resources"),
        "{text}"
    );
    assert!(text.contains("  apply, a, ap  Apply things"), "{text}");
    assert!(
        text.contains("  set-branch\n"),
        "no help text, no trailing spaces: {text}"
    );
    assert!(text.contains("  help"), "{text}");
    assert!(text.contains("-e, --env <ENV>"), "{text}");
    assert!(text.contains("-h, --help"));
    assert!(text.contains("-V, --version"));
    assert!(text.contains("Print help (see more with '--help')"));
    assert!(!text.contains("A longer explanation"));

    let long = expect_err(&["--help"]).render();
    assert!(
        long.contains("A longer explanation that only `--help` shows."),
        "{long}"
    );
    assert!(long.contains("Print help (see a summary with '-h')"));

    let version = expect_err(&["--version"]);
    assert_eq!(version.kind(), ErrorKind::DisplayVersion);
    assert_eq!(version.render(), "riv 1.2.3\n");
    assert_eq!(expect_err(&["-V"]).render(), "riv 1.2.3\n");
    assert_eq!(version.to_string(), "riv 1.2.3");
}

#[quench_test::test]
fn subcommand_help_shows_its_usage_arguments_and_inherited_globals() {
    let text = expect_err(&["list", "--help"]).render();
    assert!(
        text.starts_with("List the resources\n\nUsage: riv list [OPTIONS] [TARGET]...\n"),
        "{text}"
    );
    assert!(text.contains("Arguments:\n  [TARGET]..."), "{text}");
    assert!(text.contains("--scope <SCOPE>"), "{text}");
    assert!(text.contains("[default: all]"), "{text}");
    assert!(
        text.contains("[possible values: all, mutable, imm]"),
        "{text}"
    );
    assert!(
        text.contains("--env <ENV>"),
        "globals are inherited: {text}"
    );
    assert!(!text.contains("--version"), "{text}");

    let apply = expect_err(&["apply", "-h"]).render();
    assert!(
        apply.contains("    --dry-run"),
        "long-only options are indented: {apply}"
    );
    assert!(apply.contains("-w, --wait-for <WAIT>"), "{apply}");
    assert!(apply.contains("[default: 300]"));
}

#[quench_test::test]
fn the_help_subcommand_prints_help_for_what_it_names() {
    let general = expect_err(&["help"]).render();
    assert!(general.contains("Usage: riv [OPTIONS] [COMMAND]"));
    let list = expect_err(&["help", "list"]).render();
    assert!(list.contains("Usage: riv list"), "{list}");
    let nested = expect_err(&["help", "env", "set"]).render();
    assert!(
        nested.contains("Usage: riv env set [OPTIONS] <ENV>"),
        "{nested}"
    );
    assert_eq!(
        expect_err(&["help", "nope"]).kind(),
        ErrorKind::UnknownSubcommand
    );
}

#[quench_test::test]
fn the_command_tree_can_be_inspected() {
    let command = Cli::command();
    assert_eq!(command.get_name(), "riv");
    assert_eq!(command.get_about(), Some("The riveter"));
    assert_eq!(command.get_version(), Some("1.2.3"));
    let names: Vec<&str> = command.get_subcommands().map(Command::get_name).collect();
    assert!(names.contains(&"list") && names.contains(&"set-branch") && names.contains(&"renamed"));
    let list = command.find_subcommand("ls").expect("alias lookup");
    assert_eq!(list.get_name(), "list");
    assert_eq!(list.get_visible_aliases().collect::<Vec<_>>(), ["ls"]);
    let apply = command.find_subcommand("apply").unwrap();
    assert!(apply.get_all_aliases().any(|a| a == "hidden-alias"));
    assert!(
        apply
            .get_arguments()
            .any(|a| a.get_long() == Some("dry-run"))
    );
    assert!(command.render_help().contains("Usage:"));
    assert!(command.render_long_help().contains("Usage:"));
}

// A bare word is a positional when the command has positionals, a subcommand when it names one.
#[derive(Parser, Debug)]
#[command(name = "foreman")]
struct Foreman {
    #[command(subcommand)]
    command: Option<ForemanCmd>,

    /// A bare service name starts it.
    #[arg(trailing_var_arg = true, hide = true)]
    bare: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum ForemanCmd {
    Start {
        services: Vec<String>,
    },
    #[command(alias = "pick")]
    Repl {
        services: Vec<String>,
    },
}

#[quench_test::test]
fn positionals_and_subcommands_share_a_command() {
    let bare = Foreman::try_parse_from(["foreman", "conveyor", "--flag"]).unwrap();
    assert!(bare.command.is_none());
    assert_eq!(bare.bare, ["conveyor", "--flag"]);

    let start = Foreman::try_parse_from(["foreman", "start", "a", "b"]).unwrap();
    assert!(
        matches!(start.command, Some(ForemanCmd::Start { ref services }) if services == &["a", "b"])
    );
    assert!(start.bare.is_empty());

    assert!(matches!(
        Foreman::try_parse_from(["foreman", "pick"])
            .unwrap()
            .command,
        Some(ForemanCmd::Repl { .. })
    ));
    let text = Foreman::command().render_help();
    assert!(
        !text.contains("BARE"),
        "hidden positionals are not listed: {text}"
    );
}

#[derive(Parser, Debug)]
struct Plain {
    #[arg(long, short)]
    verbose: bool,
    #[arg(long, short = 'o')]
    output: Option<std::path::PathBuf>,
    #[arg(long, default_value_t = 1.5)]
    ratio: f64,
    input: String,
}

#[quench_test::test]
fn a_command_without_subcommands_and_default_names() {
    let plain = Plain::try_parse_from(["prog", "-vo", "out.txt", "in.txt"]).unwrap();
    assert!(plain.verbose);
    assert_eq!(
        plain.output.as_deref(),
        Some(std::path::Path::new("out.txt"))
    );
    assert!((plain.ratio - 1.5).abs() < f64::EPSILON);
    assert_eq!(plain.input, "in.txt");
    // `-` alone is a value, and `-5`-like words after `--` are values too.
    assert_eq!(Plain::try_parse_from(["prog", "-"]).unwrap().input, "-");
    assert!(Plain::try_parse_from(["prog"]).is_err());
    assert!(Plain::try_parse_from(["prog", "a", "b"]).is_err());
    let usage = Plain::try_parse_from(["prog"]).unwrap_err().render();
    assert!(
        usage.contains("Usage: quench-args [OPTIONS] <INPUT>"),
        "{usage}"
    );
    // Without `version`, there is no `-V`.
    assert_eq!(
        Plain::try_parse_from(["prog", "-V"]).unwrap_err().kind(),
        ErrorKind::UnknownArgument
    );
}

#[derive(Parser, Debug)]
#[command(
    name = "templ",
    help_template = "{name} {version}\n{about}\n\n{usage-heading} {usage}\n\nOptions:\n{options}\n\nEnd.",
    version = "9",
    about = "Templated"
)]
struct Templ {
    #[arg(long)]
    x: bool,
}

#[quench_test::test]
fn help_templates_fill_in_their_placeholders() {
    let text = Templ::try_parse_from(["templ", "-h"]).unwrap_err().render();
    assert!(
        text.starts_with("templ 9\nTemplated\n\nUsage: templ [OPTIONS]\n\nOptions:\n"),
        "{text}"
    );
    assert!(text.contains("--x"));
    assert!(text.trim_end().ends_with("End."));
}

#[quench_test::test]
fn errors_render_for_people() {
    let error = expect_err(&["--nope"]);
    let text = error.render();
    assert!(
        text.starts_with(
            "error: unexpected argument '--nope' found\n\nUsage: riv [OPTIONS] [COMMAND]\n"
        ),
        "{text}"
    );
    assert_eq!(error.to_string(), text.trim_end());
    assert!(format!("{error:?}").contains("UnknownArgument"));
    let invalid = Error::invalid_value("--n <N>", "x", "not a number");
    assert!(
        invalid
            .render()
            .starts_with("error: invalid value 'x' for '--n <N>': not a number")
    );
}

#[quench_test::test]
fn non_utf8_arguments_are_an_error() {
    use std::os::unix::ffi::OsStringExt;
    let bad = std::ffi::OsString::from_vec(vec![0xff, 0xfe]);
    let error = Cli::command()
        .try_get_matches_from([bad])
        .expect_err("invalid utf-8");
    assert_eq!(error.kind(), ErrorKind::InvalidUtf8);
}

#[quench_test::test]
fn matches_can_be_read_directly() {
    let command = Command::new("m")
        .arg(Arg::new("name").long("name").short('n').kind(Kind::Many))
        .arg(Arg::new("flag").long("flag").kind(Kind::Flag))
        .arg(Arg::new("rest").kind(Kind::Many));
    let m = command
        .try_get_matches_from(["--name", "a", "-n", "b", "--flag", "x", "y"])
        .unwrap();
    assert_eq!(m.get_many("name"), ["a", "b"]);
    assert_eq!(m.get_one("name"), Some("b"));
    assert!(m.flag("flag") && m.is_present("flag") && !m.is_present("nothing"));
    assert_eq!(m.get_many("rest"), ["x", "y"]);
    assert!(m.subcommand().is_none());
    assert!(m.get_required("nothing").is_err());
}

#[quench_test::test]
#[should_panic(expected = "duplicate long flag")]
fn debug_assert_catches_duplicate_flags() {
    Command::new("c")
        .arg(Arg::new("a").long("same").kind(Kind::Flag))
        .arg(Arg::new("b").long("same").kind(Kind::Flag))
        .debug_assert();
}

#[quench_test::test]
#[should_panic(expected = "already built in")]
fn debug_assert_catches_a_clash_with_help() {
    Command::new("c")
        .arg(Arg::new("h").short('h').kind(Kind::Flag))
        .debug_assert();
}

#[quench_test::test]
#[should_panic(expected = "unknown argument")]
fn debug_assert_catches_a_conflict_with_nothing() {
    Command::new("c")
        .arg(
            Arg::new("a")
                .long("a")
                .kind(Kind::Flag)
                .conflicts_with("zzz"),
        )
        .debug_assert();
}

#[quench_test::test]
#[should_panic(expected = "duplicate subcommand")]
fn debug_assert_catches_duplicate_subcommands() {
    Command::new("c")
        .subcommand(Command::new("x"))
        .subcommand(Command::new("x"))
        .debug_assert();
}

#[quench_test::test]
#[should_panic(expected = "no `--long` or `-s`")]
fn debug_assert_catches_a_positional_flag() {
    Command::new("c")
        .arg(Arg::new("a").kind(Kind::Flag))
        .debug_assert();
}

#[quench_test::test]
#[should_panic(expected = "duplicate argument id")]
fn debug_assert_catches_duplicate_ids() {
    Command::new("c")
        .arg(Arg::new("a").long("a1"))
        .arg(Arg::new("a").long("a2"))
        .debug_assert();
}

#[quench_test::test]
#[should_panic(expected = "duplicate subcommand")]
fn debug_assert_catches_a_reused_alias() {
    Command::new("c")
        .subcommand(Command::new("x").alias("y"))
        .subcommand(Command::new("y"))
        .debug_assert();
}

#[quench_test::test]
#[should_panic(expected = "duplicate short flag")]
fn debug_assert_catches_duplicate_shorts() {
    Command::new("c")
        .arg(Arg::new("a").short('q').kind(Kind::Flag))
        .arg(Arg::new("b").short('q').kind(Kind::Flag))
        .debug_assert();
}

#[quench_test::test]
fn possible_values_and_hidden_flags_in_help() {
    let command = Command::new("c")
        .arg(Arg::new("mode").long("mode").possible_values([
            PossibleValue::new("fast").help("quick"),
            PossibleValue::new("secret").hide(true),
        ]))
        .arg(
            Arg::new("hidden")
                .long("hidden")
                .kind(Kind::Flag)
                .hide(true),
        )
        .subcommand(Command::new("sub").hide(true))
        .subcommand(Command::new("shown").about("Shown"));
    let help = command.render_help();
    assert!(help.contains("[possible values: fast]"), "{help}");
    assert!(
        !help.contains("secret") && !help.contains("--hidden") && !help.contains("  sub"),
        "{help}"
    );
    assert!(help.contains("shown  Shown"));
    assert_eq!(
        PossibleValue::new("fast").help("quick").get_help(),
        Some("quick")
    );
    assert!(command.try_get_matches_from(["--mode", "secret"]).is_ok());
    assert!(command.try_get_matches_from(["--mode", "slow"]).is_err());
}

#[quench_test::test]
fn value_enums_parse_case_insensitively_on_request() {
    assert_eq!(
        <Scope as ValueEnum>::from_str("MUTABLE", true),
        Ok(Scope::Mutable)
    );
    assert!(<Scope as ValueEnum>::from_str("MUTABLE", false).is_err());
    assert_eq!(Scope::value_variants().len(), 3);
    assert!(Scope::Hidden.to_possible_value().is_none());
    assert_eq!(
        Scope::Mutable.to_possible_value().unwrap().get_help(),
        Some("Only the mutable ones")
    );
}

#[derive(Parser, Debug)]
struct Anvil {
    #[arg(long, global = true, conflicts_with = "tail")]
    silent: bool,
    #[arg(long, global = true, value_name = "N", num_args = 0..=1, require_equals = true, default_missing_value = "80")]
    tail: Option<usize>,
    #[arg(short = 'p', long, num_args = 0..=1, default_missing_value = "all")]
    pick: Option<String>,
    #[command(subcommand)]
    command: AnvilCmd,
}

#[derive(Subcommand, Debug)]
enum AnvilCmd {
    Lint {
        #[arg(long, default_value = "true")]
        deny_warnings: bool,
        #[arg(long)]
        json: bool,
    },
    Build,
}

#[quench_test::test]
fn optional_values_and_switches_that_default_on() {
    let anvil =
        |args: &[&str]| Anvil::try_parse_from(std::iter::once("anvil").chain(args.iter().copied()));

    let plain = anvil(&["lint"]).unwrap();
    assert_eq!((plain.tail, plain.silent), (None, false));
    assert!(matches!(
        plain.command,
        AnvilCmd::Lint {
            deny_warnings: true,
            json: false
        }
    ));

    // `--tail` alone means 80; with `=` it takes the value; the next word stays the subcommand.
    assert_eq!(anvil(&["--tail", "build"]).unwrap().tail, Some(80));
    assert_eq!(anvil(&["--tail=200", "build"]).unwrap().tail, Some(200));
    assert_eq!(anvil(&["build", "--tail"]).unwrap().tail, Some(80));
    assert_eq!(anvil(&["build", "--tail=5"]).unwrap().tail, Some(5));

    // Without `require_equals`, the next word is the value unless it is a flag or absent.
    assert_eq!(
        anvil(&["--pick", "x", "build"]).unwrap().pick.as_deref(),
        Some("x")
    );
    assert_eq!(
        anvil(&["-p", "--silent", "build"]).unwrap().pick.as_deref(),
        Some("all")
    );
    assert_eq!(anvil(&["-px", "build"]).unwrap().pick.as_deref(), Some("x"));
    assert_eq!(
        anvil(&["build", "-p"]).unwrap_err().kind(),
        ErrorKind::UnknownArgument
    );

    assert_eq!(
        anvil(&["--silent", "--tail", "build"]).unwrap_err().kind(),
        ErrorKind::ArgumentConflict
    );
    assert!(anvil(&["--tail=x", "build"]).is_err());

    let help = Anvil::command().render_help();
    assert!(help.contains("--tail[=<N>]"), "{help}");
    Anvil::command().debug_assert();
}

#[quench_test::test]
fn help_for_a_path_shows_the_whole_usage_path() {
    let command = Cli::command();
    let text = command
        .help_for(&["env", "set"], false)
        .expect("a real path");
    assert!(
        text.contains("Usage: riv env set [OPTIONS] <ENV>"),
        "{text}"
    );
    assert!(
        text.contains("--env <ENV>"),
        "globals of the ancestors are listed: {text}"
    );
    assert!(
        command
            .help_for(&["ls"], true)
            .unwrap()
            .contains("Usage: riv list")
    );
    assert!(command.help_for(&["nope"], false).is_none());
    assert!(
        command
            .help_for(&[], false)
            .unwrap()
            .contains("Usage: riv [OPTIONS]")
    );
    command.print_help().unwrap();
}
