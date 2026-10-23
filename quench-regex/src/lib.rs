//! Regular expressions: the part of the `regex` crate's syntax and API the tools use, with Unicode-aware
//! `\w`, `\d`, `\s`, `\b` and case-insensitive matching, captures, and replacement.
//!
//! ```
//! use quench_regex::Regex;
//!
//! let re = Regex::new(r"(?P<year>\d{4})-(?P<month>\d{2})").unwrap();
//! let caps = re.captures("released 2026-10, patched 2026-11").unwrap();
//! assert_eq!(&caps["year"], "2026");
//! assert_eq!(re.replace_all("2026-10 2026-11", "$month/$year"), "10/2026 11/2026");
//! ```
//!
//! Matching is by a Pike VM, so the time is proportional to the text times the pattern and no pattern can
//! take exponential time. Not supported (as in `regex`): backreferences and look-around. Also not supported
//! here: `\p{..}` property classes, set operations inside `[..]`, and inline-flag `R`/`u`.

mod class;
mod compile;
mod error;
mod parse;
mod unicode;
mod vm;

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::ops::{Index, Range};
use std::sync::Arc;

pub use error::Error;

use compile::Program;
use vm::NONE;

/// A compiled regular expression. Cloning is cheap.
#[derive(Clone)]
pub struct Regex {
    pattern: Arc<str>,
    program: Arc<Program>,
    names: Arc<HashMap<String, usize>>,
}

impl Regex {
    /// Compiles `pattern`.
    ///
    /// # Errors
    ///
    /// [`Error`] describing what is wrong with the pattern.
    pub fn new(pattern: &str) -> Result<Self, Error> {
        RegexBuilder::new(pattern).build()
    }

    /// The pattern this was compiled from.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.pattern
    }

    /// Whether the pattern matches anywhere in `text`.
    #[must_use]
    pub fn is_match(&self, text: &str) -> bool {
        vm::search(&self.program, text, 0, true).is_some()
    }

    /// The leftmost match.
    #[must_use]
    pub fn find<'t>(&self, text: &'t str) -> Option<Match<'t>> {
        self.find_at(text, 0)
    }

    /// The leftmost match starting at or after byte offset `start` (assertions still see the whole text).
    #[must_use]
    pub fn find_at<'t>(&self, text: &'t str, start: usize) -> Option<Match<'t>> {
        let slots = vm::search(&self.program, text, start, false)?;
        Some(Match {
            text,
            start: slots[0],
            end: slots[1],
        })
    }

    /// Every non-overlapping match, left to right.
    #[must_use]
    pub const fn find_iter<'r, 't>(&'r self, text: &'t str) -> Matches<'r, 't> {
        Matches {
            searcher: Searcher::new(self, text),
        }
    }

    /// The leftmost match with its capture groups.
    #[must_use]
    pub fn captures<'t>(&self, text: &'t str) -> Option<Captures<'t>> {
        self.captures_at(text, 0)
    }

    /// Like [`captures`](Self::captures), starting at byte offset `start`.
    #[must_use]
    pub fn captures_at<'t>(&self, text: &'t str, start: usize) -> Option<Captures<'t>> {
        let slots = vm::search(&self.program, text, start, false)?;
        Some(Captures {
            text,
            slots,
            names: Arc::clone(&self.names),
        })
    }

    /// Every non-overlapping match with its groups.
    #[must_use]
    pub const fn captures_iter<'r, 't>(&'r self, text: &'t str) -> CaptureMatches<'r, 't> {
        CaptureMatches {
            searcher: Searcher::new(self, text),
        }
    }

    /// Number of capture groups, counting the whole match as group 0.
    #[must_use]
    pub fn captures_len(&self) -> usize {
        self.program.slots / 2
    }

    /// The name of each group in order (`None` for unnamed ones; group 0 is always unnamed).
    pub fn capture_names(&self) -> impl Iterator<Item = Option<&str>> + '_ {
        (0..self.captures_len()).map(|i| {
            self.program
                .names
                .iter()
                .find(|(_, index)| *index == i)
                .map(|(name, _)| name.as_str())
        })
    }

    /// Replaces the first match.
    pub fn replace<'t, R: Replacer>(&self, text: &'t str, replacement: R) -> Cow<'t, str> {
        self.replacen(text, 1, replacement)
    }

    /// Replaces every match.
    pub fn replace_all<'t, R: Replacer>(&self, text: &'t str, replacement: R) -> Cow<'t, str> {
        self.replacen(text, 0, replacement)
    }

    /// Replaces at most `limit` matches (0 means all).
    pub fn replacen<'t, R: Replacer>(
        &self,
        text: &'t str,
        limit: usize,
        mut replacement: R,
    ) -> Cow<'t, str> {
        if let Some(literal) = replacement.no_expansion() {
            let mut matches = self.find_iter(text).peekable();
            if matches.peek().is_none() {
                return Cow::Borrowed(text);
            }
            let mut out = String::with_capacity(text.len());
            let mut last = 0;
            for (count, m) in matches.enumerate() {
                if limit > 0 && count >= limit {
                    break;
                }
                out.push_str(&text[last..m.start()]);
                out.push_str(&literal);
                last = m.end();
            }
            out.push_str(&text[last..]);
            return Cow::Owned(out);
        }
        let mut captures = self.captures_iter(text).peekable();
        if captures.peek().is_none() {
            return Cow::Borrowed(text);
        }
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for (count, caps) in captures.enumerate() {
            if limit > 0 && count >= limit {
                break;
            }
            let Some(whole) = caps.get(0) else { continue };
            out.push_str(&text[last..whole.start()]);
            replacement.replace_append(&caps, &mut out);
            last = whole.end();
        }
        out.push_str(&text[last..]);
        Cow::Owned(out)
    }

    /// The pieces of `text` between matches.
    #[must_use]
    pub const fn split<'r, 't>(&'r self, text: &'t str) -> Split<'r, 't> {
        Split {
            matches: self.find_iter(text),
            text,
            last: 0,
            done: false,
        }
    }
}

impl fmt::Debug for Regex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Regex({:?})", self.pattern)
    }
}

impl fmt::Display for Regex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.pattern)
    }
}

impl std::str::FromStr for Regex {
    type Err = Error;

    fn from_str(pattern: &str) -> Result<Self, Error> {
        Self::new(pattern)
    }
}

/// Builds a [`Regex`] with options set before the pattern's own flags apply.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct RegexBuilder {
    pattern: String,
    case_insensitive: bool,
    multi_line: bool,
    dot_matches_new_line: bool,
    ignore_whitespace: bool,
    swap_greed: bool,
}

impl RegexBuilder {
    #[must_use]
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            case_insensitive: false,
            multi_line: false,
            dot_matches_new_line: false,
            ignore_whitespace: false,
            swap_greed: false,
        }
    }

    /// Like `(?i)` at the start of the pattern.
    pub const fn case_insensitive(&mut self, yes: bool) -> &mut Self {
        self.case_insensitive = yes;
        self
    }

    /// `^` and `$` match at line breaks (`(?m)`).
    pub const fn multi_line(&mut self, yes: bool) -> &mut Self {
        self.multi_line = yes;
        self
    }

    /// `.` matches `\n` too (`(?s)`).
    pub const fn dot_matches_new_line(&mut self, yes: bool) -> &mut Self {
        self.dot_matches_new_line = yes;
        self
    }

    /// Whitespace and `#` comments in the pattern mean nothing (`(?x)`).
    pub const fn ignore_whitespace(&mut self, yes: bool) -> &mut Self {
        self.ignore_whitespace = yes;
        self
    }

    /// `*` is lazy and `*?` greedy (`(?U)`).
    pub const fn swap_greed(&mut self, yes: bool) -> &mut Self {
        self.swap_greed = yes;
        self
    }

    /// # Errors
    ///
    /// [`Error`] describing what is wrong with the pattern.
    pub fn build(&self) -> Result<Regex, Error> {
        let parsed = parse::parse(
            &self.pattern,
            parse::Options {
                case_insensitive: self.case_insensitive,
                multi_line: self.multi_line,
                dot_matches_new_line: self.dot_matches_new_line,
                ignore_whitespace: self.ignore_whitespace,
                swap_greed: self.swap_greed,
            },
        )?;
        let program = compile::compile(parsed)?;
        let names = program.names.iter().cloned().collect();
        Ok(Regex {
            pattern: self.pattern.as_str().into(),
            program: Arc::new(program),
            names: Arc::new(names),
        })
    }
}

/// `text` with every regex metacharacter escaped, to match it literally.
#[must_use]
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A match: where in the text, and the text itself.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Match<'t> {
    text: &'t str,
    start: usize,
    end: usize,
}

impl<'t> Match<'t> {
    /// Byte offset of the first character.
    #[must_use]
    pub const fn start(&self) -> usize {
        self.start
    }

    /// Byte offset just past the last character.
    #[must_use]
    pub const fn end(&self) -> usize {
        self.end
    }

    #[must_use]
    pub const fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.end - self.start
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }

    #[must_use]
    pub fn as_str(&self) -> &'t str {
        &self.text[self.start..self.end]
    }
}

impl fmt::Debug for Match<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Match")
            .field("start", &self.start)
            .field("end", &self.end)
            .field("string", &self.as_str())
            .finish()
    }
}

impl<'t> From<Match<'t>> for &'t str {
    fn from(m: Match<'t>) -> &'t str {
        m.as_str()
    }
}

impl<'t> From<Match<'t>> for Range<usize> {
    fn from(m: Match<'t>) -> Self {
        m.range()
    }
}

/// The groups of one match.
#[derive(Clone)]
pub struct Captures<'t> {
    text: &'t str,
    slots: Vec<usize>,
    names: Arc<HashMap<String, usize>>,
}

impl<'t> Captures<'t> {
    /// Group `index` (0 is the whole match), if it took part in the match.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<Match<'t>> {
        let start = *self.slots.get(2 * index)?;
        let end = *self.slots.get(2 * index + 1)?;
        (start != NONE && end != NONE).then_some(Match {
            text: self.text,
            start,
            end,
        })
    }

    /// The group called `name`.
    #[must_use]
    pub fn name(&self, name: &str) -> Option<Match<'t>> {
        self.get(*self.names.get(name)?)
    }

    /// Number of groups, counting the whole match.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.slots.len() / 2
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Every group in order.
    pub fn iter(&self) -> impl Iterator<Item = Option<Match<'t>>> + '_ {
        (0..self.len()).map(|i| self.get(i))
    }

    /// Appends `template` to `dst`, replacing `$1`, `${1}`, `$name`, `${name}` with the groups (unknown or
    /// unmatched ones give nothing) and `$$` with a dollar sign.
    pub fn expand(&self, template: &str, dst: &mut String) {
        let bytes = template.as_bytes();
        let mut i = 0;
        let mut literal_from = 0;
        while i < bytes.len() {
            if bytes[i] != b'$' {
                i += 1;
                continue;
            }
            dst.push_str(&template[literal_from..i]);
            i += 1;
            match bytes.get(i) {
                Some(b'$') => {
                    dst.push('$');
                    i += 1;
                }
                Some(b'{') => {
                    if let Some(close) = template[i + 1..].find('}') {
                        self.push_group(&template[i + 1..i + 1 + close], dst);
                        i += close + 2;
                    } else {
                        dst.push_str("${");
                        i += 1;
                    }
                }
                Some(b) if b.is_ascii_alphanumeric() || *b == b'_' => {
                    let end = template[i..]
                        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                        .map_or(template.len(), |n| i + n);
                    self.push_group(&template[i..end], dst);
                    i = end;
                }
                _ => dst.push('$'),
            }
            literal_from = i;
        }
        dst.push_str(&template[literal_from..]);
    }

    fn push_group(&self, reference: &str, dst: &mut String) {
        let group = reference
            .parse::<usize>()
            .ok()
            .and_then(|i| self.get(i))
            .or_else(|| self.name(reference));
        if let Some(m) = group {
            dst.push_str(m.as_str());
        }
    }
}

impl fmt::Debug for Captures<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl Index<usize> for Captures<'_> {
    type Output = str;

    /// # Panics
    ///
    /// When the group did not take part in the match.
    fn index(&self, index: usize) -> &str {
        self.get(index)
            .map_or_else(|| panic!("no group at index '{index}'"), |m| m.as_str())
    }
}

impl Index<&str> for Captures<'_> {
    type Output = str;

    /// # Panics
    ///
    /// When there is no such group or it did not take part in the match.
    fn index(&self, name: &str) -> &str {
        self.name(name)
            .map_or_else(|| panic!("no group named '{name}'"), |m| m.as_str())
    }
}

/// Finds successive matches, skipping an empty match right after the previous one.
struct Searcher<'r, 't> {
    regex: &'r Regex,
    text: &'t str,
    next_start: usize,
    last_end: Option<usize>,
}

impl<'r, 't> Searcher<'r, 't> {
    const fn new(regex: &'r Regex, text: &'t str) -> Self {
        Self {
            regex,
            text,
            next_start: 0,
            last_end: None,
        }
    }

    fn next_slots(&mut self) -> Option<Vec<usize>> {
        if self.next_start > self.text.len() {
            return None;
        }
        let mut slots = vm::search(&self.regex.program, self.text, self.next_start, false)?;
        if slots[0] == slots[1] && Some(slots[1]) == self.last_end {
            // An empty match touching the previous one: look again one character further on.
            let step = self.text[self.next_start..]
                .chars()
                .next()
                .map_or(1, char::len_utf8);
            self.next_start += step;
            if self.next_start > self.text.len() {
                return None;
            }
            slots = vm::search(&self.regex.program, self.text, self.next_start, false)?;
        }
        self.next_start = slots[1];
        self.last_end = Some(slots[1]);
        Some(slots)
    }
}

/// Iterator for [`Regex::find_iter`].
pub struct Matches<'r, 't> {
    searcher: Searcher<'r, 't>,
}

impl<'t> Iterator for Matches<'_, 't> {
    type Item = Match<'t>;

    fn next(&mut self) -> Option<Match<'t>> {
        let text = self.searcher.text;
        let slots = self.searcher.next_slots()?;
        Some(Match {
            text,
            start: slots[0],
            end: slots[1],
        })
    }
}

impl fmt::Debug for Matches<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Matches(..)")
    }
}

/// Iterator for [`Regex::captures_iter`].
pub struct CaptureMatches<'r, 't> {
    searcher: Searcher<'r, 't>,
}

impl<'t> Iterator for CaptureMatches<'_, 't> {
    type Item = Captures<'t>;

    fn next(&mut self) -> Option<Captures<'t>> {
        let text = self.searcher.text;
        let slots = self.searcher.next_slots()?;
        Some(Captures {
            text,
            slots,
            names: Arc::clone(&self.searcher.regex.names),
        })
    }
}

impl fmt::Debug for CaptureMatches<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CaptureMatches(..)")
    }
}

/// Iterator for [`Regex::split`].
pub struct Split<'r, 't> {
    matches: Matches<'r, 't>,
    text: &'t str,
    last: usize,
    done: bool,
}

impl<'t> Iterator for Split<'_, 't> {
    type Item = &'t str;

    fn next(&mut self) -> Option<&'t str> {
        if self.done {
            return None;
        }
        if let Some(m) = self.matches.next() {
            let piece = &self.text[self.last..m.start()];
            self.last = m.end();
            Some(piece)
        } else {
            self.done = true;
            Some(&self.text[self.last..])
        }
    }
}

impl fmt::Debug for Split<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Split(..)")
    }
}

/// What to put where a match was found.
pub trait Replacer {
    /// Appends the replacement for one match to `dst`.
    fn replace_append(&mut self, caps: &Captures<'_>, dst: &mut String);

    /// The replacement when it is the same text every time, with no `$` references.
    fn no_expansion(&mut self) -> Option<Cow<'_, str>> {
        None
    }
}

impl Replacer for &str {
    fn replace_append(&mut self, caps: &Captures<'_>, dst: &mut String) {
        caps.expand(self, dst);
    }

    fn no_expansion(&mut self) -> Option<Cow<'_, str>> {
        (!self.contains('$')).then_some(Cow::Borrowed(*self))
    }
}

impl Replacer for &String {
    fn replace_append(&mut self, caps: &Captures<'_>, dst: &mut String) {
        self.as_str().replace_append(caps, dst);
    }

    fn no_expansion(&mut self) -> Option<Cow<'_, str>> {
        if self.contains('$') {
            None
        } else {
            Some(Cow::Borrowed(self.as_str()))
        }
    }
}

impl Replacer for String {
    fn replace_append(&mut self, caps: &Captures<'_>, dst: &mut String) {
        self.as_str().replace_append(caps, dst);
    }

    fn no_expansion(&mut self) -> Option<Cow<'_, str>> {
        (!self.contains('$')).then_some(Cow::Borrowed(self.as_str()))
    }
}

impl Replacer for Cow<'_, str> {
    fn replace_append(&mut self, caps: &Captures<'_>, dst: &mut String) {
        self.as_ref().replace_append(caps, dst);
    }

    fn no_expansion(&mut self) -> Option<Cow<'_, str>> {
        if self.contains('$') {
            None
        } else {
            Some(Cow::Borrowed(&**self))
        }
    }
}

impl<F, T> Replacer for F
where
    F: FnMut(&Captures<'_>) -> T,
    T: AsRef<str>,
{
    fn replace_append(&mut self, caps: &Captures<'_>, dst: &mut String) {
        dst.push_str((*self)(caps).as_ref());
    }
}

/// A replacement used exactly as written, `$` included.
#[derive(Debug, Clone, Copy)]
pub struct NoExpand<'t>(pub &'t str);

impl Replacer for NoExpand<'_> {
    fn replace_append(&mut self, _caps: &Captures<'_>, dst: &mut String) {
        dst.push_str(self.0);
    }

    fn no_expansion(&mut self) -> Option<Cow<'_, str>> {
        Some(Cow::Borrowed(self.0))
    }
}

#[cfg(test)]
mod tests;
