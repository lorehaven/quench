//! The [`Renderer`] for the browser DOM.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use quench_ui::{Event, EventKind, Handler, Renderer};

use crate::events::kind_code;
use crate::host::{self, ROOT};

type Listeners = HashMap<u32, Vec<(EventKind, Handler)>>;

thread_local! {
    static LISTENERS: RefCell<Listeners> = RefCell::new(HashMap::new());
}

#[cfg(not(target_arch = "wasm32"))]
pub fn reset() {
    // Dropping a listener can drop a node handle, which edits this table: do it after the borrow.
    let old = LISTENERS.with(|l| std::mem::take(&mut *l.borrow_mut()));
    drop(old);
}

/// A DOM node. Cloning is cheap; when the last clone is dropped the handle is released, which
/// frees the slot in the JS handle table and removes the node's listeners.
#[derive(Clone)]
pub struct Node(Rc<Handle>);

struct Handle {
    id: u32,
    owned: bool,
}

impl Drop for Handle {
    fn drop(&mut self) {
        if self.owned {
            // Dropped during thread shutdown the tables may already be gone: nothing to tidy then.
            let _ = LISTENERS.try_with(|l| {
                l.borrow_mut().remove(&self.id);
            });
            host::release(self.id);
        }
    }
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node({})", self.0.id)
    }
}

impl Node {
    fn owned(id: u32) -> Self {
        Self(Rc::new(Handle { id, owned: true }))
    }

    /// The handle number, as `quench.js` knows it.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.0.id
    }
}

/// Builds views into the page's DOM through `quench.js`.
#[derive(Clone, Copy, Debug, Default)]
pub struct DomRenderer;

impl DomRenderer {
    /// The container views are mounted into: the element with id `app`, or `document.body`.
    #[must_use]
    pub fn root() -> Node {
        Node(Rc::new(Handle {
            id: ROOT,
            owned: false,
        }))
    }
}

impl Renderer for DomRenderer {
    type Node = Node;

    fn create_element(&self, tag: &str) -> Node {
        Node::owned(host::create_element(tag))
    }

    fn create_text(&self, text: &str) -> Node {
        Node::owned(host::create_text(text))
    }

    fn create_anchor(&self) -> Node {
        Node::owned(host::create_anchor())
    }

    fn set_text(&self, node: &Node, text: &str) {
        host::set_text(node.id(), text);
    }

    fn set_attr(&self, node: &Node, name: &str, value: &str) {
        host::set_attr(node.id(), name, value);
    }

    fn remove_attr(&self, node: &Node, name: &str) {
        host::remove_attr(node.id(), name);
    }

    fn insert(&self, parent: &Node, child: &Node, before: Option<&Node>) {
        host::insert(parent.id(), child.id(), before.map_or(0, Node::id));
    }

    fn remove(&self, parent: &Node, child: &Node) {
        host::remove(parent.id(), child.id());
    }

    fn listen(&self, node: &Node, kind: EventKind, handler: Handler) {
        let first_of_kind = LISTENERS.with(|l| {
            let mut l = l.borrow_mut();
            let entry = l.entry(node.id()).or_default();
            let first = !entry.iter().any(|(k, _)| *k == kind);
            entry.push((kind, handler));
            first
        });
        if first_of_kind {
            host::listen(node.id(), kind_code(kind));
        }
    }
}

/// Delivers `event` to the listeners registered on `handle`. Unknown handles are ignored. The
/// listeners run with no borrow held, so they can change the DOM and add listeners.
pub fn dispatch(handle: u32, event: &Event) {
    let handlers: Vec<Handler> = LISTENERS.with(|l| {
        l.borrow().get(&handle).map_or_else(Vec::new, |entry| {
            entry
                .iter()
                .filter(|(k, _)| *k == event.kind())
                .map(|(_, h)| Rc::clone(h))
                .collect()
        })
    });
    for handler in handlers {
        handler(event);
    }
}
