//! The description of a command line: a [`Command`] with [`Arg`]s and subcommands.

/// One accepted spelling of an enumerated value, with an optional help line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PossibleValue {
    name: String,
    help: Option<String>,
    hidden: bool,
}

impl PossibleValue {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            help: None,
            hidden: false,
        }
    }

    #[must_use]
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    #[must_use]
    pub const fn hide(mut self, hide: bool) -> Self {
        self.hidden = hide;
        self
    }

    #[must_use]
    pub fn get_name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn get_help(&self) -> Option<&str> {
        self.help.as_deref()
    }

    #[must_use]
    pub const fn is_hide_set(&self) -> bool {
        self.hidden
    }
}

/// How many values an argument takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A switch with no value: present or not.
    Flag,
    /// One value (`--name value`, or one positional); a repeat replaces the earlier one.
    Single,
    /// Any number of values: a positional that takes the rest, or an option that may repeat.
    Many,
}

/// One flag, option or positional.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Arg {
    pub(crate) id: String,
    pub(crate) long: Option<String>,
    pub(crate) short: Option<char>,
    pub(crate) value_name: Option<String>,
    pub(crate) help: Option<String>,
    pub(crate) long_help: Option<String>,
    pub(crate) kind: Kind,
    pub(crate) required: bool,
    pub(crate) global: bool,
    pub(crate) default: Option<String>,
    pub(crate) conflicts: Vec<String>,
    pub(crate) trailing: bool,
    pub(crate) allow_hyphen_values: bool,
    pub(crate) hidden: bool,
    pub(crate) possible: Vec<PossibleValue>,
    /// `Some` when the option's value is optional: this is the value it takes when none is given.
    pub(crate) missing_value: Option<String>,
    pub(crate) require_equals: bool,
}

impl Arg {
    /// A positional argument called `id` (add [`long`](Self::long) or [`short`](Self::short) to make it an option).
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            long: None,
            short: None,
            value_name: None,
            help: None,
            long_help: None,
            kind: Kind::Single,
            required: false,
            global: false,
            default: None,
            conflicts: Vec::new(),
            trailing: false,
            allow_hyphen_values: false,
            hidden: false,
            possible: Vec::new(),
            missing_value: None,
            require_equals: false,
        }
    }

    #[must_use]
    pub fn long(mut self, long: impl Into<String>) -> Self {
        self.long = Some(long.into());
        self
    }

    #[must_use]
    pub const fn short(mut self, short: char) -> Self {
        self.short = Some(short);
        self
    }

    /// The placeholder shown for the value in usage and help (`--env <ENV>`).
    #[must_use]
    pub fn value_name(mut self, name: impl Into<String>) -> Self {
        self.value_name = Some(name.into());
        self
    }

    /// The one-line help shown by `-h`.
    #[must_use]
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// The fuller help shown by `--help` (falls back to [`help`](Self::help)).
    #[must_use]
    pub fn long_help(mut self, help: impl Into<String>) -> Self {
        self.long_help = Some(help.into());
        self
    }

    #[must_use]
    pub const fn kind(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }

    #[must_use]
    pub const fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    /// Accepted after any subcommand too, with its value kept at the level that declares it.
    #[must_use]
    pub const fn global(mut self, global: bool) -> Self {
        self.global = global;
        self
    }

    /// The value used when none is given, as the text it would be typed as.
    #[must_use]
    pub fn default_value(mut self, value: impl Into<String>) -> Self {
        self.default = Some(value.into());
        self
    }

    /// Cannot be given together with the argument `id`.
    #[must_use]
    pub fn conflicts_with(mut self, id: impl Into<String>) -> Self {
        self.conflicts.push(id.into());
        self
    }

    /// Once the first value of this positional is seen, everything after it is a value, flags included.
    #[must_use]
    pub const fn trailing_var_arg(mut self, on: bool) -> Self {
        self.trailing = on;
        self
    }

    /// A value may start with `-` (it is not mistaken for an unknown flag).
    #[must_use]
    pub const fn allow_hyphen_values(mut self, on: bool) -> Self {
        self.allow_hyphen_values = on;
        self
    }

    /// Left out of help.
    #[must_use]
    pub const fn hide(mut self, hide: bool) -> Self {
        self.hidden = hide;
        self
    }

    /// Makes the option's value optional: `--name` alone means `missing`, `--name=value` or `--name value` give one.
    #[must_use]
    pub fn optional_value(mut self, missing: impl Into<String>) -> Self {
        self.missing_value = Some(missing.into());
        self
    }

    /// The value must be attached with `=` (`--name=value`); the next word is never taken as the value.
    #[must_use]
    pub const fn require_equals(mut self, on: bool) -> Self {
        self.require_equals = on;
        self
    }

    /// Only these spellings are accepted.
    #[must_use]
    pub fn possible_values(mut self, values: impl IntoIterator<Item = PossibleValue>) -> Self {
        self.possible = values.into_iter().collect();
        self
    }

    #[must_use]
    pub fn get_id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn get_long(&self) -> Option<&str> {
        self.long.as_deref()
    }

    #[must_use]
    pub const fn get_short(&self) -> Option<char> {
        self.short
    }

    #[must_use]
    pub const fn is_positional(&self) -> bool {
        self.long.is_none() && self.short.is_none()
    }

    #[must_use]
    pub const fn is_required_set(&self) -> bool {
        self.required
    }

    #[must_use]
    pub const fn is_global_set(&self) -> bool {
        self.global
    }

    /// The placeholder for the value: the explicit name, else the id in upper case.
    pub(crate) fn placeholder(&self) -> String {
        self.value_name
            .clone()
            .unwrap_or_else(|| self.id.to_uppercase().replace('-', "_"))
    }

    /// How the argument is spelled in messages: `--env <ENV>`, `-v`, `<TARGET>`.
    pub(crate) fn display_name(&self) -> String {
        match (&self.long, self.short, self.kind) {
            (Some(long), _, Kind::Flag) => format!("--{long}"),
            (Some(long), _, _) if self.missing_value.is_some() => {
                format!("--{long}[=<{}>]", self.placeholder())
            }
            (Some(long), _, _) => format!("--{long} <{}>", self.placeholder()),
            (None, Some(short), Kind::Flag) => format!("-{short}"),
            (None, Some(short), _) => format!("-{short} <{}>", self.placeholder()),
            (None, None, Kind::Many) => format!("<{}>...", self.placeholder()),
            (None, None, _) => format!("<{}>", self.placeholder()),
        }
    }
}

/// A command and the arguments and subcommands it accepts.
#[derive(Debug, Clone)]
pub struct Command {
    pub(crate) name: String,
    pub(crate) about: Option<String>,
    pub(crate) long_about: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) args: Vec<Arg>,
    pub(crate) subcommands: Vec<Self>,
    pub(crate) aliases: Vec<String>,
    pub(crate) visible_aliases: Vec<String>,
    pub(crate) help_template: Option<String>,
    pub(crate) disable_help_subcommand: bool,
    pub(crate) subcommand_required: bool,
    pub(crate) hidden: bool,
}

impl Command {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            about: None,
            long_about: None,
            version: None,
            args: Vec::new(),
            subcommands: Vec::new(),
            aliases: Vec::new(),
            visible_aliases: Vec::new(),
            help_template: None,
            disable_help_subcommand: false,
            subcommand_required: false,
            hidden: false,
        }
    }

    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// The one-line description shown in a parent's command list and by `-h`.
    #[must_use]
    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = Some(about.into());
        self
    }

    /// The fuller description shown by `--help` (falls back to [`about`](Self::about)).
    #[must_use]
    pub fn long_about(mut self, about: impl Into<String>) -> Self {
        self.long_about = Some(about.into());
        self
    }

    /// Turns on `-V` / `--version`.
    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    #[must_use]
    pub fn arg(mut self, arg: Arg) -> Self {
        self.args.push(arg);
        self
    }

    #[must_use]
    pub fn subcommand(mut self, command: Self) -> Self {
        self.subcommands.push(command);
        self
    }

    /// An extra name that works but is not listed.
    #[must_use]
    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.push(alias.into());
        self
    }

    /// An extra name that works and is listed in help.
    #[must_use]
    pub fn visible_alias(mut self, alias: impl Into<String>) -> Self {
        self.visible_aliases.push(alias.into());
        self
    }

    /// Replaces the default help layout. Placeholders: `{name}`, `{version}`, `{about}`, `{usage-heading}`,
    /// `{usage}`, `{all-args}`, `{options}`, `{positionals}`, `{subcommands}`.
    #[must_use]
    pub fn help_template(mut self, template: impl Into<String>) -> Self {
        self.help_template = Some(template.into());
        self
    }

    /// Does not add the `help` subcommand (the command can define its own).
    #[must_use]
    pub const fn disable_help_subcommand(mut self, disable: bool) -> Self {
        self.disable_help_subcommand = disable;
        self
    }

    /// Parsing fails when no subcommand is given.
    #[must_use]
    pub const fn subcommand_required(mut self, required: bool) -> Self {
        self.subcommand_required = required;
        self
    }

    /// Left out of its parent's list.
    #[must_use]
    pub const fn hide(mut self, hide: bool) -> Self {
        self.hidden = hide;
        self
    }

    #[must_use]
    pub fn get_name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn get_about(&self) -> Option<&str> {
        self.about.as_deref()
    }

    #[must_use]
    pub fn get_version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    pub fn get_subcommands(&self) -> impl Iterator<Item = &Self> {
        self.subcommands.iter()
    }

    pub fn get_arguments(&self) -> impl Iterator<Item = &Arg> {
        self.args.iter()
    }

    pub fn get_visible_aliases(&self) -> impl Iterator<Item = &str> {
        self.visible_aliases.iter().map(String::as_str)
    }

    pub fn get_all_aliases(&self) -> impl Iterator<Item = &str> {
        self.aliases
            .iter()
            .chain(&self.visible_aliases)
            .map(String::as_str)
    }

    #[must_use]
    pub fn find_subcommand(&self, name: &str) -> Option<&Self> {
        self.subcommands
            .iter()
            .find(|sub| sub.name == name || sub.get_all_aliases().any(|alias| alias == name))
    }

    pub(crate) fn find_long(&self, long: &str) -> Option<&Arg> {
        self.args
            .iter()
            .find(|arg| arg.long.as_deref() == Some(long))
    }

    pub(crate) fn find_short(&self, short: char) -> Option<&Arg> {
        self.args.iter().find(|arg| arg.short == Some(short))
    }

    pub(crate) fn positionals(&self) -> impl Iterator<Item = &Arg> {
        self.args.iter().filter(|arg| arg.is_positional())
    }

    /// Whether this command gets the implicit `help` subcommand.
    pub(crate) fn has_help_subcommand(&self) -> bool {
        !self.subcommands.is_empty()
            && !self.disable_help_subcommand
            && self.find_subcommand("help").is_none()
    }

    /// Checks the definition for mistakes that would otherwise surface only as odd parses: duplicate names,
    /// a flag with a default, conflicts with unknown arguments. Panics with a message naming the problem.
    ///
    /// # Panics
    ///
    /// On an inconsistent definition.
    pub fn debug_assert(&self) {
        let mut ids = std::collections::HashSet::new();
        let mut longs = std::collections::HashSet::new();
        let mut shorts = std::collections::HashSet::new();
        for arg in &self.args {
            assert!(
                ids.insert(arg.id.as_str()),
                "command `{}`: duplicate argument id `{}`",
                self.name,
                arg.id
            );
            if let Some(long) = &arg.long {
                assert!(
                    longs.insert(long.as_str()),
                    "command `{}`: duplicate long flag `--{long}`",
                    self.name
                );
                assert!(
                    long != "help" && !(long == "version" && self.version.is_some()),
                    "command `{}`: `--{long}` is already built in",
                    self.name
                );
            }
            if let Some(short) = arg.short {
                assert!(
                    shorts.insert(short),
                    "command `{}`: duplicate short flag `-{short}`",
                    self.name
                );
                assert!(
                    short != 'h' && !(short == 'V' && self.version.is_some()),
                    "command `{}`: `-{short}` is already built in",
                    self.name
                );
            }
            assert!(
                !(arg.kind == Kind::Flag && arg.is_positional()),
                "command `{}`: `{}` is a flag with no `--long` or `-s` to spell it",
                self.name,
                arg.id
            );
            for other in &arg.conflicts {
                assert!(
                    self.args.iter().any(|a| &a.id == other),
                    "command `{}`: `{}` conflicts with the unknown argument `{other}`",
                    self.name,
                    arg.id
                );
            }
        }
        let mut names = std::collections::HashSet::new();
        for sub in &self.subcommands {
            assert!(
                names.insert(sub.name.as_str()),
                "command `{}`: duplicate subcommand `{}`",
                self.name,
                sub.name
            );
            for alias in sub.get_all_aliases() {
                assert!(
                    names.insert(alias),
                    "command `{}`: the alias `{alias}` is used twice",
                    self.name
                );
            }
            sub.debug_assert();
        }
    }
}
