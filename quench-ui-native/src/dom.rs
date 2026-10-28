//! The DOM the views are built into.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use quench_css::MediaEnv;
use quench_layout::{Fonts, LayoutBox};
use quench_ui::{EventKind, Handler, Renderer};

/// Fetches the bytes of an image by its `src` (a file, an asset, something already downloaded).
pub type ImageLoader = Rc<dyn Fn(&str) -> Option<Vec<u8>>>;

/// A node of a [`NativeRenderer`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub(crate) usize);

impl NodeId {
    /// The number the layout gives the node's box as its key.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

pub(crate) enum Kind {
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        handlers: Vec<(EventKind, Handler)>,
        styles: Vec<(String, String)>,
    },
    Text(String),
    Anchor,
}

/// A decoded image whose pixels are shared, so a frame does not copy them.
#[derive(Debug, Clone)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Rc<Vec<u8>>,
    /// Device pixels per CSS pixel the picture was made at: 1 for a bitmap, the page's density for a drawing.
    pub scale: f32,
    /// Made from a vector drawing, so it is made again when the density changes.
    pub svg: bool,
}

impl From<quench_image::Image> for Picture {
    fn from(image: quench_image::Image) -> Self {
        Self {
            width: image.width,
            height: image.height,
            rgba: Rc::new(image.rgba),
            scale: 1.0,
            svg: false,
        }
    }
}

pub(crate) struct DomNode {
    pub kind: Kind,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

/// What the pointer and keyboard have done to the document: the state `:hover` and friends read, scroll
/// positions, the caret.
#[derive(Default)]
#[allow(clippy::struct_excessive_bools)] // independent flags of the interaction state
pub(crate) struct Interaction {
    pub hovered: Option<usize>,
    pub active: Option<usize>,
    pub focused: Option<usize>,
    /// Whether focus came from the keyboard (what `:focus-visible` asks).
    pub focus_visible: bool,
    pub trap: Option<usize>,
    /// Scroll offsets by box key.
    pub scroll: HashMap<usize, (f32, f32)>,
    /// The node the pointer went down on, for deciding whether it is a click.
    pub pressed: Option<usize>,
    /// The caret and the other end of the selection, as character indexes in the focused field's value.
    pub caret: usize,
    pub anchor: usize,
    pub clipboard: String,
    /// Set when text was copied or cut here and the host has not yet put it on the system clipboard.
    pub clipboard_out: bool,
    /// An input method's composition in the focused field: its text and the cursor in it (characters).
    pub preedit: Option<(String, usize)>,
    /// Set when what is on screen is out of date.
    pub dirty: bool,
    /// Set when the document or its style changed since the last layout (hover and focus do not count).
    pub stale: bool,
    /// Whether the caret is drawn (the host blinks it).
    pub caret_on: bool,
}

pub(crate) struct Dom {
    pub nodes: Vec<DomNode>,
    pub ui: Interaction,
    pub layout: Option<Rc<LayoutBox>>,
    pub sheets: Vec<String>,
    pub media: MediaEnv,
    pub safe_area: [f32; 4],
    pub viewport: (f32, f32),
    /// Device pixels per CSS pixel.
    pub zoom: f32,
    pub fonts: Fonts,
    /// Decoded images by the `src` they were asked for; `None` for one that could not be loaded.
    pub images: HashMap<String, Option<Picture>>,
    /// Transitions under way and the styles they started from (changed while styles are computed).
    pub anim: std::cell::RefCell<crate::anim::Anim>,
    /// How a `src` that is not a `data:` URL is fetched.
    pub loader: Option<ImageLoader>,
}

impl Dom {
    fn push(&mut self, kind: Kind) -> usize {
        self.nodes.push(DomNode {
            kind,
            parent: None,
            children: Vec::new(),
        });
        self.nodes.len() - 1
    }

    fn detach(&mut self, child: usize) {
        if let Some(parent) = self.nodes[child].parent.take() {
            self.nodes[parent].children.retain(|c| *c != child);
        }
    }

    pub(crate) fn attr(&self, id: usize, name: &str) -> Option<&str> {
        match self.nodes.get(id).map(|n| &n.kind) {
            Some(Kind::Element { attrs, .. }) => attrs
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.as_str()),
            _ => None,
        }
    }

    pub(crate) fn tag(&self, id: usize) -> Option<&str> {
        match self.nodes.get(id).map(|n| &n.kind) {
            Some(Kind::Element { tag, .. }) => Some(tag),
            _ => None,
        }
    }
}

/// What a node is, for code that walks the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Element(String),
    Text(String),
    Anchor,
}

/// A renderer that draws the views into pixels itself.
pub struct NativeRenderer {
    pub(crate) dom: RefCell<Dom>,
    /// The rules of the browser's sheet and the page's, ready to compute styles; dropped when they change.
    pub(crate) cascade: RefCell<Option<std::rc::Rc<quench_css::Cascade>>>,
    /// The clock and timers of the views.
    pub(crate) platform: Rc<crate::timers::NativePlatform>,
    /// What looks at the channels other threads send through.
    pub(crate) pollers: RefCell<Vec<Rc<dyn Fn() -> bool>>>,
}

impl std::fmt::Debug for NativeRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeRenderer")
            .field("nodes", &self.dom.borrow().nodes.len())
            .finish_non_exhaustive()
    }
}

impl Default for NativeRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeRenderer {
    /// A document with `<html><body></body></html>`; views are mounted into the body ([`root`](Self::root)).
    #[must_use]
    pub fn new() -> Self {
        let mut dom = Dom {
            nodes: Vec::new(),
            ui: Interaction {
                caret_on: true,
                dirty: true,
                ..Interaction::default()
            },
            layout: None,
            sheets: Vec::new(),
            media: MediaEnv::default(),
            safe_area: [0.0; 4],
            viewport: (1280.0, 800.0),
            zoom: 1.0,
            images: HashMap::new(),
            anim: std::cell::RefCell::new(crate::anim::Anim::default()),
            loader: None,
            fonts: Fonts::system(),
        };
        let html = dom.push(Kind::Element {
            tag: "html".into(),
            attrs: Vec::new(),
            handlers: Vec::new(),
            styles: Vec::new(),
        });
        let body = dom.push(Kind::Element {
            tag: "body".into(),
            attrs: Vec::new(),
            handlers: Vec::new(),
            styles: Vec::new(),
        });
        dom.nodes[html].children.push(body);
        dom.nodes[body].parent = Some(html);
        let platform = Rc::new(crate::timers::NativePlatform::new());
        quench_ui::platform::install(Rc::clone(&platform) as Rc<dyn quench_ui::platform::Platform>);
        Self {
            dom: RefCell::new(dom),
            cascade: RefCell::new(None),
            platform,
            pollers: RefCell::new(Vec::new()),
        }
    }

    /// The `<body>`: where views are mounted.
    #[must_use]
    pub const fn root(&self) -> NodeId {
        NodeId(1)
    }

    /// Adds a stylesheet; it comes after the browser's own and after the ones added before.
    pub fn add_stylesheet(&self, css: &str) {
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.dirty = true;
            dom.ui.stale = true;
        }
        self.dom.borrow_mut().sheets.push(css.to_string());
        *self.cascade.borrow_mut() = None;
    }

    /// The size of the window in CSS pixels.
    pub fn set_viewport(&self, width: f32, height: f32) {
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.dirty = true;
            dom.ui.stale = true;
        }
        let mut dom = self.dom.borrow_mut();
        dom.viewport = (width, height);
        dom.media.width = width / dom.zoom;
        dom.media.height = height / dom.zoom;
        drop(dom);
        *self.cascade.borrow_mut() = None;
    }

    /// How many device pixels a CSS pixel is (a phone's density): lengths in the page are multiplied by it, and
    /// `@media` sees the window in CSS pixels. The viewport and everything a host gives (pointer positions, safe
    /// areas) stay in device pixels.
    pub fn set_scale(&self, scale: f32) {
        let mut dom = self.dom.borrow_mut();
        dom.zoom = scale.max(0.1);
        // Drawings are made at the density: they are drawn again.
        dom.images.retain(|_, v| !v.as_ref().is_some_and(|p| p.svg));
        let (width, height) = dom.viewport;
        dom.media.width = width / dom.zoom;
        dom.media.height = height / dom.zoom;
        dom.ui.dirty = true;
        dom.ui.stale = true;
        drop(dom);
        *self.cascade.borrow_mut() = None;
    }

    /// What `@media` asks about (colour scheme, hover, pointer, reduced motion; the size follows the viewport).
    pub fn set_media(&self, media: MediaEnv) {
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.dirty = true;
            dom.ui.stale = true;
        }
        let mut dom = self.dom.borrow_mut();
        let (width, height) = dom.viewport;
        let zoom = dom.zoom;
        self.platform.set_reduced_motion(media.reduced_motion);
        dom.media = MediaEnv {
            width: width / zoom,
            height: height / zoom,
            ..media
        };
        drop(dom);
        *self.cascade.borrow_mut() = None;
    }

    /// `env(safe-area-inset-*)`: top, right, bottom, left.
    pub fn set_safe_area(&self, insets: [f32; 4]) {
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.dirty = true;
            dom.ui.stale = true;
        }
        self.dom.borrow_mut().safe_area = insets;
        *self.cascade.borrow_mut() = None;
    }

    /// The tag of an element.
    #[must_use]
    pub fn tag(&self, node: NodeId) -> Option<String> {
        self.dom.borrow().tag(node.0).map(String::from)
    }

    /// The value of an attribute.
    #[must_use]
    pub fn attr(&self, node: NodeId, name: &str) -> Option<String> {
        self.dom.borrow().attr(node.0, name).map(String::from)
    }

    #[must_use]
    pub fn children(&self, node: NodeId) -> Vec<NodeId> {
        self.dom.borrow().nodes[node.0]
            .children
            .iter()
            .map(|c| NodeId(*c))
            .collect()
    }

    /// Teaches the renderer how to get the image a `src` names (anything but a `data:` URL, which it reads itself).
    pub fn set_image_loader(&self, loader: impl Fn(&str) -> Option<Vec<u8>> + 'static) {
        let mut dom = self.dom.borrow_mut();
        dom.loader = Some(Rc::new(loader));
        dom.images.retain(|_, v| v.is_some());
        dom.ui.dirty = true;
        dom.ui.stale = true;
    }

    /// Gives the renderer the bytes of the image a `src` names (a PNG), so no loader is needed for it.
    ///
    /// # Errors
    /// The image cannot be read.
    pub fn add_image(&self, src: &str, bytes: &[u8]) -> Result<(), quench_image::Error> {
        let zoom = self.dom.borrow().zoom;
        let picture = crate::pipeline::picture(bytes, zoom)?;
        let mut dom = self.dom.borrow_mut();
        dom.images.insert(src.to_string(), Some(picture));
        dom.ui.dirty = true;
        dom.ui.stale = true;
        Ok(())
    }

    /// What a node is.
    #[must_use]
    pub fn kind(&self, node: NodeId) -> NodeKind {
        match &self.dom.borrow().nodes[node.0].kind {
            Kind::Element { tag, .. } => NodeKind::Element(tag.clone()),
            Kind::Text(t) => NodeKind::Text(t.clone()),
            Kind::Anchor => NodeKind::Anchor,
        }
    }

    /// The attributes of an element, in the order they were set.
    #[must_use]
    pub fn attrs(&self, node: NodeId) -> Vec<(String, String)> {
        match &self.dom.borrow().nodes[node.0].kind {
            Kind::Element { attrs, .. } => attrs.clone(),
            _ => Vec::new(),
        }
    }

    /// The inline style properties set through the style object.
    #[must_use]
    pub fn styles(&self, node: NodeId) -> Vec<(String, String)> {
        match &self.dom.borrow().nodes[node.0].kind {
            Kind::Element { styles, .. } => styles.clone(),
            _ => Vec::new(),
        }
    }

    /// All text under `node`.
    #[must_use]
    pub fn text_of(&self, node: NodeId) -> String {
        fn walk(dom: &Dom, id: usize, out: &mut String) {
            match &dom.nodes[id].kind {
                Kind::Text(t) => out.push_str(t),
                Kind::Element { .. } => dom.nodes[id]
                    .children
                    .iter()
                    .for_each(|c| walk(dom, *c, out)),
                Kind::Anchor => {}
            }
        }
        let mut out = String::new();
        walk(&self.dom.borrow(), node.0, &mut out);
        out
    }
}

impl Renderer for NativeRenderer {
    type Node = NodeId;

    fn create_element(&self, tag: &str) -> NodeId {
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.dirty = true;
            dom.ui.stale = true;
        }
        NodeId(self.dom.borrow_mut().push(Kind::Element {
            tag: tag.to_ascii_lowercase(),
            attrs: Vec::new(),
            handlers: Vec::new(),
            styles: Vec::new(),
        }))
    }

    fn create_text(&self, text: &str) -> NodeId {
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.dirty = true;
            dom.ui.stale = true;
        }
        NodeId(self.dom.borrow_mut().push(Kind::Text(text.to_string())))
    }

    fn create_anchor(&self) -> NodeId {
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.dirty = true;
            dom.ui.stale = true;
        }
        NodeId(self.dom.borrow_mut().push(Kind::Anchor))
    }

    fn set_text(&self, node: &NodeId, text: &str) {
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.dirty = true;
            dom.ui.stale = true;
        }
        if let Kind::Text(t) = &mut self.dom.borrow_mut().nodes[node.0].kind {
            *t = text.to_string();
        }
    }

    fn set_attr(&self, node: &NodeId, name: &str, value: &str) {
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.dirty = true;
            dom.ui.stale = true;
        }
        if let Kind::Element { attrs, .. } = &mut self.dom.borrow_mut().nodes[node.0].kind {
            if let Some(slot) = attrs.iter_mut().find(|(n, _)| n == name) {
                slot.1 = value.to_string();
            } else {
                attrs.push((name.to_string(), value.to_string()));
            }
        }
    }

    fn remove_attr(&self, node: &NodeId, name: &str) {
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.dirty = true;
            dom.ui.stale = true;
        }
        if let Kind::Element { attrs, .. } = &mut self.dom.borrow_mut().nodes[node.0].kind {
            attrs.retain(|(n, _)| n != name);
        }
    }

    fn insert(&self, parent: &NodeId, child: &NodeId, before: Option<&NodeId>) {
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.dirty = true;
            dom.ui.stale = true;
        }
        let mut dom = self.dom.borrow_mut();
        dom.detach(child.0);
        let position = match before {
            Some(b) => dom.nodes[parent.0]
                .children
                .iter()
                .position(|c| *c == b.0)
                .expect("`before` is not a child of `parent`"),
            None => dom.nodes[parent.0].children.len(),
        };
        dom.nodes[parent.0].children.insert(position, child.0);
        dom.nodes[child.0].parent = Some(parent.0);
    }

    fn remove(&self, parent: &NodeId, child: &NodeId) {
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.dirty = true;
            dom.ui.stale = true;
        }
        let mut dom = self.dom.borrow_mut();
        if dom.nodes[child.0].parent == Some(parent.0) {
            dom.detach(child.0);
        }
    }

    fn listen(&self, node: &NodeId, kind: EventKind, handler: Handler) {
        if let Kind::Element { handlers, .. } = &mut self.dom.borrow_mut().nodes[node.0].kind {
            handlers.push((kind, handler));
        }
    }

    fn focus(&self, node: &NodeId) {
        self.focus_node(node.0, false);
    }

    fn trap_focus(&self, node: &NodeId) {
        self.dom.borrow_mut().ui.trap = Some(node.0);
    }

    fn set_style(&self, node: &NodeId, property: &str, value: &str) {
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.dirty = true;
            dom.ui.stale = true;
        }
        if let Kind::Element { styles, .. } = &mut self.dom.borrow_mut().nodes[node.0].kind {
            styles.retain(|(p, _)| p != property);
            if !value.is_empty() {
                styles.push((property.to_string(), value.to_string()));
            }
        }
    }
}
