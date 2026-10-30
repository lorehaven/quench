---
title: View API
parent: quench-ui
nav_order: 10
---

Reference for `quench_ui::view`: the view tree and the builders. A `View` only describes UI; nothing exists until [`mount`](api-render.md#mount) builds it into a renderer. Static parts are built once. Reactive parts (`text_fn`, `attr_fn`, `attr_opt`, `attr_bool`, `class_if`, `css`, `focus_if`, `View::dynamic`, `View::keyed`) are closures that mounting wraps in effects, so a signal change patches only the node that read it.

## Key

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Int(i64),
    Str(String),
}
```

Identifies an item in a keyed list. `Int(1)` and `Str("1")` are different keys. `From` impls:

| Source | Result |
|---|---|
| `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32` | `Key::Int(i64::from(v))` |
| `u64` | `Key::Int(v)` if it fits `i64`, otherwise `Key::Str(v.to_string())` |
| `usize` | as `u64` |
| `String` | `Key::Str(v)` |
| `&str` | `Key::Str(v.to_string())` |

There is no impl for `isize`, `i128`, `u128` or `char`; map them to a string or integer yourself.

## View

```rust
pub enum View {
    Empty,
    Text(String),
    DynText(Rc<dyn Fn() -> String>),
    Element(Box<Element>),
    Fragment(Vec<View>),
    Dynamic(Rc<dyn Fn() -> View>),
    Keyed(Rc<dyn Fn() -> Vec<KeyedItem>>),   // KeyedItem is crate-private
}
```

| Variant | Meaning |
|---|---|
| `Empty` | Nothing; builds no node. |
| `Text(String)` | Static text node. |
| `DynText(f)` | A text node whose content follows `f`; one effect, one `set_text` per change. |
| `Element(e)` | An element and everything attached to it. |
| `Fragment(v)` | Views side by side, no wrapper node. |
| `Dynamic(f)` | A region rebuilt whenever what `f` read changes, ended by an anchor node. |
| `Keyed(f)` | A list reconciled by key, ended by an anchor node. Build it with `View::keyed`; the item type is not nameable outside the crate. |

`View` implements `Debug` (`Empty`, `Text("..")`, `DynText(..)`, the element, `Fragment([..])`, `Dynamic(..)`, `Keyed(..)`). It is not `Clone`, `Send` or `Sync`.

### Constructors

```rust
#[must_use] pub fn text(text: impl Into<String>) -> Self
#[must_use] pub fn text_fn(f: impl Fn() -> String + 'static) -> Self
#[must_use] pub fn dynamic(f: impl Fn() -> Self + 'static) -> Self
#[must_use] pub fn lazy(build: impl FnOnce() -> Self + 'static) -> Self
#[must_use] pub fn show(when: impl Fn() -> bool + 'static, view: impl Fn() -> Self + 'static) -> Self
#[must_use] pub fn keyed<T, K>(
    items: impl Fn() -> Vec<T> + 'static,
    key: impl Fn(&T) -> K + 'static,
    render: impl Fn(T) -> Self + 'static,
) -> Self
where T: 'static, K: Into<Key>
```

* `text` is `View::Text`. `text_fn` is `View::DynText`.
* `dynamic(f)`: `f` runs inside an effect; the signals it reads are the region's dependencies. When one changes, the old content is removed (its scope and effects disposed) and `f()`'s view is built in its place.
* `lazy(build)`: a `Dynamic` whose closure takes `build` out of a `Cell` on the first run, runs it under `untrack`, and returns `Empty` on any later run. The view is therefore built when mounted, inside the mount's scope, so signals, effects and timers it creates belong to the mount. A component that is called before `mount` and sets up effects should return its view through `lazy`.
* `show(when, view)`: `dynamic(move || if when() { view() } else { Empty })`. `view` must return a `View`; call `.into_view()` on an `Element`. The region reruns whenever any signal read by `when` or `view` changes, so `view()` is rebuilt each time while `when()` is true.
* `keyed(items, key, render)`: `items()` is read in an effect. Each item gets `key(&item).into()`. `render(item)` runs only for a key that was not present in the previous list; for a key that persists the old nodes and scope are kept even if the new item value differs, so carry changing data in signals. **Panics** `"duplicate key in keyed list: {key:?}"` when `items()` returns two items with the same key (checked on every run). See [internals](internals.md#keyed-reconciliation) for the algorithm.

## IntoView

```rust
pub trait IntoView {
    fn into_view(self) -> View;
}
```

| Type | View |
|---|---|
| `View` | itself |
| `Element` | `View::Element` |
| `String`, `&str` | `View::Text` |
| `i8 i16 i32 i64 isize u8 u16 u32 u64 usize f32 f64 bool char` | `View::Text(self.to_string())` |
| `Vec<V: IntoView>` | `View::Fragment` of the converted items |
| `Option<V: IntoView>` | `None` is `View::Empty` |
| `()` | `View::Empty` |

`i128`, `u128`, `Rc<str>` and `Cow<str>` do not implement it.

### fragment

```rust
#[must_use]
pub fn fragment(children: impl IntoIterator<Item = View>) -> View
```

Collects the views into `View::Fragment`.

## Element

```rust
pub struct Element { /* tag, attrs, classes, styles, css, children, handlers, prevent, focus, focus_if (all crate-private) */ }
impl fmt::Debug for Element   // Element { tag, attrs: <count>, classes: <count>, children, .. }
```

Every builder method takes `self` and returns `Self` and is `#[must_use]` unless noted. The order of calls matters only for children (document order), for repeated `style` and `css` properties and for `class`.

### element

```rust
#[must_use]
pub fn element(tag: &str) -> Element
```

Starts an element. The tag is lower-cased. **Panics** `"invalid tag name: {tag:?}"` unless `tag` starts with an ASCII letter and continues with ASCII letters, digits or `-` (so custom elements such as `my-widget` work; `"div><script"` panics).

### Tag helpers

Each is `#[must_use] pub fn <name>() -> Element` and equals `element("<name>")`. They are public in `quench_ui::view` and in the prelude:

`a`, `button`, `div`, `footer`, `form`, `h1`, `h2`, `h3`, `header`, `i`, `img`, `input`, `label`, `li`, `main`, `nav`, `option`, `p`, `pre`, `section`, `select`, `span`, `strong`, `textarea`, `ul`.

Any other tag (`table`, `code`, `svg`, ...) goes through `element("...")`. Note that the prelude exports `a`, `i`, `p`, `main` and `form` as functions, so a local variable of the same name shadows them in that scope (a variable named `a` next to a call `a()` fails to compile with "expected function").

Void elements (`area base br col embed hr img input link meta param source track wbr`) never get children: `input().child("x")` builds `<input>` and drops the child.

### Attributes

```rust
pub fn attr(self, name: &str, value: impl Into<String>) -> Self
pub fn attr_fn(self, name: &str, value: impl Fn() -> String + 'static) -> Self
pub fn attr_opt(self, name: &str, value: impl Fn() -> Option<String> + 'static) -> Self
pub fn attr_bool(self, name: &str, value: impl Fn() -> bool + 'static) -> Self
```

* `attr`: static value. The name is lower-cased. A later call with the same name replaces the earlier one (static or dynamic).
* `attr_fn`: value follows the closure (an effect).
* `attr_opt`: `Some(v)` sets, `None` removes the attribute (`Renderer::remove_attr`).
* `attr_bool`: present with an empty value while the closure returns true, removed otherwise. It is `attr_opt(name, || value().then(String::new))`.

When mounted, attribute names that fail `html::is_valid_attr_name` (empty, other characters than letters/digits/`-_:.`, or starting with `on`, case-insensitive) are skipped silently, and every value goes through `html::sanitize_attr_value`, also on each reactive update. See [api-render](api-render.md#html).

### Classes

```rust
pub fn class(self, name: impl Into<String>) -> Self
pub fn class_if(self, name: impl Into<String>, when: impl Fn() -> bool + 'static) -> Self
```

`class` adds a class. `class_if` adds one that is on while `when()` is true. Names are joined with a single space in call order and written as the `class` attribute; when no class is on, the attribute is removed (it is only set while the joined value is non-empty). If any `class_if` is present one effect keeps the whole attribute up to date. Classes are applied after `attr`, so a `class` attribute set with `attr("class", ..)` is overwritten when the element also has `class(..)`/`class_if(..)` calls. A name containing spaces is written as is.

### Styles

```rust
pub fn style(self, property: &str, value: impl Into<String>) -> Self
pub fn css(self, property: &str, value: impl Fn() -> String + 'static) -> Self
```

* `style`: static inline property. Repeating a property (exact, case-sensitive match) replaces its value. At mount all properties are joined as `prop: value; prop2: value2` into one `style` attribute through `Renderer::set_attr`. Properties whose name has characters other than ASCII letters, digits and `-` are skipped. Values are not validated (only escaped as attribute text by the renderer): never put untrusted text there. The attribute is an inline style, so a Content-Security-Policy without `'unsafe-inline'` blocks it in a browser.
* `css`: the reactive form. It is applied with `Renderer::set_style` (the style object, not an attribute), so it works under a policy that forbids inline styles. It is applied by the renderer only: `render_to_string` and the HTML of `TestRenderer` leave it out (inspect it with `TestRenderer::style_of`). Same property-name filter as `style`. Each call is its own effect; an empty returned string clears the property.

### Children

```rust
pub fn child(self, child: impl IntoView) -> Self
pub fn children(self, children: impl IntoIterator<Item = View>) -> Self
pub fn text(self, text: impl Into<String>) -> Self
pub fn text_fn(self, f: impl Fn() -> String + 'static) -> Self
```

`child` appends `child.into_view()`. `children` appends views (build a list of elements with `.map(IntoView::into_view)`). `text` appends a static text node. `text_fn` appends a `DynText`.

### Events and focus

```rust
pub fn on(self, kind: EventKind, handler: impl Fn(&Event) + 'static) -> Self
pub fn on_click(self, handler: impl Fn() + 'static) -> Self
pub fn on_input(self, handler: impl Fn(&str) + 'static) -> Self
pub fn on_change(self, handler: impl Fn(&str) + 'static) -> Self
pub fn on_submit(self, handler: impl Fn() + 'static) -> Self
pub fn on_keydown(self, handler: impl Fn(&KeyEvent) + 'static) -> Self
```

All six are `#[must_use]`.

* `on(kind, handler)` registers a listener. Several listeners per kind are allowed and run in registration order. At mount each handler is wrapped to run inside `batch`, and in debug builds `debug_assert_eq!(event.kind(), kind)` checks the event delivered to it.
* `on_click`, `on_submit`: the closure takes no argument. `on_input`/`on_change` receive the control's value (`Event::Input(v)`/`Event::Change(v)`). `on_keydown` receives `&KeyEvent`. They are thin wrappers over `on` that ignore events of another payload variant.
* Other kinds (pointer, scroll, resize, composition, focus, blur, transition end, key up, double click) go through `on(kind, |event| match event { .. })`.

```rust
pub fn prevent_default(self, kind: EventKind) -> Self
pub fn intercept_click(self) -> Self
pub fn focus_if(self, when: impl Fn() -> bool + 'static) -> Self
pub const fn focus_on_mount(self) -> Self
pub const fn trap_focus(self) -> Self
```

* `prevent_default(kind)`: asks the renderer to stop the browser's default action for `kind` on this element (`Prevent::Always`). A form's own submit is always stopped by the web backend; this is for the rest, such as a `keydown` you handle yourself. To decide per event, call `quench_ui::prevent_default()` from the listener instead.
* `intercept_click()`: registers `Prevent::PlainClick` for `Click`: the click listener runs for a plain left click (default stopped) and not for ctrl-, shift- or middle-clicks, which the browser handles. Use it with `a().attr("href", ..)` for in-app navigation.
* `focus_if(when)`: an effect calls `Renderer::focus` each time `when()` turns from false to true (and at build if it is true then).
* `focus_on_mount()`: `Renderer::focus` once when the element is built (`const fn`).
* `trap_focus()`: `Renderer::trap_focus` and then `focus` when built (`const fn`). For modal dialogs and sheets.

`focus_on_mount` and `trap_focus` overwrite each other; the last call wins. Backends that implement neither (the `TestRenderer`) ignore them.
