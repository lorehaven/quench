---
title: Reactive API
parent: quench-ui
nav_order: 7
---

Reference for `quench_ui::reactive` and the crate-root items built on it. Everything here lives on the current thread; no type is `Send` or `Sync`. For the algorithm see [internals](internals.md).

## Re-exports

`lib.rs` re-exports from `reactive`: `Effect`, `Memo`, `Scope`, `Signal`, `batch`, `create_effect`, `create_memo`, `create_root`, `create_signal`, `on_cleanup`, `provide_context`, `untrack`, `use_context`. The `prelude` module re-exports the same list, plus:

| From | Items |
|---|---|
| `event` | `Event`, `EventKind`, `KeyEvent`, `PointerEvent` |
| `mount` | `Mount`, `mount` |
| `renderer` | `Renderer` |
| `test_renderer` | `TestRenderer` |
| `view` | `Element`, `IntoView`, `Key`, `View`, `element`, `fragment`, and the tag helpers `a`, `button`, `div`, `footer`, `form`, `h1`, `h2`, `h3`, `header`, `i`, `img`, `input`, `label`, `li`, `main`, `nav`, `option`, `p`, `pre`, `section`, `select`, `span`, `strong`, `textarea`, `ul` |

The prelude does not export `render_to_string`, `Handler`, `ScrollEvent`, `Size`, `prevent_default`, `take_prevent_request`, `Prevent`, `Op` or `NodeId`; use `quench_ui::...` for these (`NodeId` only as `quench_ui::test_renderer::NodeId`).

Crate-root items (`quench_ui::...`): `Event`, `EventKind`, `Handler`, `KeyEvent`, `PointerEvent`, `ScrollEvent`, `Size`, `prevent_default`, `take_prevent_request`, `Mount`, `mount`, the reactive list above, `Prevent`, `Renderer`, `Op`, `TestRenderer`, `Element`, `IntoView`, `Key`, `View`, `element`, `fragment`, `render_to_string`. The modules `anim`, `event`, `form`, `gesture`, `html`, `i18n`, `kit`, `platform`, `reactive`, `renderer`, `router`, `test_renderer`, `view`, `virtual_list` and `prelude` are public; `mount` is a private module whose items are re-exported.

### render_to_string

```rust
#[must_use]
pub fn render_to_string(view: impl IntoView) -> String
```

Builds `view` into a fresh `TestRenderer` inside a new root scope, reads the HTML, unmounts and disposes the scope, and returns the HTML. Reactive parts render their current value; listeners and effects are discarded. Dynamic and keyed regions end in a `<!---->` anchor. `Element::css` properties are not part of the output (they go through `Renderer::set_style`, not an attribute). See [api-render](api-render.md) for the escaping rules.

```rust
use quench_ui::{render_to_string, prelude::*};
assert_eq!(render_to_string(div().class("a").child("x < y")), r#"<div class="a">x &lt; y</div>"#);
assert_eq!(render_to_string(View::show(|| true, || p().text("hi").into_view())), "<p>hi</p><!---->");
```

## Free functions

### create_signal

```rust
#[must_use]
pub fn create_signal<T: 'static>(value: T) -> Signal<T>
```

Creates a signal holding `value`. A signal is not owned by a scope: it lives while any clone is alive.

### create_memo

```rust
#[must_use]
pub fn create_memo<T: PartialEq + 'static>(mut f: impl FnMut() -> T + 'static) -> Memo<T>
```

Runs `f` once now, tracking what it reads, and returns a memo. The memo is owned by the current scope (it is disposed with it). Observers are notified only when the new value is not `==` to the old one.

### create_effect

```rust
pub fn create_effect(mut f: impl FnMut() + 'static) -> Effect
```

Runs `f` now and again whenever a signal or memo it read changes. Owned by the current scope and stopped when the scope is disposed or re-runs, or on `Effect::dispose`. Effects created while another effect runs belong to that effect and are disposed each time it re-runs. The first run happens inside a batch, so a write the first run makes to its own sources is queued instead of flushed into the running effect.

### batch

```rust
pub fn batch<T>(f: impl FnOnce() -> T) -> T
```

Runs `f` with effects deferred. Writes are visible to reads inside `f` immediately; each dependent effect runs once after the outermost `batch` returns. Nests (a depth counter). If `f` panics, the depth is restored and no flush happens during the unwind. Event handlers mounted by `mount` run in a batch.

### untrack

```rust
pub fn untrack<T>(f: impl FnOnce() -> T) -> T
```

Runs `f` with no current observer: reads inside do not subscribe the running effect or memo. The current owner scope is unchanged.

### create_root

```rust
pub fn create_root<T>(f: impl FnOnce() -> T) -> (T, Scope)
```

Creates a scope with no parent, runs `f` with it as the owner, and returns the result with the scope. Dispose the scope to release what `f` created.

### on_cleanup

```rust
pub fn on_cleanup(f: impl FnOnce() + 'static)
```

Registers `f` on the current scope. It runs when the scope is disposed or reset, and (for an effect or memo, whose own scope is reset before each re-run) before the next run. Cleanups run in reverse registration order, after child scopes and computations were disposed. With no current scope `f` is dropped without running.

### provide_context

```rust
pub fn provide_context<T: 'static>(value: T)
```

Stores `value` on the current scope, keyed by its type; an earlier value of the same type on the same scope is replaced. **Panics** with `"provide_context needs a scope; wrap the call in create_root"` when there is no current scope.

### use_context

```rust
#[must_use]
pub fn use_context<T: Clone + 'static>() -> Option<T>
```

Returns a clone of the nearest value of type `T` on the current scope or its ancestors, or `None`. Contexts of an effect's scope are cleared when it re-runs.

## Signal<T>

```rust
pub struct Signal<T> { /* Rc<SignalInner<T>> */ }
impl<T> Clone for Signal<T>              // cheap: clones the Rc
impl<T: fmt::Debug> fmt::Debug for Signal<T>   // prints Signal(<value>)
```

| Method | Signature | Behaviour |
|---|---|---|
| `get` | `#[must_use] pub fn get(&self) -> T where T: Clone` | Reads (clone) and subscribes the running computation. |
| `get_untracked` | `#[must_use] pub fn get_untracked(&self) -> T where T: Clone` | Reads without subscribing. |
| `with` | `pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R` | Borrows and subscribes. `f` must not write this signal (the value is borrowed; a write panics with a `RefCell` borrow error). |
| `with_untracked` | `pub fn with_untracked<R>(&self, f: impl FnOnce(&T) -> R) -> R` | Borrows without subscribing. |
| `set` | `pub fn set(&self, value: T)` | Replaces the value and notifies every observer, equal or not. |
| `update` | `pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R` | Mutates in place, notifies after `f` returns, returns `f`'s result. `f` must not read this signal. |
| `set_if_changed` | `pub fn set_if_changed(&self, value: T) where T: PartialEq` | `set` only if `value != current`. |

All methods are in `impl<T: 'static> Signal<T>`. Writing outside a batch flushes the effect queue before the call returns, so effects have already run when `set` returns.

## Memo<T>

```rust
pub struct Memo<T> { /* value + computation */ }
impl<T> Clone for Memo<T>
impl<T: fmt::Debug> fmt::Debug for Memo<T>
```

`impl<T: 'static> Memo<T>`:

| Method | Signature | Behaviour |
|---|---|---|
| `get` | `#[must_use] pub fn get(&self) -> T where T: Clone` | Refreshes if stale, subscribes, returns a clone. |
| `with` | `pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R` | Refreshes, subscribes, borrows. Panics (`"memo computed on creation"`) only if the slot is empty, which cannot happen after creation. |
| `get_untracked` | `#[must_use] pub fn get_untracked(&self) -> T where T: Clone` | Refreshes, returns a clone, does not subscribe. |

A memo has no setter. Reading a disposed memo returns its last value (no recomputation happens because its closure was dropped).

## Effect

```rust
#[derive(Clone)]
pub struct Effect { /* Rc<Computation> */ }
impl fmt::Debug for Effect       // Effect { disposed: bool }
```

| Method | Signature | Behaviour |
|---|---|---|
| `dispose` | `pub fn dispose(&self)` | Unsubscribes, drops the closure and everything it captured, disposes the effect's scope (children, cleanups). Idempotent. |
| `is_disposed` | `#[must_use] pub fn is_disposed(&self) -> bool` | Whether `dispose` ran (directly or through the owner scope). |

Dropping an `Effect` handle does not stop the effect; its owner scope keeps it alive.

## Scope

```rust
#[derive(Clone)]
pub struct Scope { /* Rc<ScopeInner> */ }
impl fmt::Debug for Scope       // Scope { disposed, .. }
impl Default for Scope          // Scope::new()
```

A scope owns effects, memos, cleanups, contexts and child scopes.

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `#[must_use] pub fn new() -> Self` | A scope nested in the current one, or a root when there is none. A nested scope is disposed with its parent. |
| `new_in` | `#[must_use] pub fn new_in(parent: &Self) -> Self` | Nested in `parent`. |
| `current` | `#[must_use] pub fn current() -> Option<Self>` | The current owner scope. |
| `run` | `pub fn run<T>(&self, f: impl FnOnce() -> T) -> T` | Runs `f` with this scope as owner (the observer is unchanged). The previous owner is restored afterwards, also on panic. |
| `dispose` | `pub fn dispose(&self)` | Disposes everything owned and detaches from the parent. Idempotent. |
| `reset` | `pub fn reset(&self)` | Disposes what the scope owns (children, computations, cleanups in reverse, contexts) but keeps the scope usable. |
| `is_disposed` | `#[must_use] pub fn is_disposed(&self) -> bool` | |

Dropping all handles to a scope with no parent drops its computations, which stops them (their observers are weak), without running cleanups. Dispose explicitly.
