use crate::snapshot;
use quench_markdown_term::Options;

const TABLE: &str = "| Left | Centre | Right |\n|:-----|:------:|------:|\n| a | b | 1 |\n| a longer cell that will need to wrap somewhere | short | 22.5 |\n";

/// Alignments honoured; the widest column shrinks and wraps when the table does not fit.
#[quench_test::test]
fn table() {
    snapshot(
        TABLE,
        &Options {
            width: 40,
            ..Options::default()
        },
        r#"
┌─────────────────────┬────────┬───────┐
│ Left                │ Centre │ Right │
├─────────────────────┼────────┼───────┤
│ a                   │   b    │     1 │
│ a longer cell that  │ short  │  22.5 │
│ will need to wrap   │        │       │
│ somewhere           │        │       │
└─────────────────────┴────────┴───────┘
"#,
    );
    snapshot(
        TABLE,
        &Options {
            width: 80,
            ..Options::default()
        },
        r#"
┌────────────────────────────────────────────────┬────────┬───────┐
│ Left                                           │ Centre │ Right │
├────────────────────────────────────────────────┼────────┼───────┤
│ a                                              │   b    │     1 │
│ a longer cell that will need to wrap somewhere │ short  │  22.5 │
└────────────────────────────────────────────────┴────────┴───────┘
"#,
    );
    snapshot(
        TABLE,
        &Options {
            width: 120,
            ..Options::default()
        },
        r#"
┌────────────────────────────────────────────────┬────────┬───────┐
│ Left                                           │ Centre │ Right │
├────────────────────────────────────────────────┼────────┼───────┤
│ a                                              │   b    │     1 │
│ a longer cell that will need to wrap somewhere │ short  │  22.5 │
└────────────────────────────────────────────────┴────────┴───────┘
"#,
    );
}

const TABLE_TINY: &str = "| Name | Value | Notes |\n|------|-------|-------|\n| alpha | 1 | the first |\n| beta | 2 | the second |\n";

/// A table that fits keeps its natural widths.
#[quench_test::test]
fn table_tiny() {
    snapshot(
        TABLE_TINY,
        &Options {
            width: 40,
            ..Options::default()
        },
        r#"
┌───────┬───────┬────────────┐
│ Name  │ Value │ Notes      │
├───────┼───────┼────────────┤
│ alpha │ 1     │ the first  │
│ beta  │ 2     │ the second │
└───────┴───────┴────────────┘
"#,
    );
    snapshot(
        TABLE_TINY,
        &Options {
            width: 80,
            ..Options::default()
        },
        r#"
┌───────┬───────┬────────────┐
│ Name  │ Value │ Notes      │
├───────┼───────┼────────────┤
│ alpha │ 1     │ the first  │
│ beta  │ 2     │ the second │
└───────┴───────┴────────────┘
"#,
    );
    snapshot(
        TABLE_TINY,
        &Options {
            width: 120,
            ..Options::default()
        },
        r#"
┌───────┬───────┬────────────┐
│ Name  │ Value │ Notes      │
├───────┼───────┼────────────┤
│ alpha │ 1     │ the first  │
│ beta  │ 2     │ the second │
└───────┴───────┴────────────┘
"#,
    );
}

/// When even the narrowest columns do not fit, rows are stacked as `header: value`.
#[quench_test::test]
fn table_stacked() {
    snapshot(
        TABLE_TINY,
        &Options {
            width: 20,
            ..Options::default()
        },
        r#"
Name: alpha
Value: 1
Notes: the first
────────────────────
Name: beta
Value: 2
Notes: the second
"#,
    );
    snapshot(
        TABLE_TINY,
        &Options {
            width: 24,
            ..Options::default()
        },
        r#"
Name: alpha
Value: 1
Notes: the first
────────────────────────
Name: beta
Value: 2
Notes: the second
"#,
    );
}
