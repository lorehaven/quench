//! `.gitignore`-style rules (`ignore`'s role): parsing ignore files, matching paths against them, and the
//! set of files a tool such as `grep -r` skips by default.
//!
//! [`Gitignore`] is one ignore file. [`Ignorer`] is what a walk uses: hidden files, the repository's
//! `.gitignore` files (also those of parent directories up to the repository root), `.ignore` files,
//! `.git/info/exclude` and the user's global git ignore file, layered the way git does it. It backs
//! [`Walk::standard_filters`](crate::fs::Walk::standard_filters).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// What a rule set says about a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Match {
    /// No rule matches.
    None,
    /// The last matching rule excludes the path.
    Ignore,
    /// The last matching rule is a `!` rule that re-includes the path.
    Whitelist,
}

#[derive(Debug, Clone)]
struct Rule {
    /// The pattern's `/`-separated segments; a lone `**` segment matches any number of directories.
    segments: Vec<String>,
    negated: bool,
    dir_only: bool,
    /// Contains a `/` (other than a trailing one): matched against the whole path relative to the base,
    /// not just the name.
    anchored: bool,
}

/// The rules of one ignore file, which apply below the directory it sits in.
#[derive(Debug, Clone, Default)]
pub struct Gitignore {
    rules: Vec<Rule>,
}

impl Gitignore {
    /// Parses ignore-file text: one pattern per line, `#` comments, `!` negation, a trailing `/` for
    /// directories only, `*`, `?`, `[a-z]` and `**`.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let rules = text.lines().filter_map(parse_rule).collect();
        Self { rules }
    }

    /// Reads and parses a file; a missing or unreadable one has no rules.
    #[must_use]
    pub fn from_file(path: &Path) -> Self {
        fs::read_to_string(path).map_or_else(|_| Self::default(), |text| Self::parse(&text))
    }

    /// Whether there are no rules.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// What the rules say about `relative`, a path below the file's directory (`/` or `\` separated).
    #[must_use]
    pub fn matched(&self, relative: &str, is_dir: bool) -> Match {
        self.matched_with(relative, is_dir, true)
    }

    /// Like [`matched`](Self::matched) for a file that belongs to no directory (the global ignore file):
    /// `relative` is the full path, and a pattern that contains a `/` can only match if it starts with `**/`.
    #[must_use]
    pub fn matched_names(&self, relative: &str, is_dir: bool) -> Match {
        self.matched_with(relative, is_dir, false)
    }

    fn matched_with(&self, relative: &str, is_dir: bool, anchors: bool) -> Match {
        let parts: Vec<&str> = relative
            .split(['/', '\\'])
            .filter(|p| !p.is_empty())
            .collect();
        let Some(name) = parts.last() else {
            return Match::None;
        };
        for rule in self.rules.iter().rev() {
            if (rule.dir_only && !is_dir) || (rule.anchored && !anchors && rule.segments[0] != "**")
            {
                continue;
            }
            let hit = if rule.anchored {
                match_segments(&rule.segments, &parts)
            } else {
                segment_glob(&rule.segments[0], name)
            };
            if hit {
                return if rule.negated {
                    Match::Whitelist
                } else {
                    Match::Ignore
                };
            }
        }
        Match::None
    }
}

fn parse_rule(line: &str) -> Option<Rule> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    // Trailing spaces are dropped unless escaped.
    let mut text = line.to_string();
    while text.ends_with(' ') && !text.ends_with("\\ ") {
        text.pop();
    }
    if text.is_empty() || text.starts_with('#') {
        return None;
    }
    let negated = text.starts_with('!');
    if negated {
        text.remove(0);
    } else if text.starts_with("\\!") || text.starts_with("\\#") {
        // An escaped `!` or `#` is a literal first character.
        text.remove(0);
    }
    let dir_only = text.ends_with('/');
    if dir_only {
        text.pop();
    }
    if text.is_empty() {
        return None;
    }
    let anchored = text.contains('/');
    let body = text.strip_prefix('/').unwrap_or(&text);
    Some(Rule {
        segments: body.split('/').map(str::to_string).collect(),
        negated,
        dir_only,
        anchored,
    })
}

/// Matches path segments against pattern segments, where `**` stands for zero or more whole segments.
fn match_segments(pattern: &[String], parts: &[&str]) -> bool {
    match pattern.split_first() {
        None => parts.is_empty(),
        Some((first, rest)) if first == "**" => {
            if rest.is_empty() {
                // A trailing `**` matches what is inside, not the directory itself.
                return !parts.is_empty();
            }
            (0..=parts.len()).any(|skip| match_segments(rest, &parts[skip..]))
        }
        Some((first, rest)) => {
            !parts.is_empty() && segment_glob(first, parts[0]) && match_segments(rest, &parts[1..])
        }
    }
}

/// One path segment against one pattern segment: `*`, `?`, `[...]` (with `!`/`^` negation and ranges)
/// and `\` escapes. `**` inside a segment is just `*`.
fn segment_glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob(&p, &t)
}

fn glob(p: &[char], t: &[char]) -> bool {
    match p.split_first() {
        None => t.is_empty(),
        Some(('*', rest)) => {
            let mut rest = rest;
            while let Some(('*', more)) = rest.split_first() {
                rest = more;
            }
            (0..=t.len()).any(|skip| glob(rest, &t[skip..]))
        }
        Some(('?', rest)) => !t.is_empty() && glob(rest, &t[1..]),
        Some(('[', rest)) => {
            let Some(&c) = t.first() else { return false };
            match class(rest, c) {
                Some((hit, after)) => hit && glob(after, &t[1..]),
                // No closing bracket: a literal `[`.
                None => c == '[' && glob(rest, &t[1..]),
            }
        }
        Some(('\\', rest)) => match (rest.split_first(), t.split_first()) {
            (Some((a, rest)), Some((b, t))) => a == b && glob(rest, t),
            _ => false,
        },
        Some((&a, rest)) => t.first() == Some(&a) && glob(rest, &t[1..]),
    }
}

/// A bracket class after its `[`: whether `c` is in it, and the pattern after the `]`.
fn class(p: &[char], c: char) -> Option<(bool, &[char])> {
    let (negated, mut i) = match p.first() {
        Some('!' | '^') => (true, 1),
        _ => (false, 0),
    };
    let start = i;
    let mut hit = false;
    while i < p.len() {
        if p[i] == ']' && i > start {
            return Some((hit != negated, &p[i + 1..]));
        }
        let low = p[i];
        if i + 2 < p.len() && p[i + 1] == '-' && p[i + 2] != ']' {
            hit |= (low..=p[i + 2]).contains(&c);
            i += 3;
        } else {
            hit |= low == c;
            i += 1;
        }
    }
    None
}

/// The layered rules for a walk.
///
/// Rules come from: the global git ignore file, `.git/info/exclude`, and in every directory from the
/// repository root (or just the walk root, outside a repository) down to the directory being listed, its
/// `.gitignore` (only inside a repository) and `.ignore`. All `.ignore` files outrank all `.gitignore`
/// files, deeper files win over shallower ones of the same kind, and the last matching rule within a file
/// wins. A directory
/// that is excluded is not entered, so nothing inside it can be re-included.
#[derive(Debug)]
pub struct Ignorer {
    root: PathBuf,
    git_root: Option<PathBuf>,
    global: Gitignore,
    exclude: Gitignore,
    /// Per directory: its `.gitignore` and its `.ignore`.
    per_directory: HashMap<PathBuf, (Gitignore, Gitignore)>,
    hidden: bool,
    files: bool,
}

fn global_ignore_file() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir).join("git/ignore"));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/git/ignore"))
}

impl Ignorer {
    /// The rules for a walk that starts at `root`; `hidden` skips dot files and `files` applies the ignore
    /// files.
    #[must_use]
    pub fn new(root: &Path, hidden: bool, files: bool) -> Self {
        let absolute = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let git_root = absolute
            .ancestors()
            .find(|dir| dir.join(".git").exists())
            .map(Path::to_path_buf);
        let (mut global, mut exclude) = (Gitignore::default(), Gitignore::default());
        if files {
            if let Some(file) = global_ignore_file() {
                global = Gitignore::from_file(&file);
            }
            if let Some(git) = &git_root {
                exclude = Gitignore::from_file(&git.join(".git/info/exclude"));
            }
        }
        Self {
            root: root.to_path_buf(),
            git_root,
            global,
            exclude,
            per_directory: HashMap::new(),
            hidden,
            files,
        }
    }

    /// The `.gitignore` and `.ignore` of `dir`, loaded once.
    fn directory_rules(&mut self, dir: &Path) -> &(Gitignore, Gitignore) {
        let in_repository = self.git_root.is_some();
        self.per_directory
            .entry(dir.to_path_buf())
            .or_insert_with(|| {
                let git = if in_repository {
                    Gitignore::from_file(&dir.join(".gitignore"))
                } else {
                    Gitignore::default()
                };
                (git, Gitignore::from_file(&dir.join(".ignore")))
            })
    }

    /// Whether a walk should skip `path` (a file or directory found while walking from the root).
    pub fn is_ignored(&mut self, path: &Path, is_dir: bool) -> bool {
        if path == self.root {
            return false;
        }
        if self.hidden
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
        {
            return true;
        }
        if !self.files {
            return false;
        }

        // The directories whose files apply: from the top of the repository (or the walk root) down.
        let absolute = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let top = self
            .git_root
            .clone()
            .unwrap_or_else(|| fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone()));
        let Some(parent) = absolute.parent().map(Path::to_path_buf) else {
            return false;
        };
        // Innermost directory first.
        let dirs: Vec<PathBuf> = parent
            .ancestors()
            .take_while(|d| d.starts_with(&top))
            .map(Path::to_path_buf)
            .collect();
        let relative = |dir: &Path| {
            absolute
                .strip_prefix(dir)
                .map(|r| r.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let decide = |m: Match| match m {
            Match::None => None,
            Match::Ignore => Some(true),
            Match::Whitelist => Some(false),
        };

        // `.ignore` files outrank `.gitignore` files whatever their depth; inside each kind a deeper
        // file outranks a shallower one. Then the repository's exclude file, then the global one.
        for dir in &dirs {
            let rules = self.directory_rules(dir).1.clone();
            if let Some(answer) = decide(rules.matched(&relative(dir), is_dir)) {
                return answer;
            }
        }
        for dir in &dirs {
            let rules = self.directory_rules(dir).0.clone();
            if let Some(answer) = decide(rules.matched(&relative(dir), is_dir)) {
                return answer;
            }
        }
        if let Some(answer) = decide(self.exclude.matched(&relative(&top), is_dir)) {
            return answer;
        }
        decide(
            self.global
                .matched_names(&absolute.to_string_lossy(), is_dir),
        )
        .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(rules: &str, path: &str, dir: bool) -> Match {
        Gitignore::parse(rules).matched(path, dir)
    }

    #[quench_test::test]
    fn names_and_anchoring() {
        assert_eq!(m("target", "target", true), Match::Ignore);
        assert_eq!(m("target", "a/b/target", false), Match::Ignore);
        assert_eq!(m("/target", "target", true), Match::Ignore);
        assert_eq!(m("/target", "a/target", true), Match::None);
        assert_eq!(m("a/b", "a/b", false), Match::Ignore);
        assert_eq!(m("a/b", "x/a/b", false), Match::None);
        assert_eq!(m("build/", "build", true), Match::Ignore);
        assert_eq!(m("build/", "build", false), Match::None);
        assert_eq!(m("", "x", false), Match::None);
        assert_eq!(m("x", "", false), Match::None);
    }

    #[quench_test::test]
    fn globs() {
        assert_eq!(m("*.log", "a/b.log", false), Match::Ignore);
        assert_eq!(m("*.log", "a/b.logx", false), Match::None);
        assert_eq!(m("?.txt", "a.txt", false), Match::Ignore);
        assert_eq!(m("?.txt", "ab.txt", false), Match::None);
        assert_eq!(m("[a-c]x", "bx", false), Match::Ignore);
        assert_eq!(m("[!a-c]x", "bx", false), Match::None);
        assert_eq!(m("[!a-c]x", "dx", false), Match::Ignore);
        assert_eq!(m("[^a]x", "bx", false), Match::Ignore);
        assert_eq!(m("[]]x", "]x", false), Match::Ignore);
        assert_eq!(m("[x", "[x", false), Match::Ignore);
        assert_eq!(m("a*b*c", "aXbYc", false), Match::Ignore);
        assert_eq!(m("a**b", "aXXb", false), Match::Ignore);
        assert_eq!(m("\\*x", "*x", false), Match::Ignore);
        assert_eq!(m("\\*x", "ax", false), Match::None);
        assert_eq!(m("\\", "x", false), Match::None);
    }

    #[quench_test::test]
    fn double_star() {
        assert_eq!(m("**/foo", "foo", false), Match::Ignore);
        assert_eq!(m("**/foo", "a/b/foo", false), Match::Ignore);
        assert_eq!(m("a/**", "a/b/c", false), Match::Ignore);
        assert_eq!(m("a/**", "a", true), Match::None);
        assert_eq!(m("a/**/b", "a/b", false), Match::Ignore);
        assert_eq!(m("a/**/b", "a/x/y/b", false), Match::Ignore);
        assert_eq!(m("a/**/b", "a/x/y/c", false), Match::None);
        assert_eq!(m("/a/*/c", "a/b/c", false), Match::Ignore);
        assert_eq!(m("/a/*/c", "a/b/d/c", false), Match::None);
    }

    #[quench_test::test]
    fn negation_comments_and_escapes() {
        let rules = "*.log\n!keep.log\n# comment\n\\#hash\n\\!bang\ntrailing   \nspaced\\ \n\r\n";
        let g = Gitignore::parse(rules);
        assert_eq!(g.matched("a.log", false), Match::Ignore);
        assert_eq!(g.matched("keep.log", false), Match::Whitelist);
        assert_eq!(g.matched("#hash", false), Match::Ignore);
        assert_eq!(g.matched("!bang", false), Match::Ignore);
        assert_eq!(g.matched("trailing", false), Match::Ignore);
        assert_eq!(g.matched("spaced ", false), Match::Ignore);
        assert_eq!(g.matched("comment", false), Match::None);
        assert!(!g.is_empty());
        assert!(Gitignore::default().is_empty());
        assert!(Gitignore::from_file(Path::new("/nonexistent/ignore")).is_empty());
        assert_eq!(m("/", "x", false), Match::None);
        assert_eq!(m("a\\b", "a/b", false), Match::None);
        assert_eq!(m("dir/", "a\\dir", true), Match::Ignore);
    }

    #[quench_test::test]
    fn global_rules_have_no_directory() {
        let g = Gitignore::parse("/build\nname\n**/deep.txt\nsrc/x\n");
        assert_eq!(g.matched_names("/home/u/p/build", true), Match::None);
        assert_eq!(g.matched_names("/home/u/p/name", false), Match::Ignore);
        assert_eq!(g.matched_names("/home/u/p/deep.txt", false), Match::Ignore);
        assert_eq!(g.matched_names("/home/u/p/src/x", false), Match::None);
    }

    #[quench_test::test]
    fn precedence_between_files() {
        use crate::tmp::TempDir;
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        fs::create_dir_all(p.join(".git")).unwrap();
        fs::create_dir_all(p.join("sub")).unwrap();
        fs::write(p.join(".ignore"), "*.rs\n").unwrap();
        fs::write(p.join("sub/.gitignore"), "!*.rs\n*.md\n").unwrap();
        fs::write(p.join("sub/.ignore"), "!keep.md\n").unwrap();
        for f in ["sub/a.rs", "sub/a.md", "sub/keep.md", "sub/b.txt"] {
            fs::write(p.join(f), "x").unwrap();
        }
        let mut ignorer = Ignorer::new(p, true, true);
        // The root `.ignore` outranks the deeper `.gitignore`'s whitelist.
        assert!(ignorer.is_ignored(&p.join("sub/a.rs"), false));
        // A deeper `.ignore` outranks a `.gitignore` of the same directory.
        assert!(!ignorer.is_ignored(&p.join("sub/keep.md"), false));
        assert!(ignorer.is_ignored(&p.join("sub/a.md"), false));
        assert!(!ignorer.is_ignored(&p.join("sub/b.txt"), false));
        assert!(!ignorer.is_ignored(p, true));
        assert!(ignorer.is_ignored(&p.join("sub/.hidden"), false));
        // With neither filter on, nothing is skipped.
        let mut off = Ignorer::new(p, false, false);
        assert!(!off.is_ignored(&p.join("sub/a.rs"), false));
        assert!(!off.is_ignored(&p.join(".git"), true));
    }
}
