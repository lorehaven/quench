---
title: API reference
parent: quench-regex
grand_parent: Crates
nav_order: 2
---

Every public item of `quench_regex`, in the order of `src/lib.rs`. All `Regex` methods take `&self` and need no mutable state; `Regex` is `Send + Sync` (its fields are `Arc`s of plain data). No `unsafe`, no feature gates.

## `struct Regex`

`#[derive(Clone)]`, cheap to clone. `Debug` prints `Regex("pattern")`; `Display` prints the pattern; `FromStr` (`Err = Error`) calls `Regex::new`.

```rust
impl Regex {
    pub fn new(pattern: &str) -> Result<Self, Error>
    #[must_use] pub fn as_str(&self) -> &str
    #[must_use] pub fn is_match(&self, text: &str) -> bool
    #[must_use] pub fn find<'t>(&self, text: &'t str) -> Option<Match<'t>>
    #[must_use] pub fn find_at<'t>(&self, text: &'t str, start: usize) -> Option<Match<'t>>
    #[must_use] pub const fn find_iter<'r, 't>(&'r self, text: &'t str) -> Matches<'r, 't>
    #[must_use] pub fn captures<'t>(&self, text: &'t str) -> Option<Captures<'t>>
    #[must_use] pub fn captures_at<'t>(&self, text: &'t str, start: usize) -> Option<Captures<'t>>
    #[must_use] pub const fn captures_iter<'r, 't>(&'r self, text: &'t str) -> CaptureMatches<'r, 't>
    #[must_use] pub fn captures_len(&self) -> usize
    pub fn capture_names(&self) -> impl Iterator<Item = Option<&str>> + '_
    pub fn replace<'t, R: Replacer>(&self, text: &'t str, replacement: R) -> Cow<'t, str>
    pub fn replace_all<'t, R: Replacer>(&self, text: &'t str, replacement: R) -> Cow<'t, str>
    pub fn replacen<'t, R: Replacer>(&self, text: &'t str, limit: usize, replacement: R) -> Cow<'t, str>
    #[must_use] pub const fn split<'r, 't>(&'r self, text: &'t str) -> Split<'r, 't>
}
```

| Method | Behaviour |
|---|---|
| `new(pattern)` | `RegexBuilder::new(pattern).build()`. Errors: see [errors](errors.md). |
| `as_str()` | the pattern text as given. |
| `is_match(text)` | whether the pattern matches anywhere. Stops at the first match found (no captures). |
| `find(text)` | the leftmost match, leftmost-first priority (like Perl/`regex`). |
| `find_at(text, start)` | leftmost match starting at or after byte offset `start`; assertions (`^`, `\b`, ...) still see the whole text. **Panics** if `start > text.len()` or `start` is not on a character boundary. A pattern anchored at the start (`^`, `\A`, in every alternative) never matches when `start > 0`. |
| `find_iter(text)` | every non-overlapping match, left to right. An empty match immediately after the previous match is skipped (the search steps one character on). |
| `captures(text)` / `captures_at(text, start)` | the leftmost match with groups; `_at` as `find_at`. |
| `captures_iter(text)` | like `find_iter`, with groups. |
| `captures_len()` | number of groups including group 0. |
| `capture_names()` | one item per group in order; `None` for unnamed groups and group 0. |
| `replace(text, r)` | replaces the first match: `replacen(text, 1, r)`. |
| `replace_all(text, r)` | every match: `replacen(text, 0, r)`. |
| `replacen(text, limit, r)` | at most `limit` matches (0 means all). Returns `Cow::Borrowed(text)` when nothing matched. |
| `split(text)` | the pieces of `text` between matches (see `Split`). |

Replacement expansion happens through the [`Replacer`](#trait-replacer). When the replacer reports `no_expansion` (a `&str`/`String`/`Cow` without `$`, or `NoExpand`), `replacen` walks `find_iter` and splices the literal text; otherwise it walks `captures_iter` and calls `replace_append` for each match.

## `struct RegexBuilder`

`#[derive(Debug, Clone)]`. Options set before the pattern's own flags apply (so `(?-i)` in the pattern still turns the option off for the rest).

```rust
impl RegexBuilder {
    #[must_use] pub fn new(pattern: &str) -> Self
    pub const fn case_insensitive(&mut self, yes: bool) -> &mut Self
    pub const fn multi_line(&mut self, yes: bool) -> &mut Self
    pub const fn dot_matches_new_line(&mut self, yes: bool) -> &mut Self
    pub const fn ignore_whitespace(&mut self, yes: bool) -> &mut Self
    pub const fn swap_greed(&mut self, yes: bool) -> &mut Self
    pub fn build(&self) -> Result<Regex, Error>
}
```

| Method | Same as flag |
|---|---|
| `case_insensitive` | `(?i)` at the start |
| `multi_line` | `(?m)`: `^` and `$` match at line breaks |
| `dot_matches_new_line` | `(?s)` |
| `ignore_whitespace` | `(?x)` |
| `swap_greed` | `(?U)` |

All default to `false`. Setters take and return `&mut Self`, so chain them on a `let mut` or on the temporary: `RegexBuilder::new(p).multi_line(true).build()`. `build` can be called repeatedly.

## `fn escape`

```rust
#[must_use]
pub fn escape(text: &str) -> String
```

`text` with a backslash before every character in `\ . + * ? ( ) | [ ] { } ^ $ # & - ~`. The result matches `text` literally, also in verbose mode. Other characters, spaces included, are left alone (so in verbose mode `(?x)` an escaped space still needs `\ `, which `escape` does not add).

## `struct Match<'t>`

`#[derive(Clone, Copy, PartialEq, Eq)]`; `Debug` shows `start`, `end`, `string`. A span of the searched text.

```rust
impl<'t> Match<'t> {
    #[must_use] pub const fn start(&self) -> usize
    #[must_use] pub const fn end(&self) -> usize
    #[must_use] pub const fn range(&self) -> Range<usize>
    #[must_use] pub const fn len(&self) -> usize
    #[must_use] pub const fn is_empty(&self) -> bool
    #[must_use] pub fn as_str(&self) -> &'t str
}
impl<'t> From<Match<'t>> for &'t str
impl<'t> From<Match<'t>> for Range<usize>
```

`start`/`end` are byte offsets into the searched text; `end` is exclusive. `PartialEq` compares the searched text (by content) and both offsets.

## `struct Captures<'t>`

`#[derive(Clone)]`; `Debug` prints a list of `Option<Match>`. The groups of one match.

```rust
impl<'t> Captures<'t> {
    #[must_use] pub fn get(&self, index: usize) -> Option<Match<'t>>
    #[must_use] pub fn name(&self, name: &str) -> Option<Match<'t>>
    #[must_use] pub const fn len(&self) -> usize
    #[must_use] pub const fn is_empty(&self) -> bool
    pub fn iter(&self) -> impl Iterator<Item = Option<Match<'t>>> + '_
    pub fn expand(&self, template: &str, dst: &mut String)
}
impl Index<usize> for Captures<'_>   // Output = str
impl Index<&str> for Captures<'_>    // Output = str
```

* `get(i)`: group `i` (0 is the whole match) if it took part in the match.
* `name(n)`: the group called `n`.
* `len()`: number of groups including group 0 (`slots / 2`). `is_empty()` is true only if there are no slots (never for a real match).
* `iter()`: every group in order.
* `expand(template, dst)`: appends `template` to `dst`, replacing `$1`, `${1}`, `$name`, `${name}` with the groups; an unknown or unmatched group gives nothing; `$$` gives `$`; `$` followed by anything else is kept as `$`; `${` without `}` is kept as `${`. A bare `$ref` extends over the longest run of ASCII letters, digits and `_`. A reference that parses as a number is looked up by index, else by name.
* `caps[i]`, `caps["name"]`: the group's text. **Panics** with `no group at index '{i}'` or `no group named '{name}'` when the group did not take part or does not exist.

## Iterators

```rust
pub struct Matches<'r, 't>      // Iterator<Item = Match<'t>>      ; Debug prints "Matches(..)"
pub struct CaptureMatches<'r, 't> // Iterator<Item = Captures<'t>>   ; Debug prints "CaptureMatches(..)"
pub struct Split<'r, 't>        // Iterator<Item = &'t str>        ; Debug prints "Split(..)"
```

They are made by `find_iter`, `captures_iter` and `split`; they cannot be constructed otherwise. `Split` yields the text before each match, then the rest: `n` matches give `n + 1` items (empty strings included; a match at the start yields a leading `""`; no matches yields the whole text).

## `trait Replacer`

```rust
pub trait Replacer {
    fn replace_append(&mut self, caps: &Captures<'_>, dst: &mut String);
    fn no_expansion(&mut self) -> Option<Cow<'_, str>> { None }
}
```

What to put where a match was found. `replace_append` appends the replacement for one match. `no_expansion` returns `Some(text)` when the replacement is the same every time and has no `$` references (an optimisation; the default is `None`).

Provided implementations:

| Type | Behaviour |
|---|---|
| `&str`, `&String`, `String`, `Cow<'_, str>` | `caps.expand(self, dst)`; `no_expansion` is the text itself when it contains no `$`. |
| `F: FnMut(&Captures<'_>) -> T`, `T: AsRef<str>` | appends `f(caps)`; no `no_expansion`. |
| `NoExpand<'t>` | appends the text as written. |

## `struct NoExpand<'t>`

```rust
#[derive(Debug, Clone, Copy)]
pub struct NoExpand<'t>(pub &'t str);
```

A replacement used exactly as written, `$` included.

## `struct Error`

See [errors](errors.md): `message()`, `offset()`, `Display`, `std::error::Error`; `#[derive(Debug, Clone, PartialEq, Eq)]`.

## Private modules

`class`, `compile`, `parse`, `unicode`, `vm` are not public.
