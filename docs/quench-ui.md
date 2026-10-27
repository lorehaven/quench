# Quench UI

`quench-ui` (crate `quench_ui`) is the platform-independent core of the Quench UI: fine-grained reactivity, a view tree, events and the `Renderer` interface a backend implements. It has no dependencies and no `unsafe` code, and builds for `wasm32-unknown-unknown`. Nothing in it knows about the DOM: the web backend (`quench-ui-web`, Stage 4) and an HTML-string renderer for server-side rendering plug in behind the same trait. Styling comes from [`quench-style`](./quench-style.md).

Everything runs on one thread (`Rc`/`RefCell`), so signals and views are `!Send`.

## A counter

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

renderer.fire(renderer.find_by_tag("button")[0], &Event::Click);
assert_eq!(renderer.text(), "count: 1+");   // one SetText on one node, nothing else
mounted.unmount();
```

## Reactivity

| | |
|---|---|
| `create_signal(v)` | `Signal<T>`: `get`, `set`, `update`, `with`, `set_if_changed`, plus `*_untracked` reads. Cheap to clone. |
| `create_memo(f)` | `Memo<T: PartialEq>`: a derived value. Observers are notified only if the new value differs. |
| `create_effect(f)` | Runs now and again when anything it read changes. |
| `batch(f)` | Defers effects until `f` returns; each affected effect then runs once. Event handlers run in a batch. |
| `untrack(f)` | Reads inside `f` do not subscribe. |
| `create_root(f)`, `Scope` | Ownership. Effects, memos, cleanups and contexts belong to the scope current when they were made; disposing it unsubscribes and drops all of it. |
| `on_cleanup(f)` | Runs when the scope is disposed or the effect re-runs. |
| `provide_context(v)`, `use_context::<T>()` | Values for everything under a scope. |

The graph is push-dirty, pull-check. Writing a signal marks its observers dirty and the ones behind them "check"; effects are queued once. Before an effect re-runs, the memos it reads are refreshed, and it runs only if one really changed. So a diamond (`a → b, c → d`) runs `d` once per change and never sees `b` updated and `c` stale. An effect that keeps triggering itself or another stops after 100,000 runs with a "reactive cycle" panic. An effect that re-runs disposes the effects and cleanups its previous run created.

## Views

`View` is `Empty`, `Text`, `DynText`, `Element`, `Fragment`, `Dynamic` and `Keyed`. Build elements with the same chain style as `quench-web`: `div().class("x").attr("id", "a").child(...)`. Anything implementing `IntoView` (strings, numbers, `Option`, `Vec`, elements) can be a child.

- **Static** parts are built once.
- **Reactive** parts are closures that mounting wraps in effects, so a change patches only the node that read it: `text_fn`, `attr_fn`, `attr_opt`, `attr_bool`, `class_if`.
- **`View::dynamic(f)`** rebuilds a region whenever what `f` read changes; `View::show(when, view)` is the common case.
- **`View::keyed(items, key, render)`** reconciles a list by key. An item whose key is still present keeps its nodes and state and is not rebuilt, even if the item value differs, so carry changing data in signals. Only items that moved are moved (a longest-increasing-subsequence pass). Duplicate keys panic. Keys convert from integers and strings.

Events are typed (`Event`, `EventKind`, `KeyEvent`, `PointerEvent`): `on_click`, `on_input(|value| ..)`, `on_change`, `on_submit`, `on_keydown`, or `on(kind, |event| ..)`.

## Renderers

`mount(&renderer, &parent, view)` builds a view into any `Renderer` (create element/text/anchor, set text and attributes, insert/remove, listen) and returns a `Mount`; `unmount()` removes the nodes and disposes every effect, and `leak()` keeps the view for good (an app that lives until the page closes). The mount keeps what it built inside each element, so a renderer whose node handles own resources (the DOM backend's listeners) holds them exactly as long as the element is attached. A dynamic or keyed region is delimited by an anchor node so it can rebuild in place.

`TestRenderer` is an in-memory tree: `html()`, `text()`, `find_by_tag`, `find_by_attr`, `attr`, `fire(node, &event)`, an `ops()` log of every change (assert that a click causes one `SetText`), and `live_nodes()` / `live_listeners()` to catch leaks. `render_to_string(view)` builds a view and returns its HTML; regions appear as `<!---->` anchors.

## Escaping and safety

Output is made safe where views are mounted, so every renderer gets it:

- Text is escaped (`& < >`); attribute values are escaped (`& < > " '`). `<script>` and `<style>` text cannot close its element.
- Attribute names that start with `on` or contain unusual characters are dropped, so markup cannot grow inline event handlers (listeners are the only way to react to events, which also suits a strict Content-Security-Policy). Invalid tag names panic.
- URL attributes (`href`, `src`, `action`, `formaction`, `poster`, `data`, `srcset`, `xlink:href`) holding `javascript:`, `vbscript:` or non-image `data:` URLs, however spelled (case, whitespace, control characters), become `about:invalid#blocked`, also on every reactive update.
- Inline style property names are limited to letters, digits and `-`. Style values are escaped as attribute text but are otherwise yours: do not put untrusted text in them.

## Testing

Unit tests under `tests/unit` cover the reactive core (tracking, branches, memos, diamonds, chains, batching, self-triggering and cycle limits, panic recovery, disposal checked through `Rc` strong counts, scopes, context, and random graphs compared with a direct recomputation), views and events (a counter and a keyed todo list as the exit-gate examples, one-patch updates, unmount leaving no nodes or listeners, 10,000 nodes returning to baseline), keyed reconciliation (random edit sequences against the expected output, node identity, minimal moves, nesting) and the escaping vectors. Line coverage 99%.

[Home](../README.md)
