//! Severity levels and the directives that filter on them.

use std::fmt;

/// How serious an event is. Lower is chattier: `Trace < Debug < Info < Warn < Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    /// `TRACE`, `DEBUG`, `INFO`, `WARN` or `ERROR`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }

    /// Reads a level name in any case; `warning` is accepted for `warn`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "trace" | "5" => Some(Self::Trace),
            "debug" | "4" => Some(Self::Debug),
            "info" | "3" => Some(Self::Info),
            "warn" | "warning" | "2" => Some(Self::Warn),
            "error" | "1" => Some(Self::Error),
            _ => None,
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

/// The least severe level that still gets through, or nothing at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelFilter {
    Off,
    From(Level),
}

impl LevelFilter {
    #[must_use]
    pub fn allows(self, level: Level) -> bool {
        match self {
            Self::Off => false,
            Self::From(least) => level >= least,
        }
    }
}

/// The filter text could not be understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterError(pub String);

impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid log filter: {}", self.0)
    }
}

impl std::error::Error for FilterError {}

/// Which targets log at which level: the `RUST_LOG` syntax,
/// `info,quench_http=debug,hyper=warn,noisy_crate=off`.
///
/// A bare level sets the default; `target=level` applies to that module path
/// and everything beneath it, the longest matching target winning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    default: LevelFilter,
    /// `(target, level)`, longest target first.
    directives: Vec<(String, LevelFilter)>,
}

impl Filter {
    /// Everything at `level` and above.
    #[must_use]
    pub const fn level(level: Level) -> Self {
        Self {
            default: LevelFilter::From(level),
            directives: Vec::new(),
        }
    }

    /// Parses directives. A bare `target` (no `=`) turns that target fully on.
    pub fn parse(text: &str) -> Result<Self, FilterError> {
        let mut default = LevelFilter::From(Level::Error);
        let mut saw_default = false;
        let mut directives = Vec::new();
        for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            // Span directives (`target[span{field=value}]=level`) have no
            // meaning here: keep the part before `[`.
            let part = part.split('[').next().unwrap_or(part);
            match part.split_once('=') {
                Some((target, level)) => {
                    directives.push((target.trim().to_string(), parse_level_filter(level)?));
                }
                None => match parse_level_filter(part) {
                    Ok(level) => {
                        default = level;
                        saw_default = true;
                    }
                    // `my_crate` alone means "all of my_crate"
                    Err(_) if is_target(part) => {
                        directives.push((part.to_string(), LevelFilter::From(Level::Trace)));
                    }
                    Err(error) => return Err(error),
                },
            }
        }
        if !saw_default && directives.is_empty() {
            return Err(FilterError("no directives".to_string()));
        }
        if !saw_default {
            // Directives alone leave everything else at the usual default.
            default = LevelFilter::From(Level::Error);
        }
        directives.sort_by_key(|(target, _)| std::cmp::Reverse(target.len()));
        Ok(Self {
            default,
            directives,
        })
    }

    /// Whether an event at `level` from module path `target` is wanted.
    #[must_use]
    pub fn enabled(&self, level: Level, target: &str) -> bool {
        for (prefix, filter) in &self.directives {
            if target == prefix
                || target
                    .strip_prefix(prefix.as_str())
                    .is_some_and(|rest| rest.starts_with("::"))
            {
                return filter.allows(level);
            }
        }
        self.default.allows(level)
    }
}

fn parse_level_filter(text: &str) -> Result<LevelFilter, FilterError> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("off") || text == "0" {
        return Ok(LevelFilter::Off);
    }
    Level::parse(text)
        .map(LevelFilter::From)
        .ok_or_else(|| FilterError(format!("`{text}` is not a level")))
}

fn is_target(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_order_and_parse() {
        assert!(
            Level::Trace < Level::Debug
                && Level::Debug < Level::Info
                && Level::Info < Level::Warn
                && Level::Warn < Level::Error
        );
        for (text, level) in [
            ("TRACE", Level::Trace),
            ("debug", Level::Debug),
            (" Info ", Level::Info),
            ("warn", Level::Warn),
            ("WARNING", Level::Warn),
            ("error", Level::Error),
            ("5", Level::Trace),
            ("1", Level::Error),
        ] {
            assert_eq!(Level::parse(text), Some(level), "{text}");
        }
        assert_eq!(Level::parse("loud"), None);
        assert_eq!(Level::Warn.to_string(), "WARN");
        assert_eq!(
            format!("{:>5}|{:<5}|", Level::Info, Level::Warn),
            " INFO|WARN |"
        );
    }

    #[test]
    fn level_filters() {
        assert!(LevelFilter::From(Level::Info).allows(Level::Info));
        assert!(LevelFilter::From(Level::Info).allows(Level::Error));
        assert!(!LevelFilter::From(Level::Info).allows(Level::Debug));
        assert!(!LevelFilter::Off.allows(Level::Error));
    }

    #[test]
    fn a_bare_level_is_the_default() {
        let f = Filter::parse("warn").unwrap();
        assert!(f.enabled(Level::Warn, "anything::at_all"));
        assert!(!f.enabled(Level::Info, "anything"));
        assert_eq!(Filter::level(Level::Debug), Filter::parse("debug").unwrap());
    }

    #[test]
    fn targets_match_their_modules_and_longest_wins() {
        let f =
            Filter::parse("info,quench_http=debug,quench_http::server=trace,hyper=warn,noisy=off")
                .unwrap();
        assert!(f.enabled(Level::Debug, "quench_http"));
        assert!(f.enabled(Level::Debug, "quench_http::router"));
        assert!(!f.enabled(Level::Trace, "quench_http::router"));
        assert!(f.enabled(Level::Trace, "quench_http::server"));
        assert!(f.enabled(Level::Trace, "quench_http::server::conn"));
        assert!(!f.enabled(Level::Info, "hyper::proto"));
        assert!(f.enabled(Level::Warn, "hyper"));
        assert!(!f.enabled(Level::Error, "noisy::deep"));
        // a prefix of the name that is not a module boundary does not match
        assert!(!f.enabled(Level::Debug, "quench_http_extra"));
        assert!(f.enabled(Level::Info, "other"));
        assert!(!f.enabled(Level::Debug, "other"));
    }

    #[test]
    fn a_bare_target_turns_it_fully_on() {
        let f = Filter::parse("my_crate").unwrap();
        assert!(f.enabled(Level::Trace, "my_crate::x"));
        assert!(!f.enabled(Level::Warn, "elsewhere"));
        assert!(f.enabled(Level::Error, "elsewhere"));
    }

    #[test]
    fn directives_without_a_default_leave_errors_on() {
        let f = Filter::parse("a=debug").unwrap();
        assert!(f.enabled(Level::Debug, "a"));
        assert!(f.enabled(Level::Error, "b"));
        assert!(!f.enabled(Level::Warn, "b"));
    }

    #[test]
    fn off_and_span_directives() {
        let f = Filter::parse("off,a=info").unwrap();
        assert!(!f.enabled(Level::Error, "b"));
        assert!(f.enabled(Level::Info, "a"));
        let span = Filter::parse("a[request{id=1}]=debug").unwrap();
        assert!(span.enabled(Level::Debug, "a"));
    }

    #[test]
    fn bad_filters_are_errors() {
        for bad in ["", "   ", ",,", "a=loud", "x y"] {
            assert!(Filter::parse(bad).is_err(), "{bad:?}");
        }
        assert!(
            Filter::parse("a=loud")
                .unwrap_err()
                .to_string()
                .contains("not a level")
        );
        assert_eq!(
            Filter::parse("").unwrap_err().to_string(),
            "invalid log filter: no directives"
        );
    }
}
