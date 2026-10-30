---
title: Testing
parent: quench-template
grand_parent: Crates
nav_order: 7
---
{% raw %}
`quench-template` has `autotests = false`; the only test target is `tests/template.rs` (`harness = false`, `quench_test::main!()`), which covers: text, variables and comments; whitespace control and `raw`; expressions; control flow; `set`/`with`/blocks; filters; tests; methods; macros, imports and includes; macro argument errors; environment hooks; errors with place. The doctest in `src/lib.rs` is the example on the [overview](index.md).

```bash
cargo test -p quench-template
```

The helper `render(source, &json!({...}))` in the test file (an `Environment::new().render_str(..)`) is the pattern to follow in your own tests: assert on the exact string.
{% endraw %}
