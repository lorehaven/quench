---
title: Testing
parent: quench-ui
nav_order: 12
---

How `quench-ui` is tested, and how to test components built on it.

## How the crate is tested

`Cargo.toml` declares `autotests = false`, `[lib] harness = false` and one `[[test]] name = "unit" path = "tests/unit.rs" harness = false`. Both the library tests (`quench_test::main!()` at the end of `lib.rs`, under `#[cfg(test)]`) and the integration target use the `quench-test` runtime: tests are written `#[quench_test::test]`, and `tests/unit.rs` includes every file with `#[path = "unit/<name>.rs"] mod ..;` and ends with `quench_test::main!();`.

Run them with `cargo test -p quench-ui`.

| File under `tests/unit/` | Covers |
|---|---|
| `reactive_tests.rs` | tracking, `untrack`, dynamic dependencies, memos (recompute and equality gating), diamonds and chains, batching, self-triggering and the cycle limit, a panic not wedging the runtime, disposal checked through `Rc` strong counts, scopes, context, cleanups, random graphs compared with a direct recomputation |
| `view_tests.rs` | static HTML, `IntoView` types, the counter (one `SetText` per click), batching in handlers, reactive attributes/classes, dynamic regions, `show`, event payloads and kinds, unmount leaving no nodes or listeners, 10,000 nodes returning to baseline, debug check of mismatched event delivery, context in views, invalid tag panic, a keyed todo list, document order, newline after `pre`/`textarea` |
| `keyed_tests.rs` | random edit sequences against an expected list, node identity, minimal moves, nesting, duplicate keys panic |
| `html_tests.rs` | escaping of text and attributes, attribute-name filter, blocked URL schemes in many spellings, tag validation, raw text, void elements, escaping on reactive updates, `render_to_string`, style property filtering |
| `virtual_list_tests.rs` | rows built, scroll window, viewport height, item changes |
| `router_tests.rs`, `form_tests.rs`, `i18n_tests.rs`, `kit_tests.rs`, `gesture_tests.rs`, `anim_tests.rs` | the application modules (see their pages) |

The old flat page claimed 99% line coverage; that figure was not re-measured for this book. *Unverified:* current coverage.

## Testing your components

Mount into a `TestRenderer`, act through `fire`, assert on `html()`, `text()` or `ops()`.

```rust
use std::rc::Rc;
use quench_ui::prelude::*;
use quench_ui::test_renderer::Op;

fn counter() -> View {
    let n = create_signal(0);
    let (shown, bump) = (n.clone(), n.clone());
    button().text_fn(move || format!("{}", shown.get()))
        .on_click(move || bump.update(|v| *v += 1))
        .into_view()
}

let r = Rc::new(TestRenderer::new());
let root = r.root();
let m = mount(&r, &root, counter());
let b = r.find_by_tag("button")[0];

r.clear_ops();
r.fire(b, &Event::Click);
assert_eq!(r.text(), "1");
// one click, one patch, nothing rebuilt
assert!(matches!(r.ops().as_slice(), [Op::SetText(_, t)] if t == "1"));

m.unmount();
assert_eq!(r.live_nodes(), 1);       // no leaked nodes
assert_eq!(r.live_listeners(), 0);   // no leaked listeners
```

Techniques:

* **Find nodes** with `find_by_tag`/`find_by_attr` (give elements a `data-testid` or `id` attribute); read with `attr`, `tag`, `text_of`, `outer_html`, `children`, `parent`.
* **Deliver events** with `fire(node, &Event::...)`. There is no bubbling: fire on the node that has the listener. Listeners run inside `batch`, as in a real host.
* **Assert minimal updates** by `clear_ops()` before the action and comparing `ops()` after (`Op::SetText`, `SetAttr`, `RemoveAttr`, `SetStyle`, `Insert`, `Remove`, ...).
* **Assert no leaks** with `live_nodes()` (1 after unmount) and `live_listeners()` (0), and with `Rc::strong_count` of values captured by closures.
* **Assert document order** (for hydration) by checking that `document_order()` yields increasing ids: `let ids = r.document_order(); assert!(ids.windows(2).all(|w| w[0] < w[1]));` (`NodeId` is `Ord`).
* **Test `css` properties** with `style_of(node, "height")`: they are not in `html()`.
* **Compare HTML ignoring anchors**: `r.html().replace("<!---->", "")`.
* **Server-rendered output**: `render_to_string(view)`.
* **Context and scopes**: wrap in `create_root` and dispose the returned scope at the end; call `scope.run(|| ..)` to build components that need a provided context.
* **Panics**: mark the test `#[should_panic(expected = "...")]` (the runtime supports it) for duplicate keys, invalid tags and the reactive cycle limit.
* **Timers, clocks, gestures, history**: use `ManualPlatform`, `Recognizer` and `MemoryHistory` ([api-platform](api-platform.md), [api-router](api-router.md)).
