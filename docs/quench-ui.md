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

## Router

`quench_ui::router` routes URLs to views, in the browser, in a test and on the server alike.

```rust
let router = Router::new(Rc::new(BrowserHistory::new()));   // or MemoryHistory::new("/users/7")
Routes::new()
    .layout(layout("/app", |ctx, inner| shell(ctx, inner))          // stays mounted across children
        .route("", |_| home())                                      // "" is the index
        .route("users/:id", |ctx| user(ctx))                        // ctx.param("id")
        .guarded_route("admin", |v| if signed_in() { Guard::Allow } else { Guard::Redirect("/app/login".into()) }, |_| admin())
        .layout(layout("settings", ..).route("profile", ..).route("about", ..)))
    .not_found(|_| not_found())
    .view(&router)
```

Patterns are literal segments, `:name` (one segment) and `*rest` (everything left); earlier routes win; a trailing slash does not matter; segments are percent-decoded. A layout receives the view of whichever child matched and is not rebuilt while navigation moves between its children; `ctx.param(..)` is reactive, so moving between `/users/1` and `/users/2` patches the page instead of rebuilding it. `link(href)` renders an `<a>` that navigates without a page load on a plain click (ctrl-click and the like still open a tab) and carries `aria-current="page"` on the current path. Guards run for the whole chain, outermost first, and read signals, so signing out redirects at once; `Guard::Redirect` replaces the history entry (a loop is cut after 8 hops and ends in not-found), `Guard::Deny` shows `.forbidden(..)`. Routes are built when mounted (`View::lazy`), so build the router inside a component or `run`.

`Router::on_back(handler)` registers a handler for the back action (an Android back button, a sheet that wants to close); `handle_back()` runs them newest first, then goes back in history, and returns false when the host should leave the app. `Router::is_not_found()` lets a server answer 404.

## Forms

`quench_ui::form`: a `Field` holds a value, an error and whether the user has been through it; `field.input()` is an `<input>` bound to it (value, `id`/`name`, `aria-invalid`, `aria-describedby`), `field.error_view()` shows the error as `role="alert"`. Rules (`rules::required`, `min_len`, `max_len`, `check`, `date`, `time`) run on blur, then on every change. While an input method composes (`compositionstart` .. `compositionend`) typing does not change the value, and the host does not write the same value back into the input, so the caret and a half-typed syllable survive. A `Form` collects fields: `form.element(|f| ..)` is a `<form novalidate>` that checks every field when submitted and calls the handler only if all pass; `apply_errors(message, &[(field, message)])` shows a server's verdict, and `quench_fullstack::FormExt::finish(result)` does that for the `Result` of a server function.

## Long lists

`virtual_list(&items, row_height, key, render)` builds only the rows in view plus five each side, inside a spacer as tall as the whole list (set through the style object, see `Element::css`). Scrolling a little keeps the rows it keeps. The element needs a bounded height from your stylesheet (`.q-vlist` scrolls). Rows have one fixed height.

## Platform services, gestures and animation

`platform` holds what the UI needs from its host: a clock, one-shot timers and the reduced-motion preference. `quench-ui-web` installs the browser's; `ManualPlatform` is one a test advances by hand. `gesture::on_gesture(element, config, handler)` turns pointer events into `Tap`, `LongPress`, `DragStart`, `Drag{dx,dy}`, `DragEnd{.., vx, vy}`, `Swipe(direction)` and `Cancel`; `Recognizer` is the same state machine without an element. `Axis` sets `touch-action` through a class (a strict policy forbids inline styles). `anim::presence(visible, duration_ms, content)` keeps content in the page until its exit transition has played (`transitionend`, with a timer as fallback) and skips the waiting when reduced motion is preferred; style `.q-anim` and `.q-anim-in`.

## Component kit

`quench_ui::kit`: `app_bar`, `icon_button`, `bottom_nav`, `sheet`, `dialog`, `snackbar` and `toast`, `list` and `list_tile`/`tile`, `switch`, `checkbox`, `text_field`, `date_field`/`time_field` (the platform's pickers), `fab`, `chip`, `tabs`, `pull_to_refresh`, `swipe_action`. They are functions that return elements with `q-*` classes, styled by `quench_style::components`. Each has the ARIA roles of its pattern: modals are `role="dialog" aria-modal="true"`, trap focus and close on Escape and on the scrim; the switch is `role="switch"`; tabs have `tablist`/`tab`/`tabpanel` with a roving `tabindex` and arrow, Home and End keys; chips use `aria-pressed`; the snackbar is a polite live region. Touch targets are at least 48 px. Pull to refresh and swipe actions need a touch device to judge: the logic is tested with simulated pointer events, but how the browser shares a drag with scrolling (`touch-action`) has to be checked on the devices you ship to.

## Translations

`quench_ui::i18n`: `I18n::new(&[("en", include_str!("en.ftl")), ("pl", ..)], "en")`, then `i18n.tr("files", &[("n", 5.into())])` inside a view closure; `set_language` updates what read it. The Fluent subset covers messages, `{ $var }`, message references, literals, `NUMBER($n)` and selectors on plural categories or exact numbers. Plural rules are CLDR's for English and Polish (`one`, `few`, `many`, `other`: 1 plik, 2 pliki, 5 plików, 22 pliki, 1.5 pliku), French and Portuguese, the languages without plurals, and English for the rest. A missing id shows as itself and a language that lacks a message falls back to the first one given.

## Renderers

`mount(&renderer, &parent, view)` builds a view into any `Renderer` (create element/text/anchor, set text and attributes, insert/remove, listen) and returns a `Mount`; `unmount()` removes the nodes and disposes every effect, and `leak()` keeps the view for good (an app that lives until the page closes). The mount keeps what it built inside each element, so a renderer whose node handles own resources (the DOM backend's listeners) holds them exactly as long as the element is attached. A dynamic or keyed region is delimited by an anchor node so it can rebuild in place. Nodes are created in **document order** (an element before its children, a region's content before the anchor that ends it), which is what lets a DOM backend hydrate server-rendered markup by walking it with a cursor; `TestRenderer::document_order()` lets a test assert it. `render_to_string` writes a second newline after `<pre>`/`<textarea>` text that starts with one, since the HTML parser drops the first.

Beyond drawing, a renderer can `prevent_default(node, kind, how)` (a link's click, a key), `focus` and `trap_focus` a node, and `set_style` through the style object. `Element::css(property, || value)` is the reactive form: unlike `Element::style` it is not an inline `style` attribute, so it works under a policy without `'unsafe-inline'`. A listener can call `quench_ui::prevent_default()` to stop the browser's default action for the event it is handling.

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
