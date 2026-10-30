---
title: i18n API
parent: quench-ui
nav_order: 4
---

Reference of `quench_ui::i18n` (source: `quench-ui/src/i18n.rs`): run-time translations from Fluent (`.ftl`) files, with plural forms and live language switching. Worked examples are in [usage-app](usage-app.md).

`I18n` holds a `Signal` and `Rc`s and is `!Send` and `!Sync`. `Catalog`, `Value`, `Category` and `ParseError` are plain data and `Send + Sync`.

```rust
use quench_ui::i18n::{Catalog, Category, I18n, ParseError, Value, plural_category};
```

## Value

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
}
```

An argument of a message. Conversions (all `From`): `i32`, `u32`, `usize` (cast with `as f64`, precision loss above 2^53), `f64`, `&str`, `String`. So `("n", 5.into())` and `("name", "Ola".into())` work.

`Display`: `Number` uses `{n}` of `f64` (`1.0` prints `1`, `1.5` prints `1.5`, `100000000.0` prints `100000000`); `Text` prints as is. There is no locale-aware number formatting (no grouping, no decimal comma).

## Category

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category { Zero, One, Two, Few, Many, Other }
```

The CLDR plural categories. `plural_category` returns only `One`, `Few`, `Many` and `Other`; `Zero` and `Two` are never produced (no supported language uses them). A selector can still carry a variant keyed `[zero]` or `[two]`; it is just never chosen by category (an exact number key `[0]` or `[2]` works).

## plural_category

```rust
#[must_use]
pub fn plural_category(language: &str, n: f64) -> Category
```

The plural category of `n` in `language` (`en`, `pl-PL`, `pl_PL`, ...). Only the part before the first `-` or `_` is used, case-sensitively (`PL` is not `pl`).

The number is judged by its written form: `n.abs()` is formatted with `{}`, split at `.` into the integer part `i` (parsed as `u64`, 0 if that fails: NaN, infinity or beyond `u64`) and the fraction digits. A number is "with fraction" when the written form has a `.` part, so `1.0` (written `1`) is an integer, `1.5` is not. The sign is ignored (`-1` is like `1`).

| Language | Rule |
|---|---|
| `pl` | with fraction: `Other`. `i == 1`: `One`. `i % 10` in 2..=4 and `i % 100` not in 12..=14: `Few`. Everything else (0, 5..=21, 22.. ending 0,1,5-9, 11-14): `Many`. |
| `ja`, `zh`, `ko`, `vi`, `th` | always `Other`. |
| `fr`, `pt` | `i == 0` (with or without fraction), or `i == 1` without fraction: `One`. Otherwise `Other`. (CLDR's `many` of French and Portuguese for large round numbers is not implemented.) |
| any other language, including `en`, `de`, `ru`, `cs`, `ar`, `es`, `it` | `i == 1` without fraction: `One`. Otherwise `Other`. |

Checked values (from a local run): `pl` gives 0 Many, 1 One, 1.5 Other, 2 Few, 5 Many, 12 Many, 22 Few; `fr` gives 0 One, 1 One, 1.5 Other, 2 Other; `en` gives 0 Other, 1 One, 1.5 Other, 2 Other. Russian, Czech and Arabic use the English rule here, which is **not** their CLDR rule.

## ParseError

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}
impl fmt::Display for ParseError   // "line {line}: {message}"
impl std::error::Error for ParseError
```

`line` is 1-based and is the line where the **message starts** (not the line of the fault inside a multi-line message). `I18n::new` prefixes `message` with `"<language>: "`; `Catalog::parse` does not. The messages that occur: ``unbalanced `}` ``, ``unclosed `{` ``, ``expected `}` ``, `unclosed selector`, ``expected a variant `[key]` ``, ``a selector needs a default variant `*[other]` ``.

## Catalog

```rust
#[derive(Clone, Debug, Default)]
pub struct Catalog { /* private */ }
```

The messages of one language.

* `pub fn parse(source: &str) -> Result<Self, ParseError>`: reads a `.ftl` text (`\r\n` is turned into `\n` first). Errors: see the table below. A later definition of an id replaces an earlier one. Messages whose value is empty are not stored (so `contains` is false and `format` returns `None`).
* `#[must_use] pub fn contains(&self, id: &str) -> bool`: whether message `id` exists. Terms (`-brand`) are not stored.
* `#[must_use] pub fn format(&self, language: &str, id: &str, args: &[(&str, Value)]) -> Option<String>`: the text of message `id`, or `None` if there is no such message. `language` selects the plural rules only. A variable that is not in `args` contributes nothing (the empty string). Never fails.

## The supported Fluent subset

The parser is line-based and hand-written; it is **not** a conforming Fluent parser. Behaviour, exactly:

### Entries

| Source | Result |
|---|---|
| Line starting with an ASCII letter | A message. The id is the text before the first `=` (trimmed). |
| `id = value` | Message; spaces directly after `=` are dropped (`trim_start_matches(' ')`). `id=value` works. |
| Indented lines after a message (a space or tab first, not blank) | Continuation lines of the value: the indentation is removed and the lines are joined with `\n`. Blank lines between continuation lines are kept as empty lines. |
| Lines inside an open `{` (brace depth above 0) | Always part of the entry, indented or not. |
| Blank line, `# comment`, `## group`, `### file` | Skipped (any line not starting with a letter or `-`, outside an entry). |
| Line starting with `-` | A term. Read (so its continuation lines are consumed) but **not stored**. |
| Indented first line (` id = x`) | Not a message (the line must start with a letter in column 0); skipped. |
| Same id twice | The later replaces the earlier. |
| Attribute line (`    .title = ...`) after a message value, outside braces | The attribute and everything indented or blank after it are dropped. |
| Message whose value starts on the next line (`id =` newline, indented text) | Parsed, but the value starts with a `\n` (the newline after `=` is kept): `a =\n    value` gives `"\nvalue"`. See the report. |
| Message with no `=` | Ignored. |

### Placeables (`{ ... }`)

| Syntax | Meaning |
|---|---|
| `{ $name }` | The argument `name` (`Value::to_string`). Missing argument: empty. |
| `{ message-id }` | Another message of the same catalog, formatted with the same arguments. Missing: empty. Nesting depth is limited to 8 (deeper references, including a message that references itself, produce nothing). The id characters are letters, digits, `_`, `-` and `.`; an attribute reference `{ msg.attr }` is looked up as the id `msg.attr` and so is empty. |
| `{ "literal" }` | The text between the quotes, verbatim. No escapes: `\"`, `A` and `\{` are not interpreted, and the literal ends at the next `"`. This is how to write a literal `{` or `}`: `{ "{" }`. |
| `{ 5 }`, `{ 1.5 }` | A number literal printed as written (a word of letters, digits, `_`, `-`, `.`). |
| `{ NUMBER($n) }` | The argument printed like `{ $n }`. Options (`minimumFractionDigits: 2`) are skipped, not applied. |
| `{ $n -> ... }`, `{ NUMBER($n) -> ... }` | A selector (below). |
| `{ -term }` | A term reference: contributes nothing (the term is not defined). |
| `{ OTHER($x) }` (`DATETIME`, `PLATFORM`, anything but `NUMBER`) | Contributes nothing; the arguments are skipped up to `)`. |

A `}` in plain text, with no matching `{`, is an error (`unbalanced }`). A `{` that is never closed is an error. Whitespace around the contents of a placeable is free.

### Selectors

```text
files = { $n ->
    [0] No files
    [one] { $n } file
   *[other] { $n } files
}
```

* Variants are `[key]` or `*[key]` (default). The key is trimmed text up to `]`; keys are not interpreted as Fluent identifiers or numbers by the parser.
* Exactly the default marker is required: one variant must start with `*`, else the parse fails with ``a selector needs a default variant `*[other]` ``. (If several are marked, the last wins.)
* A variant body runs to the next line that begins (after spaces) with `[` or `*[`, or to the selector's closing `}`; trailing spaces and newlines of the body are removed. Variant bodies may contain placeables, including nested selectors.
* **Choice for a number argument**: first a variant whose key parses as `f64` and equals the number (so `[0]`, `[1]`, `[1.5]`); then a variant whose key equals the plural category name of the argument in the **catalog's** language (`zero`, `one`, `two`, `few`, `many`, `other`); otherwise the default variant.
* **Choice for a text argument**: a variant whose key equals the text exactly; otherwise the default. No plural logic.
* **No argument**: the default variant.
* The selector must be introduced by `$name ->` or `NUMBER($name) ->`. A selector on a literal, a message reference or a term (`{ "x" -> ... }`) is a parse error (``expected `}` ``).
* The selector's closing `}` may be followed by more text on the same line (`}tail`).

### Not supported (skipped or rejected)

| Fluent feature | Behaviour here |
|---|---|
| Attributes (`.attr = ...`) | Dropped; `msg.attr` references resolve to empty. |
| Terms (`-term = ...`, `{ -term }`, term attributes and parameters) | Defined terms are discarded; references produce nothing. |
| Functions other than `NUMBER` (`DATETIME`, ...) | Produce nothing. |
| `NUMBER` options and number formatting | Ignored; the value is printed with `f64`'s `Display`. |
| Selectors on message/term references or literals | Parse error. |
| String escapes (`A`, `\"`, `\\`) | Not interpreted. |
| Multi-line text with common-indent removal | Each continuation line is trimmed of all its leading whitespace, not of the common indent. Extra indentation inside a message is lost. |
| Special-character rules (a text line starting with `[`, `*` or `.`) | Not special outside selectors/attributes: a continuation line `.x = y` outside braces is treated as an attribute and dropped. |
| Message references with arguments (`{ msg(arg: 1) }`) | Arguments are skipped; the reference is dropped (a `(` after a name makes it a function call: only `NUMBER` yields output). |
| Resource-level features: `@import`-like, junk recovery | None; a malformed message fails the whole file. |
| Bidi isolation marks around placeables | Not inserted. |

## I18n

```rust
#[derive(Clone)]
pub struct I18n { /* private */ }
impl Debug for I18n   // shows the language
```

The app's translations and the language in use. Cheap to clone; **clones share the language signal** (changing it in one changes it everywhere).

* `pub fn new(files: &[(&str, &str)], initial: &str) -> Result<Self, ParseError>`: catalogs as `(language, ftl text)`, starting in `initial`. The first file given is the fallback language. Create it inside a reactive root (it creates the language signal). `initial` need not be one of the languages given. With no files, every lookup returns the id.
  *Errors:* a file that fails to parse; the error message is prefixed with the language (`"pl: unbalanced }"`), `line` is the line in that file.
* `#[must_use] pub fn language(&self) -> String`: the language in use; a read in a computation follows changes.
* `pub fn set_language(&self, language: &str)`: switches (`set_if_changed`: setting the same language does not notify). It may name a language with no catalog; lookups then fall back.
* `#[must_use] pub fn languages(&self) -> Vec<String>`: the languages given, in order.
* `#[must_use] pub fn t(&self, id: &str) -> String`: `self.tr(id, &[])`.
* `#[must_use] pub fn tr(&self, id: &str, args: &[(&str, Value)]) -> String`: message `id` in the current language, else the fallback.

### Lookup order of `tr`

1. Read the language signal (so a view closure that calls `tr` re-runs on `set_language`).
2. Try the current language, then the fallback (the first catalog's language). For each, find the catalog whose name equals it; failing that, the catalog whose name equals the part before the first `-` or `_` (`pl-PL` finds `pl`; `pl` does **not** find a catalog registered as `pl-PL`).
3. The first catalog that has the message formats it. The `language` given to `format` for plural rules is the **catalog's name** (`pl`), not the requested one.
4. If none has it: return `id` itself (a missing translation shows up on the page instead of failing). A language not among the catalogs falls to the fallback catalog, with the fallback's plural rules.

Example:

```rust
use quench_ui::i18n::I18n;

let (_, _scope) = quench_ui::create_root(|| {
    let i18n = I18n::new(
        &[
            ("en", "files = { $n ->\n    [one] { $n } file\n   *[other] { $n } files\n}\nbye = Bye"),
            ("pl", "files = { $n ->\n    [one] { $n } plik\n    [few] { $n } pliki\n    [many] { $n } plików\n   *[other] { $n } pliku\n}"),
        ],
        "pl",
    )
    .unwrap();
    assert_eq!(i18n.tr("files", &[("n", 2.into())]), "2 pliki");
    assert_eq!(i18n.t("bye"), "Bye");            // pl lacks it: falls back to en
    i18n.set_language("en");
    assert_eq!(i18n.tr("files", &[("n", 2.into())]), "2 files");
    assert_eq!(i18n.t("missing"), "missing");     // unknown id comes back as itself
});
```
