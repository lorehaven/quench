---
title: Usage
parent: quench-ui
nav_order: 1
---

Task-oriented examples for the core of `quench-ui`. All snippets use `use quench_ui::prelude::*;` unless a further import is shown, and run on the in-memory `TestRenderer`; in an application the renderer is the browser or native backend. Routing, forms and translations are in [usage-app](usage-app.md).

## 1. Signals, memos and effects

A signal holds a value; reading it inside an effect or memo subscribes; writing it re-runs the subscribers.

```rust
use std::cell::RefCell;
use std::rc::Rc;
use quench_ui::prelude::*;

let a = create_signal(2);
let b = create_signal(3);
let sum = { let (a, b) = (a.clone(), b.clone()); create_memo(move || a.get() + b.get()) };

let log = Rc::new(RefCell::new(Vec::new()));
let (log2, sum2) = (log.clone(), sum.clone());
let _effect = create_effect(move || log2.borrow_mut().push(sum2.get()));

a.set(10);                         // effect runs again: 13
batch(|| { a.set(1); b.set(1); }); // one run for both writes: 2
assert_eq!(*log.borrow(), vec![5, 13, 2]);
```

* Effects run once immediately, then after each relevant write. Outside a `batch` they have already run when `set` returns.
* A memo notifies its observers only if the new value is `!=` the old one; use `set_if_changed` on a signal for the same effect at the source.
* `get()` clones; use `with(|v| ..)` to borrow a large value. Inside `with`, do not write the same signal (a `RefCell` borrow panic).
* A signal is not tied to a scope; effects and memos are. Clone the signal into each closure (`let s2 = s.clone();`): the closures are `'static`.

## 2. Reading without subscribing

```rust
use std::{cell::Cell, rc::Rc};
use quench_ui::prelude::*;

let a = create_signal(0);
let b = create_signal(0);
let runs = Rc::new(Cell::new(0));
let (r, a2, b2) = (runs.clone(), a.clone(), b.clone());
let _e = create_effect(move || {
    let _ = a2.get();                 // tracked
    let _ = untrack(|| b2.get());     // not tracked (b2.get_untracked() is the same)
    r.set(r.get() + 1);
});
b.set(1);                             // no re-run
assert_eq!(runs.get(), 1);
a.set(1);
assert_eq!(runs.get(), 2);
```

## 3. Scopes, cleanup and context

`create_root` gives you a scope to dispose later. Effects, memos, cleanups and contexts created while it is current belong to it.

```rust
use std::{cell::Cell, rc::Rc};
use quench_ui::prelude::*;

#[derive(Clone)]
struct Theme(&'static str);

let cleaned = Rc::new(Cell::new(false));
let flag = cleaned.clone();
let ((), scope) = create_root(|| {
    provide_context(Theme("dark"));
    on_cleanup(move || flag.set(true));
    assert_eq!(use_context::<Theme>().unwrap().0, "dark");
});
scope.dispose();                      // runs the cleanup, unsubscribes every effect
assert!(cleaned.get());
```

* `use_context::<T>()` searches the current scope, then its ancestors, and returns a clone. The type is the key: wrap values in a newtype.
* `provide_context` panics without a current scope; inside `mount`ed views there always is one.
* `on_cleanup` inside an effect runs before each re-run and on dispose.
* A component function that is called while building a view runs in the caller's scope. A component that sets up effects before `mount` should return `View::lazy(|| ..)`, so what it creates belongs to the mount.

## 4. Building views and mounting

```rust
use std::rc::Rc;
use quench_ui::prelude::*;

let renderer = Rc::new(TestRenderer::new());
let root = renderer.root();
let view = div().class("card").attr("id", "c1")
    .child(h1().text("Title"))
    .child(p().text("Body ").child(strong().text("bold")));
let mounted = mount(&renderer, &root, view);
assert_eq!(renderer.html(),
    r#"<div id="c1" class="card"><h1>Title</h1><p>Body <strong>bold</strong></p></div>"#);
mounted.unmount();
assert_eq!(renderer.live_nodes(), 1);   // only the root is left
```

* Attributes are written in the order set, then `class`, then `style`, whatever the order of the builder calls; see [api-view](api-view.md#classes).
* Anything `IntoView` is a child: strings, numbers, `Option`, `Vec`, elements (`div().child(Some("x"))`, `ul().child(vec![li().text("a"), li().text("b")])`).
* Keep the `Mount` value; dropping it silently stops reactivity ([api-render](api-render.md#mount)). `leak()` keeps a view for good.

## 5. Reactive text, attributes, classes and regions

```rust
use std::rc::Rc;
use quench_ui::prelude::*;

let r = Rc::new(TestRenderer::new());
let root = r.root();
let open = create_signal(false);
let (o1, o2, o3, o4) = (open.clone(), open.clone(), open.clone(), open.clone());
let _m = mount(&r, &root, div()
    .child(button()
        .class_if("on", move || o1.get())                 // class on while true
        .attr_bool("disabled", move || !o2.get())         // attribute present while true
        .text("toggle")
        .on_click(move || o3.update(|v| *v = !*v)))
    .child(View::show(move || o4.get(), || p().text("details").into_view())));

assert_eq!(r.html(), r#"<div><button disabled="">toggle</button><!----></div>"#);
r.fire(r.find_by_tag("button")[0], &Event::Click);
assert_eq!(r.html(), r#"<div><button class="on">toggle</button><p>details</p><!----></div>"#);
```

* `text_fn`, `attr_fn` (always present), `attr_opt` (`None` removes), `attr_bool`, `class_if` patch one node each.
* `View::show(when, view)` and `View::dynamic(f)` rebuild a region; the closure must return a `View` (call `.into_view()` on an element). The `<!---->` after the region is its anchor.
* A region rebuild disposes everything the old content created. Do not use it for content that must keep state while its data changes: use a keyed list or put the changing data in signals.

## 6. Keyed lists

```rust
use quench_ui::prelude::*;

#[derive(Clone)]
struct Todo { id: u32, title: String, done: Signal<bool> }

let todos = create_signal(vec![
    Todo { id: 1, title: "milk".into(), done: create_signal(false) },
    Todo { id: 2, title: "eggs".into(), done: create_signal(true) },
]);
let source = todos.clone();
let html = quench_ui::render_to_string(ul().child(View::keyed(
    move || source.get(),
    |todo| todo.id,                       // anything Into<Key>
    |todo| {
        let done = todo.done.clone();
        li().class_if("done", move || done.get()).text(todo.title).into_view()
    },
)));
assert_eq!(html, r#"<ul><li>milk</li><li class="done">eggs</li><!----></ul>"#);
```

* `render` runs only for keys not present before. For a key that stays, the old nodes keep their state even if the item value in the new list differs: put values that change in signals (here `done`), not in the item.
* Reordering moves only the nodes that are out of place; removing disposes the item's scope; duplicate keys panic.
* Keys are `Key::Int` or `Key::Str` (any integer up to 64 bits, `String`, `&str`).
* For 10,000 rows use [`virtual_list`](api-render.md#virtual_list).

## 7. Events

```rust
use std::rc::Rc;
use quench_ui::prelude::*;

let r = Rc::new(TestRenderer::new());
let root = r.root();
let name = create_signal(String::new());
let (typed, shown) = (name.clone(), name.clone());
let m = mount(&r, &root, div()
    .child(input().attr("type", "text").on_input(move |v| typed.set(v.to_string())))
    .child(p().text_fn(move || format!("hello {}", shown.get()))));
r.fire(r.find_by_tag("input")[0], &Event::Input("Ada".into()));
assert_eq!(r.text(), "hello Ada");
m.unmount();
```

* `on_click`, `on_input`, `on_change`, `on_submit`, `on_keydown` cover the common cases; `on(EventKind::PointerDown, |e| ..)` handles the rest (`match e { Event::PointerDown(p) => .. , _ => {} }`).
* Each listener runs inside `batch`: several writes in one handler cause one update per affected effect.
* `quench_ui::prevent_default()` inside a listener asks the host to cancel the browser default for that event; `Element::prevent_default(kind)` and `intercept_click()` do it declaratively.
* In a test, deliver events with `renderer.fire(node, &event)`: the listeners of that node (not its ancestors: there is no bubbling) whose kind equals `event.kind()` run.

## 8. Rendering to HTML (server side)

```rust
use quench_ui::{render_to_string, prelude::*};

assert_eq!(
    render_to_string(div().style("color", "red").child("x < y")),
    r#"<div style="color: red">x &lt; y</div>"#);
// URL attributes are neutralised, event-handler attributes are dropped
assert_eq!(render_to_string(a().attr("href", "javascript:alert(1)").attr("onclick", "x()")),
    r#"<a href="about:invalid#blocked"></a>"#);
```

* Reactive parts render their current value; listeners and effects are discarded. Regions end in `<!---->`.
* `Element::css(..)` is not rendered (it is applied through the style object in a live renderer).
* `pre`/`textarea` whose text begins with a newline get a second newline so the HTML parser keeps the first.

## 9. Components, props and context

A component is a function returning a `View` (or `Element`). Pass data as plain arguments and signals as `Signal<T>` handles.

```rust
use quench_ui::prelude::*;

#[derive(Clone)]
struct Theme(&'static str);

fn badge(label: &str) -> View {
    let theme = use_context::<Theme>().map_or("light", |t| t.0);   // resolved in the caller's scope
    span().class(format!("badge-{theme}")).text(label.to_string()).into_view()
}

fn counter(start: i32) -> View {
    View::lazy(move || {                 // built inside the mount's scope
        let n = create_signal(start);
        let (show, bump) = (n.clone(), n.clone());
        button().text_fn(move || format!("{}", show.get()))
            .on_click(move || bump.update(|v| *v += 1))
            .into_view()
    })
}
```

Calling `badge` inside `scope.run(..)` of a scope that provided `Theme("dark")` gives `<span class="badge-dark">`; outside it gives `badge-light`.

## 10. Pitfalls

* Types are `!Send`; do not move signals across threads or into `std::thread::spawn`.
* Forgetting `.into_view()` in `View::show`/`View::dynamic`/`View::keyed` closures is a compile error (`expected View, found Element`).
* A prelude function named like your local (`a`, `i`, `p`, `main`, `form`, `input`, `label`, ...) is shadowed by the local, and calling it fails to compile.
* A write inside `Signal::with`/`update` of the same signal panics with a borrow error.
* An effect that writes a signal it reads loops until it panics with `reactive cycle: effects keep triggering each other` (after 100,000 effect runs in one flush).
* `Element::style` is an inline style attribute; use `css` or a stylesheet class under a strict CSP.
