---
title: Gherkin and expression syntax
parent: quench-test-bdd
grand_parent: Crates
nav_order: 4
---

## Feature files

Parsed by `gherkin::parse` (English keywords only; files with the extension `.feature`).

```text
@tag1 @tag2
Feature: Name
  Free text description,
  until the first keyword.

  Background:
    Given something shared

  Scenario: Name            (also: Example: Name)
    Given a step
    When another
    Then result
    And more                (And / But / * take the role of the previous step)

  Scenario Outline: <n> items   (also: Scenario Template:)
    When I add <n>
    Examples:               (also: Scenarios:)
      | n |
      | 1 |

  Rule: Name
    Background:
      Given only for this rule
    Scenario: In a rule
      ...
```

| Element | Notes |
|---|---|
| Comments | lines starting with `#` (after indentation) are skipped, also between steps and inside tables |
| Tags | `@name` words on lines before `Feature`, `Rule`, `Scenario`, `Scenario Outline` and `Examples`; stored without the `@`; a `#` starts a comment after tags |
| Description | free text after `Feature:`, `Rule:`, `Scenario:` or `Examples:` until the first step, keyword or table row |
| Steps | `Given`, `When`, `Then`, `And`, `But`, `*` followed by a space; the value is the rest of the line, trimmed |
| Doc string | a line of `"""` or ```` ``` ```` after a step, up to the same fence on its own line; the indentation of the opening fence is removed from each line |
| Data table | lines starting with `|` after a step or under `Examples`; cells are trimmed; `\|` is a pipe, `\n` a newline, other `\x` stay as written |
| Background | steps run before every scenario of the feature, or of the rule (feature background first, then the rule's) |
| Outline | needs at least one `Examples` (else the error `` a scenario outline needs `Examples` ``); a plain scenario with `Examples` is an error (`` `Examples` belong to a scenario outline ``); every row of the table below its header row becomes a scenario with `<column>` replaced in the name, step text, doc strings and table cells; the tags of the `Examples` block are added to the scenario's tags |
| Rule | scenarios inside get the rule's and the feature's tags |

Not supported: other languages, `Rule` inside `Rule`, scenarios after an `Examples` block without a new `Scenario`. A line that is none of the above is `` unexpected `<line>` ``. Errors: `ParseError { file, line, message }` displayed as `path:line: message` (or `line N: message`): `the file has no feature`, `` expected `Feature:`, found `...` ``, `` expected a scenario, found `...` ``, `a doc string is never closed`, and the outline errors above. A file that cannot be parsed counts in `Summary::parse_errors` and fails the run.

All tags of a scenario for filtering are: feature tags, then rule tags, then the scenario's own tags (then the `Examples` tags for outline rows).

## Step expressions (cucumber expressions)

The pattern of `#[given("...")]` is compiled to a regular expression anchored at both ends (`^...$`) and must match the **whole** step text.

| Syntax | Meaning |
|---|---|
| plain text | literal (regex metacharacters are escaped) |
| `{int}` | `[-+]?\d+` |
| `{float}` | an optionally signed decimal number, with optional exponent (`.5`, `1.`, `2e-3`) |
| `{word}` | one or more non-whitespace characters |
| `{string}` | text in double or single quotes; `\"` `\'` `\\` inside are unescaped; the captured text excludes the quotes |
| `{}` | anything (`(.*)`) |
| `text(optional)` | the parenthesised text is optional: `cucumber(s)` |
| `a/b` | alternation within a word: `is/are`; `/` inside `(...)` or `{...}` is literal |
| `\(`, `\{`, `\/`, `\\` | an escaped character is literal |

An unknown `{name}` is an error `` unknown parameter type `{name}` ``; an unclosed `(` or `{` is `` an optional `(` is never closed `` / `` a parameter `{` is never closed ``. Space between words separates alternation groups (alternation `a/b` is only within one word).

## Regex steps

`#[given(regex = r"...")]` uses [quench-regex](../quench-regex/index.md) syntax and is **not** anchored automatically: it matches if it matches anywhere, so write `^...$`. Every capture group is an argument, in order (an unmatched group gives an empty string).

## Tag expressions

For `--tags` (`Options::tags`): `tag`, `@tag`, `not X`, `X and Y`, `X or Y`, parentheses. `not` binds tighter than `and`, which binds tighter than `or`. Tags are compared without the `@`. Errors: `the tag expression ends early`, ``a `(` is never closed``, `` unexpected `x` ``.

```text
@fast and not @slow
@a or (@b and @c)
```

The tag `serial` (written `@serial`) additionally means: run this scenario alone, after all concurrent ones.
