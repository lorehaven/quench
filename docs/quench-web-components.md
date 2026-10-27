# Quench Web Components

`quench-web-components` (crate `quench_web_components`) is a small library of reusable, higher-level UI builders (buttons, cards/panels/containers, form fields, status badges/alerts) layered on top of `quench-web`'s `Element` builder. Its own README is a short pointer to this page. It is used by the forge services `conveyor`, `gatehouse`, `switchboard` and `warehouse`, which call `containers::empty_state`. Its `components` and `dom` modules are re-exports of the ones in `quench-web`, kept so existing `quench_web_components::components::...` paths still work.

## Public API / Key Types

Reachable via `quench_web_components::prelude::*` (a subset) or the crate root:

- `buttons` — `ButtonVariant` (`Primary`, `Secondary`, `Danger`, `Success`, `Warning`, `Outline`), `ButtonBuilder` (`.variant()`/`.primary()`/`.secondary()`/etc., `.disabled()`, `.button_type()`, `.id()`, `.build() -> Element`), and helpers `primary_button`, `secondary_button`, `danger_button`, `success_button`, `warning_button`, `outline_button`.
- `containers` — `Card` (title/content/footer/`.compact()`), `Panel` (title/content/`.bordered()`/`.highlighted()`), `Container` (`.fluid()`/`.centered()`), plus helpers `card`, `compact_card`, `panel`, `highlighted_panel`.
- `forms` — `FormInput`, `FormSelect`, `FormTextarea` builders, each producing a labeled `.form-group` wrapper `Element` when `.label()` is set, or a bare field otherwise.
- `status` — `StatusLevel` (`Info`, `Success`, `Warning`, `Error`), `StatusBadge` (`.with_icon()`), `AlertBox` (`.closeable()`), plus helpers `success_badge`, `warning_badge`, `error_badge`, `info_alert`, `success_alert`, `warning_alert`, `error_alert`.
- `components` — `HeaderBuilder`, `FooterBuilder`, `NavPanelBuilder`, `nav_button()`, `locale_switch()`: re-exports of `quench-web`'s (the crate used to carry its own near-identical copies).
- `dom` — re-exports of `quench_web::dom`'s `on_dom_ready`, `toggle_modal`, `update_from_select`, `set_select_value` (used internally by `NavPanelBuilder`; no additional behavior on top of `quench-web`'s copies).

## Testing

Unit tests under `tests/unit` cover every builder (about 99% line coverage).

## Usage example

```rust
use quench_web_components::containers::Card;
use quench_web_components::status::success_badge;

fn build_widget() -> quench_web::Element {
    Card::new()
        .title("Deployment")
        .content(success_badge("healthy"))
        .build()
}
```

[Home](../README.md)
