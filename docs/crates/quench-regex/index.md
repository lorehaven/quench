---
title: quench-regex
parent: Crates
has_children: true
permalink: /crates/quench-regex/
nav_order: 39
---

`quench-regex` (library `quench_regex`) is a regular-expression engine with no dependencies. It replaces the `regex` crate in the tools of the workspace: the API follows `regex` (`Regex`, `RegexBuilder`, `Match`, `Captures`, `find_iter`, `captures_iter`, `replace`/`replace_all`/`replacen`, `split`, `escape`, `Replacer`, `NoExpand`), so a tool switches by changing its imports. Matching is a Pike VM: the time is proportional to the text length times the program size for every pattern, never exponential.

## Facts

| | |
|---|---|
| Version | 0.1.2 |
| Edition | 2024 |
| License | MIT |
| Features | none |
| Dependencies | none (dev: `quench-test`) |
| Text type | `&str` only (UTF-8; offsets are byte offsets) |
| Test harness | `harness = false`, `quench_test::main!()` |

```toml
[dependencies]
quench-regex = { version = "0.1.2", registry = "ennor" }
```

## Where to go next

* [Usage](usage.md): finding, capturing, replacing, splitting, builder flags.
* [Pattern syntax](syntax.md): every construct that is supported and every one that is not, with the error each unsupported one gives.
* [API reference](api.md)
* [Errors](errors.md)
* [Internals](internals.md): the matcher, limits, prefix shortcut, case folding, performance.
* [Testing](testing.md)

## Minimal example

```rust
use quench_regex::Regex;

let re = Regex::new(r"(?P<year>\d{4})-(?P<month>\d{2})").unwrap();
let caps = re.captures("released 2026-10, patched 2026-11").unwrap();
assert_eq!(&caps["year"], "2026");
assert_eq!(re.replace_all("2026-10 2026-11", "$month/$year"), "10/2026 11/2026");
```
