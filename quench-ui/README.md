# quench-ui
Platform-independent UI core: fine-grained signals, a view tree, renderers and events.

## What it is
`quench-ui` is the platform-independent core of the Quench UI: fine-grained reactivity (signals, memos, effects, scopes), a view tree, typed events and the `Renderer` trait that a backend implements. It replaces a UI framework such as Leptos or Yew. It has no dependencies and no `unsafe` code, and it builds for `wasm32-unknown-unknown` and for native hosts.

Nothing in the core knows about the DOM. Backends plug in behind `Renderer`: `quench-ui-web` for the browser, `quench-ui-native` for a self-drawing native backend, the in-memory `TestRenderer` in this crate, and `render_to_string` for server-side rendering. Everything runs on one thread.

## Use cases
- Build reactive UIs from signals, memos and effects.
- Write views once and render them in the browser, natively or to an HTML string.
- Test views against the in-memory `TestRenderer`.
- Add routing, forms with rules, translations and a component kit.
- Use host services, animation and gestures through the platform module.

## Example
```rust
use std::rc::Rc;
use quench_ui::prelude::*;

let renderer = Rc::new(TestRenderer::new());
let root = renderer.root();

let count = create_signal(0);
let (shown, clicked) = (count.clone(), count.clone());
let view = div()
    .child(span().text_fn(move || format!("count: {}", shown.get())))
    .child(button().text("+").on_click(move || clicked.update(|c| *c += 1)));

let mounted = mount(&renderer, &root, view);
assert_eq!(renderer.html(), "<div><span>count: 0</span><button>+</button></div>");
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-ui/
