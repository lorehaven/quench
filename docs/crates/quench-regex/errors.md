---
title: Errors
parent: quench-regex
grand_parent: Crates
nav_order: 3
---

`Regex::new`, `FromStr` and `RegexBuilder::build` return `Result<_, Error>`. Matching itself cannot fail.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error { /* message: String, offset: Option<usize> */ }

impl Error {
    #[must_use] pub fn message(&self) -> &str          // what is wrong, without the position
    #[must_use] pub const fn offset(&self) -> Option<usize> // character offset into the pattern, if it has a place
}
impl fmt::Display for Error
impl std::error::Error for Error   // no source()
```

`Display`: `regex parse error at {offset}: {message}` when there is an offset, otherwise `regex error: {message}`. The offset counts characters (not bytes) and is where the parser stopped, which is usually just after the offending character.

## Messages

| Message | Cause |
|---|---|
| `unexpected end` | the pattern ended where an atom was needed (internal guard) |
| `unclosed group` | `(` without `)`; also `(?i` |
| `unopened group` | `)` without `(` |
| `groups are nested too deeply` | more than 250 nested groups |
| `repetition operator missing its expression` | `*`, `+`, `?` or `{` where an atom was expected |
| `repetition quantifier expects a decimal number` | `{x}`, `{2,x}`, `{}` |
| `unclosed counted repetition` | `a{2` |
| `repetition count is too large` | a count above 1000 |
| `repetition quantifier has its minimum above its maximum` | `{3,2}` |
| `` a lone `\` `` | a backslash at the end |
| `unrecognized escape sequence` | `\` + ASCII letter/digit that is not an escape (`\y`, `\1`, `\p`) |
| `invalid hexadecimal escape` | bad or out-of-range `\x`, `\u`, `\U` |
| `unclosed character class` | `[` without `]` |
| `class range is out of order` | `[b-a]` |
| `a class shorthand cannot end a range` | `[a-\d]` |
| `set operations in a class are not supported` | `&&`, `~~`, `--` at a class member start |
| `malformed POSIX class` | `[[:alpha]` |
| `unknown POSIX class` | `[[:nope:]]` |
| `` expected `<` after `(?P` `` | `(?Pa)` |
| `invalid capture group name` | empty, non-alphanumeric/underscore, or starting with a digit |
| `duplicate capture group name` | the same name twice |
| `unrecognized flag` | a letter other than `imsxU`, a second `-`, or `(?=`, `(?!` |
| `the compiled pattern would be too large` | more than 200 000 instructions after expansion of counted repeats (no offset: `regex error: ...`) |
