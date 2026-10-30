---
title: Pattern syntax
parent: quench-regex
grand_parent: Crates
nav_order: 4
---

The syntax is that of the `regex` crate, minus a few constructs. Everything below is read from `src/parse.rs`. A pattern is a sequence of characters; positions in errors are character (not byte) offsets.

## Literals and escapes

Any character other than the metacharacters `\ . + * ? ( ) | [ ] { } ^ $` matches itself. In verbose mode (`x`) `#` also starts a comment and whitespace is ignored.

| Escape | Matches |
|---|---|
| `\n` `\t` `\r` | newline, tab, carriage return |
| `\f` `\v` `\a` | U+000C, U+000B, U+0007 |
| `\xHH` | the character with two hex digits |
| `\x{H..}` | any number of hex digits in braces |
| `\uHHHH`, `\u{H..}` | four hex digits, or braced |
| `\UHHHHHHHH`, `\U{H..}` | eight hex digits, or braced |
| `\` + ASCII punctuation or space | that character (`\.`, `\-`, `\ `, `\#`, `\/`) |
| `\` + a non-ASCII character | that character |

Any other `\` + ASCII letter or digit is an error: `unrecognized escape sequence`. That is how the unsupported `\1` (backreference), `\p{..}` / `\P{..}`, `\Q..\E`, `\h`, `\Z`, `\G`, `\k<name>` are rejected. A value that is not a Unicode scalar (`\x{110000}`, surrogates) is `invalid hexadecimal escape`. A lone `\` at the end is `` a lone `\` ``.

## Classes of characters

| Syntax | Meaning |
|---|---|
| `.` | any character except `\n`; with flag `s`, any character |
| `[abc]`, `[a-z]` | a set of characters and ranges |
| `[^abc]` | the complement (over all of Unicode, `\n` included) |
| `[]abc]`, `[^]abc]` | a `]` first in the class is a literal |
| `[a-]` | a `-` last is a literal |
| `[[a-c][x-z]]` | nested bracket classes are a union |
| `[[:alpha:]]`, `[[:^alpha:]]` | ASCII POSIX classes (see below), negated with `^` |
| `\d` `\D` | Unicode decimal digits (`Nd`) and their complement |
| `\s` `\S` | Unicode whitespace and complement |
| `\w` `\W` | Unicode word characters (letters, marks, digits, connector punctuation, ...) and complement |

`\d \D \s \S \w \W` may also appear inside `[..]`; a shorthand cannot be the end of a range (`[a-\d]` is `a class shorthand cannot end a range`). A range whose end is before its start is `class range is out of order`. Escapes inside a class (`\n`, `\x41`, `\]`, `\-`) work as outside.

The tables behind `\w`, `\d`, `\s` are generated from the `regex` crate's definitions ("Unicode 16 era" per the header of `src/unicode.rs`). `\d` matches `٣` (Arabic-Indic three); `\s` matches U+00A0.

POSIX classes (ASCII only): `alnum`, `alpha`, `ascii`, `blank`, `cntrl`, `digit`, `graph`, `lower`, `print`, `punct`, `space`, `upper`, `word`, `xdigit`. An unknown name is `unknown POSIX class`; `[[:alpha]` is `malformed POSIX class`.

Not supported: set operations. `&&`, `~~` or `--` where a class member starts give `set operations in a class are not supported` (for instance `[a&&b]`, `[a~~b]`; `[a--b]` is read as the range `a` to `-` and fails as `class range is out of order`).

## Assertions

| Syntax | Meaning |
|---|---|
| `^` | start of text; with `m`, also after every `\n` |
| `$` | end of text; with `m`, also before every `\n` |
| `\A` | start of text (always) |
| `\z` | end of text (always) |
| `\b` | word boundary (Unicode `\w`) |
| `\B` | not a word boundary |

`$` without `m` matches only at the very end of the text, **not** before a final `\n`. `\Z` does not exist. Assertions can be repeated (`^*`) and may be quantified.

## Groups

| Syntax | Meaning |
|---|---|
| `(...)` | capturing group, numbered by its opening parenthesis from 1 |
| `(?:...)` | non-capturing |
| `(?P<name>...)`, `(?<name>...)` | named capturing group; the name is non-empty, made of letters, digits and `_`, and does not start with an ASCII digit; names are unique (`duplicate capture group name`) |
| `(?flags)` | sets flags for the rest of the enclosing group (or pattern); matches nothing and cannot be quantified |
| `(?flags:...)` | flags for this group only |

Flags: `i` case-insensitive, `m` multi-line, `s` dot matches newline, `x` verbose, `U` swap greediness. `(?i-m)` turns `i` on and `m` off; one `-` turns off every flag that follows (a second `-` is `unrecognized flag`). Any other letter is `unrecognized flag`. A flag state set inside a group ends at the group's `)`.

Not supported: look-around (`(?=`, `(?!`, `(?<=`, `(?<!` all fail: `(?=` and `(?!` as `unrecognized flag`; `(?<=`/`(?<!` as `invalid capture group name`), atomic groups, conditionals, comments `(?#..)`, `(?P=name)` (`` expected `<` after `(?P` ``). There are no backreferences.

Group nesting is limited to 250 (`groups are nested too deeply`).

## Alternation

`a|b|c`; the first alternative that leads to a match wins (leftmost-first, like Perl and `regex`): `a|ab` on `ab` matches `a`. An empty alternative is allowed (`a|`).

## Repetition

| Syntax | Meaning |
|---|---|
| `*` `+` `?` | 0 or more, 1 or more, 0 or 1 |
| `{n}` `{n,}` `{n,m}` | exactly n, at least n, between n and m |
| each followed by `?` | lazy (fewest first) |

Repetition counts are at most 1000 (`repetition count is too large`); `{3,2}` is `repetition quantifier has its minimum above its maximum`; `{x}` / `{2,x}` is `repetition quantifier expects a decimal number`; `a{2` is `unclosed counted repetition`. A quantifier with nothing before it (`*`, `+a`, `{2}`) is `repetition operator missing its expression`. **A `{` that does not start a valid quantifier is an error, not a literal** (`a{x}` fails); write `\{`. A closing `}` alone is a literal. Repeating a repeat (`a**`, `a+?`-then-`+`) is allowed.

Flag `U` swaps the greediness of every quantifier (`*` lazy, `*?` greedy). In verbose mode, whitespace before a quantifier is skipped; between `{n,m}` and its lazy `?` as well.

The compiled program may have at most 200 000 instructions; `((a{1000}){1000}){1000}` fails with `the compiled pattern would be too large`.

## Case-insensitive matching

With flag `i`, a literal character matches every member of its Unicode simple case-folding "orbit" (`k` matches `K` and the Kelvin sign U+212A; `σ` matches `Σ` and `ς`; `s` matches `S` and the long s U+017F), and a bracketed class is extended with the orbits of all its members. The negation is applied after folding (`(?i)[^k]` excludes `K` and U+212A too). Shorthands (`\w` etc.) are unchanged.

## Summary of what is NOT supported

| Construct | Result |
|---|---|
| backreferences `\1`, `\k<n>`, `(?P=n)` | error |
| look-ahead, look-behind | error |
| `\p{..}`, `\P{..}`, `\pL` property classes | `unrecognized escape sequence` |
| set operations in classes | `set operations in a class are not supported` |
| `\Q..\E`, `\Z`, `\G`, `\h`, `\R` | `unrecognized escape sequence` |
| inline comments `(?#..)`, atomic groups, possessive quantifiers | error |
| byte (non-UTF-8) matching | not available: only `&str` |
| flags `R`, `u`, `n` | `unrecognized flag` |
