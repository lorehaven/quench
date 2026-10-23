# Quench Regex

`quench-regex` (crate `quench_regex`) replaces the `regex` crate for the Forge tools. It has no dependencies. The API follows `regex` (`Regex`, `RegexBuilder`, `Match`, `Captures`, `find_iter`, `captures_iter`, `replace`/`replace_all`/`replacen`, `split`, `escape`, `Replacer`, `NoExpand`), so a tool switches by changing its imports.

```rust
use quench_regex::Regex;

let re = Regex::new(r"(?P<year>\d{4})-(?P<month>\d{2})")?;
let caps = re.captures("released 2026-10, patched 2026-11").unwrap();
assert_eq!(&caps["year"], "2026");
assert_eq!(re.replace_all("2026-10 2026-11", "$month/$year"), "10/2026 11/2026");
```

## Syntax

Literals and escapes (`\n \t \r \f \v \a`, `\xHH`, `\x{H..}`, `\uHHHH`, `\u{..}`, `\UHHHHHHHH`, escaped punctuation); `.`; classes `[a-z]`, `[^..]`, nested `[[a-b][x-z]]`, ASCII `[[:alpha:]]`/`[[:^alpha:]]` and shorthands inside classes; `\d \D \s \S \w \W` (Unicode: `\d` is the `Nd` category, `\w` and `\s` follow Unicode's definitions, taken from the `regex` crate's tables); assertions `^ $ \A \z \b \B`; groups `(..)`, `(?:..)`, `(?P<name>..)`, `(?<name>..)`; alternation; repetition `* + ? {n} {n,} {n,m}` and the lazy `?` forms (a repetition limit of 1000); flags `i m s x U` as `(?i)`, `(?-i)`, `(?i:..)` and through `RegexBuilder`. Case-insensitive matching uses Unicode simple case folding (`(?i)k` matches the Kelvin sign, `σ` matches `ς`).

Not supported, and rejected with an error that says so: backreferences, look-around, `\p{..}` property classes, set operations (`&&`, `--`, `~~`) inside a class. Matching text is `&str` only.

## Matching

A pattern compiles to a small program that a Pike VM runs over the text, keeping one thread per possible position in the pattern. The time is proportional to text length times pattern size for every pattern, so there is no input that makes a pattern take exponential time (`(a+)+$` on thousands of `a`s is fine). Matches follow leftmost-first semantics like Perl and `regex` (`a|ab` matches `a`), captures record the winning path's groups, and `find_iter` skips an empty match that touches the previous match.

A pattern that starts with plain characters jumps to the next occurrence of that text before it starts a match, and a pattern anchored with `\A` or `^` is tried at the start only.

On a 555 KB web page the tool's own patterns (script/style stripping, e-mail, phone, URL, citation markers) take 1-20 ms here against 0.1-0.7 ms for `regex`'s automata; that is the price of a 600-line engine instead of a dependency tree, and it is fine for what the services do with it. A lazily built DFA would close most of the gap if a caller ever needs it.

## Testing

Unit tests next to the code, and a differential run against `regex` on 3.2 million generated (pattern, text) cases: random patterns over classes, groups, lazy and counted repeats, anchors, flags and Unicode text, comparing `is_match`, every `find_iter` span, the capture groups of the first match, and `replace_all`. Identical on all of them, including which patterns are rejected.

[Home](../README.md)
