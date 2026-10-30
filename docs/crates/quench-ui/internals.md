---
title: Internals
parent: quench-ui
nav_order: 11
---

Design notes that affect users: how the reactive graph decides what to run, its limits, how mounting orders and escapes output, and how keyed lists are reconciled. Everything described here is in `reactive.rs`, `mount.rs`, `html.rs` and `view.rs`.

## The reactive graph

### Nodes

* A **signal** holds a value and a list of weak references to the computations that read it (its observers).
* A **computation** is an effect or a memo. It has a state (`Clean`, `Check`, `Dirty`), the list of sources it read on its last run (strong references), a list of weak observers (memos can be observed), the boxed closure, and its own `Scope`.
* A **scope** owns computations, child scopes, cleanups and contexts. A computation's closure runs with the computation as the current observer and its scope as the current owner.

Observers are held weakly and sources strongly, so a source (a memo) is kept alive by whoever reads it, and an effect is kept alive only by the scope that owns it.

### Push-dirty, pull-check

1. `Signal::set`/`update` calls `notify`: each live observer is marked `Dirty` (`stale`).
2. Marking a clean **effect** pushes it onto the run queue once. Marking a clean **memo** also marks every observer behind it `Check` (recursively), but does not queue the memo.
3. Unless a batch is open or a flush is already running, `flush` pops effects from the queue and calls `update_if_necessary` on each.
4. `update_if_necessary` on a `Check` computation first refreshes each of its sources in order (`Memo` sources recompute if stale). If a source's change marked this computation `Dirty` (a memo whose value really differs marks its live observers `Dirty` after it runs), the loop stops early. A `Dirty` computation runs; a computation still `Check` goes back to `Clean` without running.
5. A memo's closure returns "changed" (`new != old`). Only if it changed are its observers marked `Dirty`.

Consequences:

* **Diamond**: with `a -> b, c -> d`, a write to `a` marks `b` and `c` dirty and `d` check; `d` is queued once, refreshes `b` and `c` first and then runs once, never seeing one updated and the other stale.
* **Memo gating**: a memo that recomputes to an equal value does not run its observers.
* **Dynamic dependencies**: before every run a computation unsubscribes from all of its sources and its scope is reset, then re-tracks while running. Dependencies follow the branch taken.
* `Memo::get` outside an effect refreshes the memo first, so a read is never stale even when no flush has happened yet (inside a batch, for example).
* Reads happen in `with`/`get`: `Signal::with` subscribes first and then borrows; `track` ignores a source that is already in the observer's list.

### Batching and the run queue

`batch` increments a depth counter; when the outermost batch ends (and the thread is not panicking) the queue is flushed. Writes inside a batch only mark and enqueue. Each created computation runs its first execution inside a batch, so its own writes are queued until it finishes. Writes made by an effect while a flush is running do not start a nested flush; they enqueue and the running flush loop picks them up.

### The 100,000-run limit

`flush` counts how many queue entries it processed in the current flush. The constant is `FLUSH_LIMIT = 100_000`; when `ran` reaches it (`assert!(ran < FLUSH_LIMIT, "reactive cycle: effects keep triggering each other")`) the call panics with that message. The limit therefore trips on the 100,000th queue entry of a single flush (99,999 entries are processed). It is a limit on effect runs per flush, not a cycle detector: a legitimate flush with more than 99,999 queued effect runs would panic too. Effects that write a signal they read (`s.set(s.get() + 1)`) are the usual cause.

### Panics

* A panic in an effect or memo closure propagates to whoever triggered the flush (`Signal::set`, the end of `batch`, `Scope::run`, ...). The runtime restores itself: the observer/owner are restored by a drop guard, the `flushing` flag is cleared by a drop guard, and `batch`'s depth counter is decremented in `Drop`; during unwinding the batch does not flush. Effects still in the queue stay queued and run at the next flush. The computation that panicked has had its closure taken out; it keeps its state (it does not run again).
* A panic in a closure inside `mount` propagates likewise from the `mount` call or the write that triggered it.

### Disposal

* `Effect::dispose`/`Scope::dispose` set a `disposed` flag (making a second call a no-op), unsubscribe from sources, drop the closure (and everything it captured, which the tests check through `Rc` strong counts) and detach from the parent.
* `Scope::reset` (and, before each re-run, the computation's own scope) disposes child scopes, then computations, then runs cleanups in reverse registration order, then clears contexts.
* A disposed computation is skipped by `stale`, `update` and `flush`.

## Mounting

`mount` calls `build` recursively. Reactive parts are effects created in the mount's scope:

* `DynText`, dynamic attributes, `class` lists with at least one `class_if`, `css` properties and `focus_if` closures get one effect each. The effect either creates the node on its first run or patches it afterwards, so a signal change causes exactly one renderer call (one `set_text`, `set_attr`, `remove_attr` or `set_style`).
* `Dynamic`: the effect calls the view function (tracked), then, untracked, removes the previous content, builds the new content before the region's anchor, and creates the anchor after the first content.
* `Keyed`: see below.

Inside `build` all work happens under `untrack`, so only the explicit closures (`f()` of a region, text, attribute) track.

### Document order

The renderer sees creation calls in the order the nodes appear in the resulting document. An element is created first; then its attributes, listeners and children are built; its `insert` into the parent is the last call. A region builds its first content and only then creates its anchor. Hydration of server-rendered markup depends on this. The exception is a keyed update with surviving items: new items are built from the end of the list backwards.

### Escaping

Escaping is split in two places:

* **mount**: attribute names are checked (`is_valid_attr_name`: ASCII alphanumerics and `-_:.`, not starting with `on` in any case), values of URL attributes are replaced by `about:invalid#blocked` when, after removing control characters and whitespace and lower-casing, they start with `javascript:`, `vbscript:`, or `data:` that is not `data:image/`; style and `css` property names are limited to ASCII letters, digits and `-`; tag names are checked in `element()`. The checks run on every reactive update as well.
* **renderer output** (`TestRenderer`, thus `render_to_string`): text is escaped (`& < >`), attribute values (`& < > " '`, NUL to U+FFFD), `<script>`/`<style>` text with `escape_raw_text`. A renderer that writes markup must apply equivalent escaping itself; `mount` passes raw strings.

Event handlers can only be attached as listeners (`on*`); markup attributes named `on...` are dropped, which also suits a strict Content-Security-Policy.

## Keyed reconciliation

`View::keyed` yields `(Key, FnOnce() -> View)` pairs. The region keeps `Entry { key, part, scope }` per item (`part` is the nodes it built, `scope` a child of the scope the list was built in, so entries survive re-runs of the list's own effect).

On each run `reconcile`:

1. Asserts there are no duplicate keys (`"duplicate key in keyed list: {key:?}"`).
2. Splits the old entries: those whose key is absent from the new list are disposed (scope first, then nodes removed); survivors are indexed by key with their old position.
3. If there are no survivors, builds all items front to back before the end anchor (document order).
4. Otherwise computes the **longest strictly increasing subsequence** (O(n log n), patience sorting with `partition_point`) of the survivors' old positions listed in their new order. Survivors in that subsequence stay where they are; the others will be moved.
5. Walks the new list **backwards**, keeping the first node of the entry placed after the current one (initially the region's end anchor, or the node the region was built before). A survivor not in the subsequence has all its nodes `insert`ed before that node (a move). A new key is built (`make()` in a fresh scope) before it. Entries that are in the subsequence are untouched.

So the number of renderer moves equals the number of survivors outside one longest increasing subsequence, and no surviving node is recreated. An entry that produced no nodes (an empty view) does not update the insertion point.

## Virtual list

`virtual_list` keeps `scroll` and `viewport` signals and a memo of `(first, last)`; the keyed rows read the memo, so scrolling inside the same range does not touch the DOM. The positioning is done with `css` (style object) properties, not inline styles.

## Thread model

All state lives in a `thread_local!` runtime (observer, owner, batch depth, queue, flushing flag) and in `Rc`/`RefCell`. Nothing is `Send`. Separate threads have separate runtimes; a signal used on another thread than it was created on is not supported (the types do not allow moving it).
