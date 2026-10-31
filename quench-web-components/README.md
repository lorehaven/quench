# quench-web-components
Reusable UI components for `quench-web`.

## What it is
`quench-web-components` is a small set of reusable widget builders on top of the `Element` of `quench-web`: buttons, cards, panels, containers, form fields, status badges and alert boxes, plus the shell components re-exported from `quench-web`.

Each builder only produces an `Element` with class names; it carries no styling of its own. `Element` and the tag helpers come from `quench-web`, so add that as a dependency too.

## Use cases
- Buttons with primary, secondary and danger variants.
- Cards holding a title, content and footer.
- Forms with inputs, selects and textareas.
- Status badges and alert boxes for service dashboards.

## Example
```rust
use quench_web_components::containers::Card;
use quench_web_components::status::success_badge;

fn widget() -> quench_web::Element {
    Card::new()
        .title("Deployment")
        .content(success_badge("healthy"))
        .build()
}
```

A form:

```rust
use quench_web::prelude::*;
use quench_web_components::{buttons::primary_button, forms::*};

let f = form()
    .attr("method", "post")
    .attr("action", "/users")
    .child(FormInput::new("name").label("Name").required(true).build())
    .child(FormSelect::new("role").label("Role").option("admin", "Administrator").selected("admin").build())
    .child(primary_button("Create").attr("type", "submit"));
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-web-components/
