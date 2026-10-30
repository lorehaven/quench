---
title: Testing
parent: quench-web
grand_parent: Crates
nav_order: 9
---

## How the crate is tested

Tests are in `tests/unit/` (one file per source module, wired from `tests/unit.rs`), using `quench-test` as a custom harness (`harness = false`, `#[quench_test::test]`).

| File | Covers |
|---|---|
| `html_builder_element_tests.rs`, `html_builder_elements_tests.rs` | `Element` methods and the tag helpers |
| `html_builder_page_tests.rs` | `Link`, `Script`, `js!`, `PageBuilder`, `pretty_print_html` (indentation, escaping, preformatted tags) |
| `framework_app_builder_tests.rs`, `framework_app_shell_tests.rs`, `framework_app_scripts_tests.rs` | `AppBuilder`, `AppShellBuilder` (header/nav switches, theme fallback, missing locale panic and `try_build` error), the script generators |
| `framework_components_*_tests.rs`, `framework_dom_tests.rs` | header, footer, nav panel, locale switch, DOM snippets |
| `golden_baseline_tests.rs` | byte-for-byte comparison of the generated stylesheets, `translations.js`, the favicon and two full pages (`basic.html`, `service.html`) with `tests/fixtures/baseline/` |
| `legacy_paths_tests.rs` | compile-time check that every pre-`quench-style` path still resolves |

Run:

```bash
cargo test -p quench-web
```

After an intended change to generated output, regenerate the golden files and review the diff:

```bash
QUENCH_UPDATE_GOLDEN=1 cargo test -p quench-web golden
```

`AppShellBuilder::build` writes `dist/assets` in the working directory, so the shell tests create `quench-web/dist/` when run from the crate directory.

## Testing code that uses the crate

Render and assert on strings:

```rust
use quench_web::prelude::*;

#[test]
fn page_contains_title() {
    let html = div().child(h1().text("Hi")).render();
    assert_eq!(html, "<div><h1>Hi</h1></div>");
}
```

For full pages use `AppBuilder` (no disk writes) with explicit `supported_locales(vec!["en-US".into()])` so the test does not depend on the working directory's `i18n/`. `tests/unit/golden_baseline_tests.rs` has a `shell_page` helper that does exactly this.
