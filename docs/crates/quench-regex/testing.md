---
title: Testing
parent: quench-regex
grand_parent: Crates
nav_order: 6
---

`quench-regex` uses `harness = false` with `quench_test::main!()` (see [quench-test](../quench-test/index.md)). The tests are in `src/tests.rs` plus small ones in `class.rs`:

| Test | Covers |
|---|---|
| `literals_classes_and_dots` | literals, classes, `.` |
| `unicode_classes_and_boundaries` | `\w \d \s \b` on non-ASCII |
| `case_insensitivity_follows_unicode` | Kelvin sign, sigma, etc. |
| `anchors_and_multiline` | `^ $ \A \z`, `(?m)` |
| `repetition_greedy_and_lazy` | greedy, lazy, counted |
| `groups_and_captures`, `matches_and_their_conversions` | groups, names, `Match` conversions |
| `replacement_forms`, `splitting` | `replace*`, `$` expansion, `NoExpand`, closures, `split` |
| `empty_matches_do_not_stick_to_the_previous_match` | iterator rule |
| `alternation_and_nesting`, `verbose_mode_ignores_layout`, `builder_flags`, `escaping` | |
| `bad_patterns_are_rejected_with_a_place` | all error messages in [errors](errors.md) |
| `patterns_that_could_blow_up_other_engines_stay_fast` | `(a+)+$`, `(a\|aa)*b`, `a{1000}` |
| `literal_prefixes_and_anchors_are_only_shortcuts` | prefix and anchor shortcut correctness |

Run: `cargo test -p quench-regex`.
