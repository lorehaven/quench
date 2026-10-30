---
title: Events, renderer, mount
parent: quench-ui
nav_order: 8
---

Reference for `quench_ui::event`, `renderer`, `mount` (items re-exported at the crate root), `html`, `test_renderer` and `virtual_list`.

## event

Typed UI events, independent of the platform that produced them.

### EventKind

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum EventKind {
    Click, DoubleClick, Input, Change, KeyDown, KeyUp, Submit, Focus, Blur,
    PointerDown, PointerUp, PointerMove,
    PointerCancel,      // the browser took the pointer over (to scroll, say): the gesture is off
    CompositionStart,   // an input method started composing text
    CompositionEnd,
    Scroll,
    TransitionEnd,      // a CSS transition finished
    Resize,             // the element's size changed (also reported once when first laid out)
}

#[must_use]
pub const fn dom_name(self) -> &'static str
```

`dom_name` is the DOM event name a web backend listens for:

| Kind | Name | Kind | Name |
|---|---|---|---|
| `Click` | `click` | `PointerMove` | `pointermove` |
| `DoubleClick` | `dblclick` | `PointerCancel` | `pointercancel` |
| `Input` | `input` | `CompositionStart` | `compositionstart` |
| `Change` | `change` | `CompositionEnd` | `compositionend` |
| `KeyDown` | `keydown` | `Scroll` | `scroll` |
| `KeyUp` | `keyup` | `TransitionEnd` | `transitionend` |
| `Submit` | `submit` | `Resize` | `resize` |
| `Focus` | `focus` | `PointerDown` | `pointerdown` |
| `Blur` | `blur` | `PointerUp` | `pointerup` |

### Event

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Click, DoubleClick,
    Input(String),                 // the control's current value after the edit
    Change(String),
    KeyDown(KeyEvent), KeyUp(KeyEvent),
    Submit, Focus, Blur,
    PointerDown(PointerEvent), PointerUp(PointerEvent), PointerMove(PointerEvent), PointerCancel(PointerEvent),
    CompositionStart(String),      // the control's value when composing started
    CompositionEnd(String),        // the control's value when the composed text was committed
    Scroll(ScrollEvent),
    TransitionEnd,
    Resize(Size),
}

#[must_use]
pub const fn kind(&self) -> EventKind
```

`kind` returns the listener kind the event is delivered to. `Event::Input(_).kind() == EventKind::Input`, and so on, one variant per kind.

### Payload types

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyEvent { pub key: String, pub ctrl: bool, pub shift: bool, pub alt: bool, pub meta: bool }
impl KeyEvent { #[must_use] pub fn new(key: impl Into<String>) -> Self }   // all modifiers false

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PointerEvent { pub x: f64, pub y: f64, pub pointer_id: i32 }   // CSS pixels

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollEvent { pub top: f64, pub left: f64 }                    // CSS pixels

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size { pub width: f64, pub height: f64 }                       // CSS pixels
```

`KeyEvent::key` is the DOM `key` value (`"a"`, `"Enter"`, `"ArrowDown"`).

### Handler

```rust
pub type Handler = Rc<dyn Fn(&Event)>;
```

A listener stored in the view tree and passed to `Renderer::listen`.

### prevent_default and take_prevent_request

```rust
pub fn prevent_default()
#[must_use] pub fn take_prevent_request() -> bool
```

A thread-local flag. A listener calls `prevent_default()` to ask the host to stop the browser's default action for the event it is handling (for example Home/End moving a widget's selection instead of scrolling the page). It has an effect only while the host is still waiting on the listener, which is the case for events from the user, not for events replayed after an import.

Hosts call `take_prevent_request()` once before delivering an event (to clear the flag) and once after (to read it); each call returns the flag and resets it to false.

## renderer

### Prevent

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Prevent {
    Always,       // always prevent the default action
    PlainClick,   // handle a plain left click and prevent its default; leave modified/non-primary clicks to the browser, without running the listener
}
```

### Renderer

```rust
pub trait Renderer: 'static {
    type Node: Clone + 'static;
    fn create_element(&self, tag: &str) -> Self::Node;
    fn create_text(&self, text: &str) -> Self::Node;
    fn create_anchor(&self) -> Self::Node;
    fn set_text(&self, node: &Self::Node, text: &str);
    fn set_attr(&self, node: &Self::Node, name: &str, value: &str);
    fn remove_attr(&self, node: &Self::Node, name: &str);
    fn insert(&self, parent: &Self::Node, child: &Self::Node, before: Option<&Self::Node>);
    fn remove(&self, parent: &Self::Node, child: &Self::Node);
    fn listen(&self, node: &Self::Node, kind: EventKind, handler: Handler);
    fn prevent_default(&self, _node: &Self::Node, _kind: EventKind, _how: Prevent) {}
    fn set_style(&self, _node: &Self::Node, _property: &str, _value: &str) {}
    fn focus(&self, _node: &Self::Node) {}
    fn trap_focus(&self, _node: &Self::Node) {}
}
```

What a backend implements to show a view tree: the DOM, an in-memory tree, a native widget tree. Methods take `&self` because the renderer is shared (`Rc<R>`) with the effects that patch it later, so it keeps its own interior mutability. **None of the methods may call back into reactive code** (no signal reads or writes, no effect creation).

| Item | Contract |
|---|---|
| `Node` | A handle to a node. Cheap to clone. |
| `create_element(tag)` | Creates a detached element. `tag` is already validated and lower-case. |
| `create_text(text)` | Creates a detached text node. |
| `create_anchor()` | An invisible node marking where a dynamic or keyed region inserts its content (a comment in HTML). |
| `set_text(node, text)` | Replaces a text node's content. |
| `set_attr(node, name, value)` | Sets an attribute. `mount` already dropped invalid names and neutralised dangerous URLs; the renderer receives the sanitised value and must still escape it on output. |
| `remove_attr(node, name)` | Removes an attribute. |
| `insert(parent, child, before)` | Inserts `child` into `parent` before `before`, or at the end when `None`. An already attached `child` moves. |
| `remove(parent, child)` | Detaches `child` from `parent`. |
| `listen(node, kind, handler)` | Registers a listener (already wrapped to run in a batch). Several per kind are allowed. |
| `prevent_default(node, kind, how)` | Default: no-op. Called after `listen`; sets how the node's listeners for `kind` treat the default action. A renderer with no default actions ignores it. |
| `set_style(node, property, value)` | Default: no-op. Sets one CSS property through the style object, not the `style` attribute. An empty `value` clears it. |
| `focus(node)` | Default: no-op. Moves keyboard focus. |
| `trap_focus(node)` | Default: no-op. Keeps focus inside `node` (Tab and Shift+Tab cycle) until the node is released. |

The trait has no `Send`/`Sync` bound.

## mount

### mount

```rust
pub fn mount<R: Renderer>(renderer: &Rc<R>, parent: &R::Node, view: impl IntoView) -> Mount<R>
```

Builds `view` as the last children of `parent`. It creates a new `Scope` (nested in the current scope, if any), builds the view inside it with `untrack`, and returns the `Mount`. Not `#[must_use]`.

What the build does, per view:

* `Empty`: no node. `Text`: `create_text` + `insert`. `DynText`: an effect that creates the node on the first run and `set_text` afterwards.
* `Fragment`: each child built in order.
* `Element`, in this order: `create_element`; attributes (static ones set immediately, dynamic ones in an effect each; invalid names skipped; values sanitised); classes; the inline `style` attribute (one `set_attr`); `css` properties (one effect each); listeners (`listen`, each wrapped in `batch`); `prevent_default` entries; `focus_if` effects; `focus`/`trap_focus`; children (none for a void tag); and finally `insert(parent, node, before)`.
* `Dynamic`/`Keyed`: described under [Regions](#regions).

**Document-order guarantee.** Nodes are created in the order they appear in the finished document: an element before its children (its `insert` into the parent comes last, but its `create_element` comes first), siblings left to right, and a region's content before the anchor that ends it. The anchor is created only after the first content is in place. A DOM backend can therefore hydrate server-rendered markup by walking it with a cursor, taking the next node on each `create_*`. `TestRenderer::document_order` lets a test assert this. In a keyed list with surviving items, new items are created back to front (see [internals](internals.md#keyed-reconciliation)); document order holds for the initial build and for a build with no surviving keys.

**Regions.** A `Dynamic` or `Keyed` view is an effect that owns its content. On each run a dynamic region removes the old content (disposing everything under it) and builds the new view before its anchor. A panic in a user closure propagates to the caller (see [internals](internals.md#panics)).

**Escaping and URL rules** are applied here, so every renderer gets them: attribute names are checked with `is_valid_attr_name`, values with `sanitize_attr_value`; style and `css` property names are limited to ASCII letters, digits and `-`. Text is passed to `create_text`/`set_text` unescaped: a renderer that produces markup escapes it itself (`TestRenderer` does). See [html](#html).

### Mount

```rust
pub struct Mount<R: Renderer> { /* renderer, parent, part, scope */ }
impl<R: Renderer> fmt::Debug for Mount<R>

pub fn unmount(self)
pub const fn leak(self)
#[must_use] pub const fn scope(&self) -> &Scope
```

* `unmount()`: disposes the scope (all effects, cleanups, contexts and child scopes), then removes every node the view built from `parent` (nested nodes of elements go with their element; regions' content and anchors are removed too).
* `leak()`: `std::mem::forget(self)`: the nodes and what they own are never released. For an app that lives until the page closes.
* `scope()`: the scope that owns the view's effects. Disposing it directly stops the effects but leaves the nodes.

Dropping a `Mount` without `unmount` or `leak` leaves the nodes in place, but when the mount was created outside any scope nothing else holds its scope, so its effects are dropped and the view **stops updating** (checked: after `drop(mount)` a later `signal.set` no longer patches the text). Keep the `Mount` alive, create it inside a scope, or call `leak()`.

## html

Escaping and the checks that keep attacker-controlled strings from becoming markup (`quench_ui::html`). All functions are `#[must_use]` unless noted.

```rust
pub const VOID_TAGS: [&str; 14] = ["area","base","br","col","embed","hr","img","input","link","meta","param","source","track","wbr"];
pub const BLOCKED_URL: &str = "about:invalid#blocked";

pub fn is_void(tag: &str) -> bool
pub fn escape_text(s: &str) -> String
pub fn escape_attr(s: &str) -> String
pub fn escape_raw_text(s: &str) -> String
pub fn is_valid_tag(tag: &str) -> bool
pub fn is_valid_attr_name(name: &str) -> bool
pub fn sanitize_attr_value<'a>(name: &str, value: &'a str) -> &'a str
```

| Function | Behaviour |
|---|---|
| `is_void(tag)` | `tag` is in `VOID_TAGS` (exact, case-sensitive). |
| `escape_text(s)` | For text between tags: `&` to `&amp;`, `<` to `&lt;`, `>` to `&gt;`. Quotes untouched. |
| `escape_attr(s)` | For a value inside double quotes: `&` `&amp;`, `<` `&lt;`, `>` `&gt;`, `"` `&quot;`, `'` `&#39;`, NUL to U+FFFD. |
| `escape_raw_text(s)` | For the text of `<script>`/`<style>`: `</` becomes `<\/` and `<!` becomes `\x3c!`, so the text cannot end the element or open a comment. Other characters unchanged. |
| `is_valid_tag(tag)` | Starts with an ASCII letter, rest ASCII letters, digits or `-`. |
| `is_valid_attr_name(name)` | Non-empty, only ASCII letters, digits and `-`, `_`, `:`, `.`, and (lower-cased) not starting with `on`. So `onclick`, `onerror`, `ONLOAD` and also `once` or `online` are rejected. |
| `sanitize_attr_value(name, value)` | If `name` (case-insensitive) is one of `href src action formaction poster data xlink:href srcset`, the value is compacted (ASCII control characters and all whitespace removed, lower-cased) and tested; if it starts with `javascript:` or `vbscript:`, or with `data:` but not `data:image/`, the result is `BLOCKED_URL`; otherwise `value` unchanged. Other attributes are returned unchanged. |

Limits of the check: any `data:image/...` URL is allowed, including `data:image/svg+xml`; `srcset` is tested only at the start of the whole value, not per candidate; `http:` and relative URLs are never rewritten. The checks live in `mount`, so they apply to every renderer.

## test_renderer

An in-memory `Renderer` with a real node tree you can query, fire events at and read back as HTML, plus a log of every operation.

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(usize);        // the field is private

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    CreateElement(String),
    CreateText(String),
    CreateAnchor,
    SetText(NodeId, String),
    SetAttr(NodeId, String, String),
    RemoveAttr(NodeId, String),
    SetStyle(NodeId, String, String),
    Insert { parent: NodeId, child: NodeId, before: Option<NodeId> },
    Remove { parent: NodeId, child: NodeId },
    Listen(NodeId, EventKind),
}

pub struct TestRenderer { /* RefCell<Tree> */ }
impl Default for TestRenderer       // TestRenderer::new()
impl fmt::Debug for TestRenderer    // TestRenderer { nodes: <count> }
impl Renderer for TestRenderer      // type Node = NodeId
```

`NodeId` is exported from `quench_ui::test_renderer`, not from the crate root. `Op::SetStyle` and `SetAttr` etc. are logged by the `Renderer` methods; `prevent_default`, `focus` and `trap_focus` are the trait's no-ops and log nothing.

`TestRenderer` is `!Send`. All methods take `&self`. Use it inside an `Rc` (`mount` takes `&Rc<R>`).

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `#[must_use] pub fn new() -> Self` | A tree with just a root element of tag `#root`. |
| `root` | `#[must_use] pub const fn root(&self) -> NodeId` | The container to mount into. |
| `ops` | `#[must_use] pub fn ops(&self) -> Vec<Op>` | Every operation since creation or the last `clear_ops`. |
| `clear_ops` | `pub fn clear_ops(&self)` | Empties the log. |
| `html` | `#[must_use] pub fn html(&self) -> String` | Root's content as HTML (the root tag itself excluded). Anchors are `<!---->`. |
| `outer_html` | `#[must_use] pub fn outer_html(&self, node: NodeId) -> String` | One node's HTML, itself included. |
| `text` | `#[must_use] pub fn text(&self) -> String` | All text under the root, concatenated, no escaping. |
| `text_of` | `#[must_use] pub fn text_of(&self, node: NodeId) -> String` | Same for a node. |
| `children` | `#[must_use] pub fn children(&self, node: NodeId) -> Vec<NodeId>` | Child nodes including text and anchors. |
| `parent` | `#[must_use] pub fn parent(&self, node: NodeId) -> Option<NodeId>` | |
| `tag` | `#[must_use] pub fn tag(&self, node: NodeId) -> Option<String>` | `None` for text and anchors. |
| `attr` | `#[must_use] pub fn attr(&self, node: NodeId, name: &str) -> Option<String>` | An attribute set through `set_attr` (exact name). |
| `style_of` | `#[must_use] pub fn style_of(&self, node: NodeId, property: &str) -> Option<String>` | A property set through `set_style` (not the `style` attribute); an empty value removes it. |
| `find_by_tag` | `#[must_use] pub fn find_by_tag(&self, tag: &str) -> Vec<NodeId>` | Elements under the root with this tag, document order (the root is never returned). |
| `find_by_attr` | `#[must_use] pub fn find_by_attr(&self, name: &str, value: &str) -> Vec<NodeId>` | Elements whose attribute equals `value`. |
| `fire` | `pub fn fire(&self, node: NodeId, event: &Event)` | Calls the node's listeners whose kind equals `event.kind()`, in registration order, with no borrow held, so they may change the tree. Does nothing for text or anchors. |
| `document_order` | `#[must_use] pub fn document_order(&self) -> Vec<NodeId>` | Every node under the root in document order. Ids are numbered at creation, so ids that increase along the list prove the view created nodes in document order. |
| `live_nodes` | `#[must_use] pub fn live_nodes(&self) -> usize` | Nodes reachable from the root, root included. Returns to 1 after unmounting. |
| `live_listeners` | `#[must_use] pub fn live_listeners(&self) -> usize` | Listeners on reachable nodes. |

Behaviours of the renderer part:

* `insert` first detaches an attached child, then inserts it. **Panics** with `` `before` is not a child of `parent` `` if `before` is not a child. Out-of-range `NodeId` values panic with an index error.
* `set_attr` replaces an existing attribute in place, keeping its position; new ones are appended. Attributes print in insertion order.
* HTML output: attribute values go through `escape_attr`, text through `escape_text`, text inside `<script>`/`<style>` through `escape_raw_text`; void tags write no end tag and no children; a text child that starts with `\n` directly inside `pre`, `textarea` or `listing` gets an extra `\n` in front, since the HTML parser drops the first newline.
* Attributes appear exactly as `mount` set them; `style` appears only for `Element::style`, never for `css`.

## virtual_list

A scrolling list that builds only the rows in view (`quench_ui::virtual_list`). Rows have one fixed height.

```rust
pub const OVERSCAN: usize = 5;

#[must_use]
pub fn virtual_list<T, K>(
    items: &Signal<Vec<T>>,
    item_height: f64,
    key: impl Fn(&T) -> K + 'static,
    render: impl Fn(T) -> View + 'static,
) -> Element
where T: Clone + 'static, K: Into<Key>
```

Returns a `div.q-vlist` that listens for `Scroll` (stores `top`) and `Resize` (stores the height as the viewport). Structure:

```text
div.q-vlist                        on Scroll, Resize
  div.q-vlist-space                css height = len * item_height px
    div.q-vlist-window[role=list]  css --q-row-h = item_height px; css transform = translateY(first * item_height px)
      (keyed) div.q-vlist-row[role=listitem] > render(item)
```

* The first row built is `floor(scroll_top / item_height) - OVERSCAN` (saturating at 0); the last (exclusive) is `floor((scroll_top + viewport) / item_height) + 1 + OVERSCAN`; both clamp to the list length. A memo holds the pair, so the window only rebuilds when the first or last row changes.
* The viewport height is assumed to be 640 px until the first `Resize` event.
* A non-finite or non-positive `item_height` makes every row index 0 (rows from the top); a negative scroll top is treated as 0.
* Rows are matched by `key` as in `View::keyed`; **panics** on duplicate keys among the rows built. Items are cloned out of the signal on each range change.
* The element needs a bounded height from your stylesheet (`.q-vlist` scrolls; give it a fixed height or put it in a flex/grid cell). `.q-vlist-row` reads `--q-row-h`. The `css` properties are not an inline `style`, so they work under a strict CSP but are absent from server-rendered HTML.

```rust
use quench_ui::prelude::*;
use quench_ui::virtual_list::virtual_list;

let rows = create_signal((0..10_000u32).collect::<Vec<_>>());
let list = virtual_list(&rows, 32.0, |n| *n, |n| span().text(format!("row {n}")).into_view());
```
