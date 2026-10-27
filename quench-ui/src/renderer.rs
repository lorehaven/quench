//! What a backend implements to show a view tree.
//!
//! Methods take `&self`: a renderer is shared with the effects that patch it later, so it keeps
//! its own interior mutability. None of them may call back into reactive code.

use crate::event::{EventKind, Handler};

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
}
