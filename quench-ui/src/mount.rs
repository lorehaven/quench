//! Builds a [`View`] into a [`Renderer`] and keeps it up to date.
//!
//! Each reactive part becomes an effect that owns its nodes and patches only them. A dynamic or
//! keyed region is delimited by an anchor node, so it can be rebuilt in place without knowing
//! what surrounds it.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

use crate::event::{Event, EventKind};
use crate::html::{is_valid_attr_name, is_void, sanitize_attr_value};
use crate::reactive::{Scope, batch, create_effect, untrack};
use crate::renderer::Renderer;
use crate::view::{Attr, Class, Element, IntoView, Key, KeyedItem, View};

/// The nodes a view built, as far as removing them is concerned.
enum Part<N> {
    None,
    Node(N),
    /// An element and what was built inside it. A renderer whose node handles own resources (a
    /// DOM backend's listeners, say) needs them to live exactly as long as the element is attached.
    Element(N, Vec<Self>),
    Many(Vec<Self>),
    /// A dynamic or keyed region: its current content and the anchor after it.
    Region(Rc<Region<N>>),
}

struct Region<N> {
    anchor: N,
    content: RefCell<Part<N>>,
}

impl<N: Clone> Clone for Part<N> {
    fn clone(&self) -> Self {
        match self {
            Self::None => Self::None,
            Self::Node(n) => Self::Node(n.clone()),
            Self::Element(n, children) => Self::Element(n.clone(), children.clone()),
            Self::Many(v) => Self::Many(v.clone()),
            Self::Region(r) => Self::Region(Rc::clone(r)),
        }
    }
}

impl<N: Clone> Part<N> {
    /// Every node currently in the part, in document order.
    fn nodes(&self, out: &mut Vec<N>) {
        match self {
            Self::None => {}
            Self::Node(n) | Self::Element(n, _) => out.push(n.clone()),
            Self::Many(parts) => parts.iter().for_each(|p| p.nodes(out)),
            Self::Region(region) => {
                region.content.borrow().nodes(out);
                out.push(region.anchor.clone());
            }
        }
    }
}

fn remove_part<R: Renderer>(r: &R, parent: &R::Node, part: &Part<R::Node>) {
    let mut nodes = Vec::new();
    part.nodes(&mut nodes);
    for node in &nodes {
        r.remove(parent, node);
    }
}

/// A mounted view.
///
/// Call [`Mount::unmount`] to remove it and dispose its effects, or [`Mount::leak`] to keep it for
/// good. Dropping it without either leaves the DOM as it is but releases the renderer's handles
/// for static nodes.
pub struct Mount<R: Renderer> {
    renderer: Rc<R>,
    parent: R::Node,
    part: Part<R::Node>,
    scope: Scope,
}

impl<R: Renderer> fmt::Debug for Mount<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mount")
            .field("scope", &self.scope)
            .finish_non_exhaustive()
    }
}

impl<R: Renderer> Mount<R> {
    /// Removes the view's nodes and disposes everything it created.
    pub fn unmount(self) {
        self.scope.dispose();
        remove_part(&*self.renderer, &self.parent, &self.part);
    }

    /// Keeps the view in place and live for the rest of the program: the nodes and what they own
    /// are never released. For an app that runs until the page closes.
    pub const fn leak(self) {
        std::mem::forget(self);
    }

    /// The scope that owns the view's effects.
    #[must_use]
    pub const fn scope(&self) -> &Scope {
        &self.scope
    }
}

/// Builds `view` as the last children of `parent`.
pub fn mount<R: Renderer>(renderer: &Rc<R>, parent: &R::Node, view: impl IntoView) -> Mount<R> {
    let scope = Scope::new();
    let view = view.into_view();
    let part = scope.run(|| untrack(|| build(renderer, view, parent, None)));
    Mount {
        renderer: Rc::clone(renderer),
        parent: parent.clone(),
        part,
        scope,
    }
}

fn build<R: Renderer>(
    r: &Rc<R>,
    view: View,
    parent: &R::Node,
    before: Option<&R::Node>,
) -> Part<R::Node> {
    match view {
        View::Empty => Part::None,
        View::Text(text) => {
            let node = r.create_text(&text);
            r.insert(parent, &node, before);
            Part::Node(node)
        }
        View::DynText(f) => build_dyn_text(r, f, parent, before),
        View::Element(el) => build_element(r, *el, parent, before),
        View::Fragment(views) => Part::Many(
            views
                .into_iter()
                .map(|v| build(r, v, parent, before))
                .collect(),
        ),
        View::Dynamic(f) => build_dynamic(r, f, parent, before),
        View::Keyed(source) => build_keyed(r, source, parent, before),
    }
}

fn build_dyn_text<R: Renderer>(
    r: &Rc<R>,
    f: Rc<dyn Fn() -> String>,
    parent: &R::Node,
    before: Option<&R::Node>,
) -> Part<R::Node> {
    let slot: Rc<RefCell<Option<R::Node>>> = Rc::new(RefCell::new(None));
    let (r2, slot2, parent2, before2) = (
        Rc::clone(r),
        Rc::clone(&slot),
        parent.clone(),
        before.cloned(),
    );
    create_effect(move || {
        let text = f();
        let mut slot = slot2.borrow_mut();
        if let Some(node) = &*slot {
            r2.set_text(node, &text);
        } else {
            let node = r2.create_text(&text);
            r2.insert(&parent2, &node, before2.as_ref());
            *slot = Some(node);
        }
    });
    let node = slot.borrow().clone().expect("effect runs once on creation");
    Part::Node(node)
}

fn set_attr_checked<R: Renderer>(r: &R, node: &R::Node, name: &str, value: &str) {
    r.set_attr(node, name, sanitize_attr_value(name, value));
}

fn build_element<R: Renderer>(
    r: &Rc<R>,
    el: Element,
    parent: &R::Node,
    before: Option<&R::Node>,
) -> Part<R::Node> {
    let node = r.create_element(&el.tag);

    for (name, attr) in el.attrs {
        if !is_valid_attr_name(&name) {
            continue;
        }
        match attr {
            Attr::Static(value) => set_attr_checked(&**r, &node, &name, &value),
            Attr::Dynamic(f) => {
                let (r2, node2) = (Rc::clone(r), node.clone());
                create_effect(move || match f() {
                    Some(value) => set_attr_checked(&*r2, &node2, &name, &value),
                    None => r2.remove_attr(&node2, &name),
                });
            }
        }
    }

    build_classes(r, &node, el.classes);

    let style: Vec<String> = el
        .styles
        .iter()
        .filter(|(p, _)| p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        .map(|(p, v)| format!("{p}: {v}"))
        .collect();
    if !style.is_empty() {
        r.set_attr(&node, "style", &style.join("; "));
    }

    for (kind, handler) in el.handlers {
        r.listen(&node, kind, wrap_handler(kind, handler));
    }

    let children: Vec<Part<R::Node>> = if is_void(&el.tag) {
        Vec::new()
    } else {
        el.children
            .into_iter()
            .map(|child| build(r, child, &node, None))
            .collect()
    };
    r.insert(parent, &node, before);
    Part::Element(node, children)
}

/// Runs the listener in a batch, so several writes cause one update.
fn wrap_handler(kind: EventKind, handler: crate::event::Handler) -> crate::event::Handler {
    Rc::new(move |event: &Event| {
        debug_assert_eq!(event.kind(), kind, "event delivered to the wrong listener");
        batch(|| handler(event));
    })
}

fn build_classes<R: Renderer>(r: &Rc<R>, node: &R::Node, classes: Vec<Class>) {
    if classes.is_empty() {
        return;
    }
    let dynamic = classes.iter().any(|c| c.when.is_some());
    let join = move |classes: &[Class]| {
        classes
            .iter()
            .filter(|c| c.when.as_ref().is_none_or(|w| w()))
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    };
    if dynamic {
        let (r2, node2) = (Rc::clone(r), node.clone());
        create_effect(move || {
            let value = join(&classes);
            if value.is_empty() {
                r2.remove_attr(&node2, "class");
            } else {
                r2.set_attr(&node2, "class", &value);
            }
        });
    } else {
        r.set_attr(node, "class", &join(&classes));
    }
}

fn build_dynamic<R: Renderer>(
    r: &Rc<R>,
    f: Rc<dyn Fn() -> View>,
    parent: &R::Node,
    before: Option<&R::Node>,
) -> Part<R::Node> {
    let anchor = r.create_anchor();
    r.insert(parent, &anchor, before);
    let region = Rc::new(Region {
        anchor,
        content: RefCell::new(Part::None),
    });
    let (r2, parent2, region2) = (Rc::clone(r), parent.clone(), Rc::clone(&region));
    create_effect(move || {
        let view = f();
        untrack(|| {
            let old = region2.content.replace(Part::None);
            remove_part(&*r2, &parent2, &old);
            let part = build(&r2, view, &parent2, Some(&region2.anchor));
            *region2.content.borrow_mut() = part;
        });
    });
    Part::Region(region)
}

struct Entry<N> {
    key: Key,
    part: Part<N>,
    scope: Scope,
}

fn build_keyed<R: Renderer>(
    r: &Rc<R>,
    source: Rc<dyn Fn() -> Vec<KeyedItem>>,
    parent: &R::Node,
    before: Option<&R::Node>,
) -> Part<R::Node> {
    let anchor = r.create_anchor();
    r.insert(parent, &anchor, before);
    let region = Rc::new(Region {
        anchor,
        content: RefCell::new(Part::None),
    });
    // Entries must outlive each re-run of the effect below, which resets its own scope, so they
    // hang off the scope the list was built in.
    let outer = Scope::current();
    let entries: Rc<RefCell<Vec<Entry<R::Node>>>> = Rc::new(RefCell::new(Vec::new()));
    let (r2, parent2, region2) = (Rc::clone(r), parent.clone(), Rc::clone(&region));
    create_effect(move || {
        let items = source();
        untrack(|| {
            let old = std::mem::take(&mut *entries.borrow_mut());
            let new = reconcile(&r2, &parent2, &region2.anchor, old, items, outer.as_ref());
            *region2.content.borrow_mut() =
                Part::Many(new.iter().map(|e| e.part.clone()).collect());
            *entries.borrow_mut() = new;
        });
    });
    Part::Region(region)
}

/// Turns the old entries into the new list: drops what is gone, builds what is new, and moves only
/// what is out of place.
fn reconcile<R: Renderer>(
    r: &Rc<R>,
    parent: &R::Node,
    anchor: &R::Node,
    old: Vec<Entry<R::Node>>,
    items: Vec<KeyedItem>,
    outer: Option<&Scope>,
) -> Vec<Entry<R::Node>> {
    let mut seen = HashSet::new();
    for (key, _) in &items {
        assert!(seen.insert(key), "duplicate key in keyed list: {key:?}");
    }

    // Survivors in their old order.
    let mut old_index: HashMap<Key, usize> = HashMap::new();
    let mut by_key: HashMap<Key, Entry<R::Node>> = HashMap::new();
    for entry in old {
        if seen.contains(&entry.key) {
            old_index.insert(entry.key.clone(), old_index.len());
            by_key.insert(entry.key.clone(), entry);
        } else {
            entry.scope.dispose();
            remove_part(&**r, parent, &entry.part);
        }
    }
    // The survivors that already appear in the right order stay where they are; the rest move.
    let order: Vec<(usize, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(at, (key, _))| old_index.get(key).map(|old| (at, *old)))
        .collect();
    let stays: HashSet<usize> =
        longest_increasing(&order.iter().map(|(_, old)| *old).collect::<Vec<_>>())
            .into_iter()
            .map(|i| order[i].0)
            .collect();

    // Walk the new order backwards, so each entry is placed relative to the one after it.
    let mut placed: Vec<Entry<R::Node>> = Vec::with_capacity(items.len());
    let mut next_node = anchor.clone();
    for (at, (key, make)) in items.into_iter().enumerate().rev() {
        let entry = if let Some(entry) = by_key.remove(&key) {
            if !stays.contains(&at) {
                let mut nodes = Vec::new();
                entry.part.nodes(&mut nodes);
                for node in &nodes {
                    r.insert(parent, node, Some(&next_node));
                }
            }
            entry
        } else {
            let scope = outer.map_or_else(Scope::new, Scope::new_in);
            let part = scope.run(|| build(r, make(), parent, Some(&next_node)));
            Entry { key, part, scope }
        };
        let mut nodes = Vec::new();
        entry.part.nodes(&mut nodes);
        if let Some(first) = nodes.first() {
            next_node = first.clone();
        }
        placed.push(entry);
    }
    placed.reverse();
    placed
}

/// Indices into `values` of one longest strictly increasing subsequence.
fn longest_increasing(values: &[usize]) -> Vec<usize> {
    // `tails[k]` is the index of the smallest tail of an increasing run of length k + 1.
    let mut tails: Vec<usize> = Vec::new();
    let mut previous: Vec<Option<usize>> = vec![None; values.len()];
    for (i, value) in values.iter().enumerate() {
        let k = tails.partition_point(|t| values[*t] < *value);
        if k > 0 {
            previous[i] = Some(tails[k - 1]);
        }
        if k == tails.len() {
            tails.push(i);
        } else {
            tails[k] = i;
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(i) = at {
        out.push(i);
        at = previous[i];
    }
    out
}
