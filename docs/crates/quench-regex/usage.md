---
title: Usage
parent: quench-regex
grand_parent: Crates
nav_order: 1
---

## 1. Test and find

```rust
use quench_regex::Regex;

let re = Regex::new(r"\b\w+@\w+\.com\b").unwrap();
assert!(re.is_match("write to bob@example.com today"));
let m = re.find("write to bob@example.com today").unwrap();
assert_eq!((m.start(), m.end(), m.as_str()), (9, 24, "bob@example.com"));
```

Offsets are byte offsets into the `&str`. `Regex` is cheap to clone (it holds `Arc`s) and is `Send + Sync`; compile once and reuse (compiling is not cached).

## 2. Every match

```rust
use quench_regex::Regex;

let re = Regex::new(r"\d+").unwrap();
let numbers: Vec<&str> = re.find_iter("a1 b22 c333").map(|m| m.as_str()).collect();
assert_eq!(numbers, ["1", "22", "333"]);
```

Matches do not overlap; an empty match that touches the end of the previous match is skipped.

## 3. Captures, numbered and named

```rust
use quench_regex::Regex;

let re = Regex::new(r"(?P<key>\w+)=(?P<value>[^;]*)(;|$)").unwrap();
let caps = re.captures("a=1;b=two").unwrap();
assert_eq!(&caps["key"], "a");
assert_eq!(&caps[2], "1");
assert_eq!(caps.get(3).unwrap().as_str(), ";");
assert!(caps.name("nope").is_none());
for caps in re.captures_iter("a=1;b=two") {
    println!("{} -> {}", &caps["key"], &caps["value"]);
}
```

`caps[i]` and `caps["name"]` **panic** when the group did not take part in the match or does not exist; use `get`/`name` when a group is optional.

## 4. Replace

```rust
use quench_regex::{NoExpand, Regex};

let re = Regex::new(r"(?P<y>\d{4})-(?P<m>\d{2})").unwrap();
assert_eq!(re.replace("2026-10", "$m/$y"), "10/2026");
assert_eq!(re.replace_all("2026-10 2026-11", "${m}.${y}"), "10.2026 11.2026");
assert_eq!(re.replace_all("2026-10", "$$m"), "$m");
assert_eq!(re.replace_all("2026-10", NoExpand("$m")), "$m");
assert_eq!(re.replacen("2026-10 2026-11 2026-12", 2, "x"), "x x 2026-12");
let shout = Regex::new(r"[a-z]+").unwrap().replace_all("ab cd", |c: &quench_regex::Captures<'_>| c[0].to_uppercase());
assert_eq!(shout, "AB CD");
```

In a `&str` replacement `$1`, `${1}`, `$name`, `${name}` insert a group; `$$` is a dollar. `$name` takes the longest run of letters, digits and `_`, so `$m.` works but `$ma` looks up the group `ma` (unknown groups give nothing); write `${m}a`. The result is `Cow<str>`: borrowed (no allocation) when nothing matched.

## 5. Split

```rust
use quench_regex::Regex;

let re = Regex::new(r"\s*,\s*").unwrap();
let parts: Vec<&str> = re.split("a , b,c").collect();
assert_eq!(parts, ["a", "b", "c"]);
```

## 6. Flags and the builder

```rust
use quench_regex::{Regex, RegexBuilder};

assert!(Regex::new("(?i)straße").unwrap().is_match("STRASSE") == false);
let re = RegexBuilder::new("^abc$").case_insensitive(true).multi_line(true).build().unwrap();
assert!(re.is_match("x\nABC\ny"));
```

Builder flags apply before the pattern's own `(?..)` flags and can be changed by them (`(?-i)`).

## 7. Escape user text

```rust
use quench_regex::{escape, Regex};

let word = "a.b*c";
let re = Regex::new(&format!("^{}$", escape(word))).unwrap();
assert!(re.is_match("a.b*c") && !re.is_match("aXbbc"));
```

## 8. Continuing from an offset

```rust
use quench_regex::Regex;

let re = Regex::new(r"\bfoo\b").unwrap();
let text = "foo foo";
let second = re.find_at(text, 1).unwrap();
assert_eq!(second.start(), 4);
```

`find_at(text, start)` starts at a byte offset but assertions still see the whole text (`\b` at `start` looks at the character before it). `start` must be at most `text.len()` and on a character boundary or the call panics (it slices the text).

## 9. Parse with `FromStr`

```rust
use quench_regex::Regex;
let re: Regex = r"^v\d+$".parse().unwrap();
assert_eq!(re.to_string(), r"^v\d+$");
assert_eq!(format!("{re:?}"), r#"Regex("^v\\d+$")"#);
```
