# quench-regex
Regular expressions with Unicode classes, captures and replacement, no dependencies.

## What it is
`quench-regex` is a regular-expression engine with no dependencies. It replaces the `regex` crate in the workspace tools, and its API follows `regex` (`Regex`, `RegexBuilder`, `Match`, `Captures`, `find_iter`, `replace_all`, `split`, `escape`, `Replacer`, `NoExpand`), so a tool switches by changing its imports.

Matching is a Pike VM: time is proportional to the text length times the program size for every pattern, never exponential. It works on `&str` only, with byte offsets. `quench-schema` uses it for pattern keywords.

## Use cases
- Test and find matches in text with byte offsets.
- Extract numbered or named capture groups.
- Rewrite text with `$name` replacements or a closure.
- Split text on a pattern.

## Example
```rust
use quench_regex::Regex;

let re = Regex::new(r"(?P<year>\d{4})-(?P<month>\d{2})").unwrap();
let caps = re.captures("released 2026-10, patched 2026-11").unwrap();
assert_eq!(&caps["year"], "2026");
assert_eq!(re.replace_all("2026-10 2026-11", "$month/$year"), "10/2026 11/2026");

let numbers: Vec<&str> = Regex::new(r"\d+").unwrap()
    .find_iter("a1 b22 c333").map(|m| m.as_str()).collect();
assert_eq!(numbers, ["1", "22", "333"]);
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-regex/
