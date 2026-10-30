---
title: Testing
parent: quench-web-components
grand_parent: Crates
nav_order: 5
---

Tests live in `tests/unit/`, wired from `tests/unit.rs` with the `quench-test` harness (`harness = false` for the test target).

| File | Covers |
|---|---|
| `buttons_tests.rs` | variants, class names, `disabled`, `type`, `id`, helper functions |
| `containers_tests.rs` | `Card`, `Panel`, `Container`, helpers, `empty_state` |
| `forms_tests.rs` | `FormInput`, `FormSelect`, `FormTextarea` with and without label |
| `status_tests.rs` | `StatusLevel`, `StatusBadge` (icon only with a level), `AlertBox`, helpers |
| `components_header_tests.rs`, `components_footer_tests.rs`, `components_nav_tests.rs`, `dom_tests.rs` | the re-exported shell components and DOM helpers |
| `legacy_paths_tests.rs` | the `components` / `dom` paths that used to be local copies still resolve |

```bash
cargo test -p quench-web-components
```

To test code that uses the widgets, call `.render()` on the built `Element` and compare strings; attribute order is alphabetical, so equality checks are stable:

```rust
let html = quench_web_components::buttons::primary_button("Go").render();
assert_eq!(html, r#"<button class="btn btn-primary" type="button">Go</button>"#);
```
