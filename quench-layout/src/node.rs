//! What goes into layout (a tree of styled nodes) and what comes out (boxes with positions).

use std::rc::Rc;

use quench_css::ComputedStyle;

/// A styled node. `key` is the caller's own name for it (an index into its DOM, say) and comes back on the boxes.
#[derive(Debug)]
pub struct Node {
    pub key: usize,
    pub style: Rc<ComputedStyle>,
    pub content: Content,
}

#[derive(Debug)]
pub enum Content {
    /// Characters; the style is the style of the element they are in.
    Text(String),
    /// A box with a size of its own and nothing inside (a checkbox, an image): `(width, height)` in pixels.
    Replaced(f32, f32),
    Children(Vec<Rc<Node>>),
}

impl Node {
    #[must_use]
    pub fn element(key: usize, style: ComputedStyle, children: Vec<Rc<Self>>) -> Rc<Self> {
        Rc::new(Self {
            key,
            style: Rc::new(style),
            content: Content::Children(children),
        })
    }

    #[must_use]
    pub fn text(key: usize, style: &Rc<ComputedStyle>, text: &str) -> Rc<Self> {
        Rc::new(Self {
            key,
            style: Rc::clone(style),
            content: Content::Text(text.to_string()),
        })
    }

    #[must_use]
    pub fn replaced(key: usize, style: ComputedStyle, width: f32, height: f32) -> Rc<Self> {
        Rc::new(Self {
            key,
            style: Rc::new(style),
            content: Content::Replaced(width, height),
        })
    }

    pub(crate) const fn is_text(&self) -> bool {
        matches!(self.content, Content::Text(_))
    }
}

/// A piece of inline content on a line, positioned from the top left of the box that holds the lines.
#[derive(Debug, Clone)]
pub enum Fragment {
    Text(TextFragment),
    /// The extent of an inline element on one line: where its background and borders go.
    Inline(InlineFragment),
}

#[derive(Debug, Clone)]
pub struct TextFragment {
    /// The key of the text node.
    pub key: usize,
    /// The characters, spaces included, after `text-transform`.
    pub text: String,
    pub x: f32,
    /// The baseline.
    pub baseline: f32,
    pub width: f32,
    pub style: Rc<ComputedStyle>,
}

#[derive(Debug, Clone)]
pub struct InlineFragment {
    pub key: usize,
    pub style: Rc<ComputedStyle>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Whether the element starts / ends here (borders and radii are only at those ends).
    pub first: bool,
    pub last: bool,
}

/// An out-of-flow box waiting for the size of its containing block.
#[derive(Debug, Clone)]
pub struct Pending {
    pub node: Rc<Node>,
    /// Where it would have been, from the top left of the box that holds it.
    pub x: f32,
    pub y: f32,
}

/// A laid out box. `x` and `y` are those of its border box, from the border box of its parent.
#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub key: usize,
    pub style: Rc<ComputedStyle>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Top, right, bottom, left, in pixels.
    pub margin: [f32; 4],
    pub padding: [f32; 4],
    pub border: [f32; 4],
    /// Boxes inside it: blocks, flex items, inline blocks, positioned boxes, in document order.
    pub children: Vec<Self>,
    /// Text and inline elements on lines.
    pub fragments: Vec<Fragment>,
    /// The first baseline, from the top of the border box.
    pub baseline: f32,
    /// How far the contents reach from the border box's top left (for scrolling).
    pub overflow: (f32, f32),
    pub(crate) pending: Vec<Pending>,
}

impl LayoutBox {
    /// The content box: `(x, y, width, height)` from the border box's top left.
    #[must_use]
    pub fn content_rect(&self) -> (f32, f32, f32, f32) {
        let x = self.border[3] + self.padding[3];
        let y = self.border[0] + self.padding[0];
        (
            x,
            y,
            (self.width - x - self.border[1] - self.padding[1]).max(0.0),
            (self.height - y - self.border[2] - self.padding[2]).max(0.0),
        )
    }

    /// The padding box: `(x, y, width, height)` from the border box's top left.
    #[must_use]
    pub fn padding_rect(&self) -> (f32, f32, f32, f32) {
        (
            self.border[3],
            self.border[0],
            (self.width - self.border[1] - self.border[3]).max(0.0),
            (self.height - self.border[0] - self.border[2]).max(0.0),
        )
    }

    /// Every text fragment of the text node `key`, as `(x, baseline, fragment)` from the root's border box
    /// (translations included), in reading order.
    #[must_use]
    pub fn text_fragments(&self, key: usize) -> Vec<(f32, f32, &TextFragment)> {
        let mut out = Vec::new();
        self.collect_text(key, 0.0, 0.0, &mut out);
        out
    }

    fn collect_text<'a>(
        &'a self,
        key: usize,
        x: f32,
        y: f32,
        out: &mut Vec<(f32, f32, &'a TextFragment)>,
    ) {
        let t = &self.style.transform.translate;
        let (ax, ay) = (
            x + self.x + t.0.resolve(self.width),
            y + self.y + t.1.resolve(self.height),
        );
        for f in &self.fragments {
            if let Fragment::Text(text) = f
                && text.key == key
            {
                out.push((ax + text.x, ay + text.baseline, text));
            }
        }
        for c in &self.children {
            c.collect_text(key, ax, ay, out);
        }
    }

    /// The box with this key, searching depth first.
    #[must_use]
    pub fn find(&self, key: usize) -> Option<&Self> {
        if self.key == key {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(key))
    }

    /// The position of the box with this key from the root's border box (its absolute position on the page).
    #[must_use]
    pub fn position_of(&self, key: usize) -> Option<(f32, f32)> {
        if self.key == key {
            return Some((0.0, 0.0));
        }
        self.children
            .iter()
            .find_map(|c| c.position_of(key).map(|(x, y)| (x + c.x, y + c.y)))
    }
}
