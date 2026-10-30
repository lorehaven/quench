---
title: Internals
parent: quench-regex
grand_parent: Crates
nav_order: 5
---

## Pipeline

1. `parse` turns the pattern into a tree (`Node`: `Empty`, `Char`, `Class`, `Any`, `Assert`, `Concat`, `Alternate`, `Repeat`, `Group`) and collects group names. Options and flags are resolved here (case folding turns literals into classes; `^`/`$` become line or text assertions).
2. `compile` produces a program of instructions `Char`, `Class`, `Any`, `Split`, `Jmp`, `Save`, `Assert`, `Match` (limit 200 000). Counted repeats are expanded (`x{n,m}` is `n` copies then `m-n` nested optionals), which is why the repeat limit is 1000 and the total size is bounded. A class with exactly one member becomes a `Char`. A nullable body in `x*` keeps its empty pass, as RE2 and `regex` compile it, so priorities agree with them.
3. `vm::search` runs a Pike VM: all threads advance over the text in lock step, each with its capture slots, in priority order. A program counter reached twice at one position keeps the higher-priority thread. When a thread reaches `Match`, lower-priority threads are cut and the search continues only with higher-priority ones, which yields leftmost-first semantics.

## Complexity

Time is `O(text length × program size)` for every pattern and input; there is no backtracking, so `(a+)+$` on 5000 `a`s followed by `b` and `(a|aa)*b` are fast (both are in the tests). Memory per search is two thread lists of the program size times the slot count. `search` allocates its working buffers on every call, so many tiny searches pay a fixed cost; compile once, search many times.

The code comment says about performance on a 555 KB page (from the previous documentation, not re-measured): 1-20 ms against 0.1-0.7 ms for `regex`.

## Shortcuts

* **Literal prefix**: when the pattern begins with literal characters, the search jumps to the next occurrence of that text (via `str::find`) whenever no thread is alive. Only a shortcut; results are identical.
* **Anchored start**: a pattern that can match only at text start (`^`/`\A` without multi-line, in every alternative) is tried at position 0 only; `find_at(text, n > 0)` returns `None`.
* **`is_match`** stops at the first `Match` reached.
* `find_iter` and friends do not restart state between matches except for `next_start`; an empty match touching the previous one is skipped by retrying one character further.

## Assertions

`^` is position 0 (or after `\n` in multi-line), `$` is the text end (or before `\n` in multi-line). `\b` compares whether the character before and the one after are word characters (`class::is_word`: ASCII fast path, otherwise the `\w` table).

## Unicode tables

`src/unicode.rs` (about 2300 lines) holds `WORD`, `DIGIT`, `SPACE` range tables and `ORBITS`, the simple case-folding equivalence classes. They are generated from the `regex` crate's own definitions and "must not be edited by hand". Case folding is *simple* folding only: no multi-character folds (`ß` does not match `SS`).

## Limits

| Limit | Value |
|---|---|
| counted repetition | 1000 |
| group nesting | 250 |
| program size | 200 000 instructions |
| input | any `&str`; offsets are `usize` byte offsets |

## Differences from the `regex` crate

From the code and the crate documentation: no `\p{..}`, no set operations in classes, no inline flags `u`/`R`, no `bytes` API, no `RegexSet`, no `Regex::shortest_match`, no `split_n`/`splitn`, no `captures_read`, no size-limit builder options. Differential testing against `regex` is described in the previous docs (3.2 million generated cases) but no such harness was found in this repository (unverified); the behaviour above is what the unit tests assert.
