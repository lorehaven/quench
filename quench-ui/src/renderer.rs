//! What a backend implements to show a view tree.
//!
//! Methods take `&self`: a renderer is shared with the effects that patch it later, so it keeps
//! its own interior mutability. None of them may call back into reactive code.

use crate::event::{EventKind, Handler};

/// How a listener treats the browser's default action for its event.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Prevent {
    /// Always prevent it.
    Always,
    /// For a link-like element: handle a plain left click and prevent its default (the navigation),
    /// but leave modified or non-primary clicks (open in a new tab, ctrl-click) to the browser,
    /// without running the listener for them.
    PlainClick,
}

/// A tree of nodes that views are built into: the DOM, an in-memory tree, a native widget tree.
pub trait Renderer: 'static {
    /// A handle to a node. Cheap to clone.
    type Node: Clone + 'static;

    fn create_element(&self, tag: &str) -> Self::Node;
    fn create_text(&self, text: &str) -> Self::Node;
    /// An invisible node that marks a position where a dynamic region inserts its content.
    fn create_anchor(&self) -> Self::Node;
    fn set_text(&self, node: &Self::Node, text: &str);
    fn set_attr(&self, node: &Self::Node, name: &str, value: &str);
    fn remove_attr(&self, node: &Self::Node, name: &str);
    /// Inserts `child` into `parent` before `before`, or at the end. An attached `child` moves.
    fn insert(&self, parent: &Self::Node, child: &Self::Node, before: Option<&Self::Node>);
    fn remove(&self, parent: &Self::Node, child: &Self::Node);
    fn listen(&self, node: &Self::Node, kind: EventKind, handler: Handler);

    /// Sets how `node`'s listeners for `kind` treat the default action. Called after `listen`.
    /// A renderer with no default actions, such as a test tree, ignores it.
    fn prevent_default(&self, _node: &Self::Node, _kind: EventKind, _how: Prevent) {}

    /// Sets one CSS property on `node` through the style object, not the `style` attribute, so
    /// it works under a policy that forbids inline styles. An empty `value` clears it.
    fn set_style(&self, _node: &Self::Node, _property: &str, _value: &str) {}

    /// Moves keyboard focus to `node`.
    fn focus(&self, _node: &Self::Node) {}

    /// Keeps keyboard focus inside `node` (Tab and Shift+Tab cycle through what it contains) until
    /// the node is released.
    fn trap_focus(&self, _node: &Self::Node) {}
}
