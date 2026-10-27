//! An in-memory [`Renderer`]: a real node tree you can query, fire events at, and read back as
//! HTML, plus a log of every operation applied to it.

use std::cell::RefCell;

use crate::event::{Event, EventKind, Handler};
use crate::html::{escape_attr, escape_raw_text, escape_text, is_void};
use crate::renderer::Renderer;

/// A node in a [`TestRenderer`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(usize);

/// One change applied to the tree, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    CreateElement(String),
    CreateText(String),
    CreateAnchor,
    SetText(NodeId, String),
    SetAttr(NodeId, String, String),
    RemoveAttr(NodeId, String),
    Insert {
        parent: NodeId,
        child: NodeId,
        before: Option<NodeId>,
    },
    Remove {
        parent: NodeId,
        child: NodeId,
    },
    Listen(NodeId, EventKind),
}

type Matcher<'a> = &'a dyn Fn(&str, &[(String, String)]) -> bool;

enum Kind {
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        handlers: Vec<(EventKind, Handler)>,
    },
    Text(String),
    Anchor,
}

struct Node {
    kind: Kind,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
}

struct Tree {
    nodes: Vec<Node>,
    ops: Vec<Op>,
}

impl Tree {
    fn push(&mut self, kind: Kind) -> NodeId {
        self.nodes.push(Node {
            kind,
            parent: None,
            children: Vec::new(),
        });
        NodeId(self.nodes.len() - 1)
    }

    fn detach(&mut self, child: NodeId) {
        if let Some(parent) = self.nodes[child.0].parent.take() {
            self.nodes[parent.0].children.retain(|c| *c != child);
        }
    }
}

/// An in-memory tree that records what is done to it.
pub struct TestRenderer {
    tree: RefCell<Tree>,
}

impl std::fmt::Debug for TestRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestRenderer")
            .field("nodes", &self.tree.borrow().nodes.len())
            .finish()
    }
}

impl Default for TestRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl TestRenderer {
    /// A tree holding just its root element.
    #[must_use]
    pub fn new() -> Self {
        let mut tree = Tree {
            nodes: Vec::new(),
            ops: Vec::new(),
        };
        tree.push(Kind::Element {
            tag: "#root".to_string(),
            attrs: Vec::new(),
            handlers: Vec::new(),
        });
        Self {
            tree: RefCell::new(tree),
        }
    }

    /// The container views are mounted into.
    #[must_use]
    pub const fn root(&self) -> NodeId {
        NodeId(0)
    }

    /// Every operation applied since creation or the last [`TestRenderer::clear_ops`].
    #[must_use]
    pub fn ops(&self) -> Vec<Op> {
        self.tree.borrow().ops.clone()
    }

    pub fn clear_ops(&self) {
        self.tree.borrow_mut().ops.clear();
    }

    /// The root's content as HTML. Anchors are written as `<!---->`.
    #[must_use]
    pub fn html(&self) -> String {
        let tree = self.tree.borrow();
        let mut out = String::new();
        for child in &tree.nodes[0].children {
            write_html(&tree, *child, false, &mut out);
        }
        out
    }

    /// The HTML of one node, itself included.
    #[must_use]
    pub fn outer_html(&self, node: NodeId) -> String {
        let tree = self.tree.borrow();
        let mut out = String::new();
        write_html(&tree, node, false, &mut out);
        out
    }

    /// All text under the root, concatenated.
    #[must_use]
    pub fn text(&self) -> String {
        self.text_of(self.root())
    }

    /// All text under `node`, concatenated.
    #[must_use]
    pub fn text_of(&self, node: NodeId) -> String {
        fn walk(tree: &Tree, id: NodeId, out: &mut String) {
            match &tree.nodes[id.0].kind {
                Kind::Text(t) => out.push_str(t),
                Kind::Element { .. } => {
                    for child in &tree.nodes[id.0].children {
                        walk(tree, *child, out);
                    }
                }
                Kind::Anchor => {}
            }
        }
        let mut out = String::new();
        walk(&self.tree.borrow(), node, &mut out);
        out
    }

    #[must_use]
    pub fn children(&self, node: NodeId) -> Vec<NodeId> {
        self.tree.borrow().nodes[node.0].children.clone()
    }

    #[must_use]
    pub fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.tree.borrow().nodes[node.0].parent
    }

    /// The tag of an element node.
    #[must_use]
    pub fn tag(&self, node: NodeId) -> Option<String> {
        match &self.tree.borrow().nodes[node.0].kind {
            Kind::Element { tag, .. } => Some(tag.clone()),
            _ => None,
        }
    }

    /// The value of an attribute on an element node.
    #[must_use]
    pub fn attr(&self, node: NodeId, name: &str) -> Option<String> {
        match &self.tree.borrow().nodes[node.0].kind {
            Kind::Element { attrs, .. } => attrs
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.clone()),
            _ => None,
        }
    }

    /// Elements under the root with this tag, in document order.
    #[must_use]
    pub fn find_by_tag(&self, tag: &str) -> Vec<NodeId> {
        self.find(|t, _| t == tag)
    }

    /// Elements under the root whose attribute `name` equals `value`, in document order.
    #[must_use]
    pub fn find_by_attr(&self, name: &str, value: &str) -> Vec<NodeId> {
        self.find(|_, attrs| attrs.iter().any(|(n, v)| n == name && v == value))
    }

    fn find(&self, matches: impl Fn(&str, &[(String, String)]) -> bool) -> Vec<NodeId> {
        fn walk(tree: &Tree, id: NodeId, matches: Matcher<'_>, out: &mut Vec<NodeId>) {
            if let Kind::Element { tag, attrs, .. } = &tree.nodes[id.0].kind
                && id.0 != 0
                && matches(tag, attrs)
            {
                out.push(id);
            }
            for child in &tree.nodes[id.0].children {
                walk(tree, *child, matches, out);
            }
        }
        let mut out = Vec::new();
        walk(&self.tree.borrow(), self.root(), &matches, &mut out);
        out
    }

    /// Delivers `event` to the listeners on `node`. The listeners run with no borrow held, so
    /// they can change the tree.
    pub fn fire(&self, node: NodeId, event: &Event) {
        let handlers: Vec<Handler> = match &self.tree.borrow().nodes[node.0].kind {
            Kind::Element { handlers, .. } => handlers
                .iter()
                .filter(|(k, _)| *k == event.kind())
                .map(|(_, h)| h.clone())
                .collect(),
            _ => Vec::new(),
        };
        for handler in handlers {
            handler(event);
        }
    }

    /// Nodes reachable from the root, the root included. After unmounting a view this returns
    /// to 1, so a count that keeps growing is a leak.
    #[must_use]
    pub fn live_nodes(&self) -> usize {
        fn count(tree: &Tree, id: NodeId) -> usize {
            1 + tree.nodes[id.0]
                .children
                .iter()
                .map(|c| count(tree, *c))
                .sum::<usize>()
        }
        count(&self.tree.borrow(), self.root())
    }

    /// Listeners registered on reachable nodes.
    #[must_use]
    pub fn live_listeners(&self) -> usize {
        fn count(tree: &Tree, id: NodeId) -> usize {
            let own = match &tree.nodes[id.0].kind {
                Kind::Element { handlers, .. } => handlers.len(),
                _ => 0,
            };
            own + tree.nodes[id.0]
                .children
                .iter()
                .map(|c| count(tree, *c))
                .sum::<usize>()
        }
        count(&self.tree.borrow(), self.root())
    }
}

fn write_html(tree: &Tree, id: NodeId, in_raw: bool, out: &mut String) {
    let node = &tree.nodes[id.0];
    match &node.kind {
        Kind::Text(text) => {
            if in_raw {
                out.push_str(&escape_raw_text(text));
            } else {
                out.push_str(&escape_text(text));
            }
        }
        Kind::Anchor => out.push_str("<!---->"),
        Kind::Element { tag, attrs, .. } => {
            out.push('<');
            out.push_str(tag);
            for (name, value) in attrs {
                out.push(' ');
                out.push_str(name);
                out.push_str("=\"");
                out.push_str(&escape_attr(value));
                out.push('"');
            }
            out.push('>');
            if is_void(tag) {
                return;
            }
            let raw = tag == "script" || tag == "style";
            for child in &node.children {
                write_html(tree, *child, raw, out);
            }
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
    }
}

impl Renderer for TestRenderer {
    type Node = NodeId;

    fn create_element(&self, tag: &str) -> NodeId {
        let mut tree = self.tree.borrow_mut();
        tree.ops.push(Op::CreateElement(tag.to_string()));
        tree.push(Kind::Element {
            tag: tag.to_string(),
            attrs: Vec::new(),
            handlers: Vec::new(),
        })
    }

    fn create_text(&self, text: &str) -> NodeId {
        let mut tree = self.tree.borrow_mut();
        tree.ops.push(Op::CreateText(text.to_string()));
        tree.push(Kind::Text(text.to_string()))
    }

    fn create_anchor(&self) -> NodeId {
        let mut tree = self.tree.borrow_mut();
        tree.ops.push(Op::CreateAnchor);
        tree.push(Kind::Anchor)
    }

    fn set_text(&self, node: &NodeId, text: &str) {
        let mut tree = self.tree.borrow_mut();
        tree.ops.push(Op::SetText(*node, text.to_string()));
        if let Kind::Text(t) = &mut tree.nodes[node.0].kind {
            *t = text.to_string();
        }
    }

    fn set_attr(&self, node: &NodeId, name: &str, value: &str) {
        let mut tree = self.tree.borrow_mut();
        tree.ops
            .push(Op::SetAttr(*node, name.to_string(), value.to_string()));
        if let Kind::Element { attrs, .. } = &mut tree.nodes[node.0].kind {
            if let Some(slot) = attrs.iter_mut().find(|(n, _)| n == name) {
                slot.1 = value.to_string();
            } else {
                attrs.push((name.to_string(), value.to_string()));
            }
        }
    }

    fn remove_attr(&self, node: &NodeId, name: &str) {
        let mut tree = self.tree.borrow_mut();
        tree.ops.push(Op::RemoveAttr(*node, name.to_string()));
        if let Kind::Element { attrs, .. } = &mut tree.nodes[node.0].kind {
            attrs.retain(|(n, _)| n != name);
        }
    }

    fn insert(&self, parent: &NodeId, child: &NodeId, before: Option<&NodeId>) {
        let mut tree = self.tree.borrow_mut();
        tree.ops.push(Op::Insert {
            parent: *parent,
            child: *child,
            before: before.copied(),
        });
        tree.detach(*child);
        let position = match before {
            Some(b) => tree.nodes[parent.0]
                .children
                .iter()
                .position(|c| c == b)
                .expect("`before` is not a child of `parent`"),
            None => tree.nodes[parent.0].children.len(),
        };
        tree.nodes[parent.0].children.insert(position, *child);
        tree.nodes[child.0].parent = Some(*parent);
    }

    fn remove(&self, parent: &NodeId, child: &NodeId) {
        let mut tree = self.tree.borrow_mut();
        tree.ops.push(Op::Remove {
            parent: *parent,
            child: *child,
        });
        if tree.nodes[child.0].parent == Some(*parent) {
            tree.detach(*child);
        }
    }

    fn listen(&self, node: &NodeId, kind: EventKind, handler: Handler) {
        let mut tree = self.tree.borrow_mut();
        tree.ops.push(Op::Listen(*node, kind));
        if let Kind::Element { handlers, .. } = &mut tree.nodes[node.0].kind {
            handlers.push((kind, handler));
        }
    }
}
