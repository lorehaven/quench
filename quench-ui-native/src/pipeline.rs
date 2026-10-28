//! Style, layout and paint: from the DOM to pixels.

use std::fmt::Write as _;
use std::rc::Rc;

use quench_css::{
    Cascade, Color as Css, ComputedStyle, Display, Element, Length, State, parse,
    parse_declarations,
};
use quench_layout::{LayoutBox, Node, layout};
use quench_paint::{Decoration, paint};
use quench_raster::Canvas;

use crate::dom::{Dom, Kind, NativeRenderer};

/// The browser's own style sheet: what elements look like before a page styles them.
pub const UA_STYLESHEET: &str = include_str!("ua.css");

/// One element of the DOM, as the selectors see it.
#[derive(Clone)]
struct El<'a> {
    dom: &'a Dom,
    id: usize,
}

impl Element for El<'_> {
    fn tag(&self) -> String {
        self.dom.tag(self.id).unwrap_or_default().to_string()
    }

    fn id(&self) -> Option<String> {
        self.dom.attr(self.id, "id").map(String::from)
    }

    fn has_class(&self, class: &str) -> bool {
        self.dom
            .attr(self.id, "class")
            .is_some_and(|c| c.split_whitespace().any(|w| w == class))
    }

    fn attribute(&self, name: &str) -> Option<String> {
        self.dom.attr(self.id, name).map(String::from)
    }

    fn parent(&self) -> Option<Self> {
        self.dom.nodes[self.id]
            .parent
            .filter(|p| self.dom.tag(*p).is_some())
            .map(|id| Self { dom: self.dom, id })
    }

    fn previous_sibling(&self) -> Option<Self> {
        let parent = self.dom.nodes[self.id].parent?;
        let siblings = &self.dom.nodes[parent].children;
        let at = siblings.iter().position(|c| *c == self.id)?;
        siblings[..at]
            .iter()
            .rev()
            .find(|c| self.dom.tag(**c).is_some())
            .map(|id| Self {
                dom: self.dom,
                id: *id,
            })
    }

    fn state(&self, state: State) -> bool {
        match state {
            State::Disabled => self.dom.attr(self.id, "disabled").is_some(),
            State::Checked => self.dom.attr(self.id, "checked").is_some(),
            State::Hover => within(self.dom, self.dom.ui.hovered, self.id),
            State::Active => within(self.dom, self.dom.ui.active, self.id),
            State::Focus => self.dom.ui.focused == Some(self.id),
            State::FocusVisible => {
                self.dom.ui.focused == Some(self.id) && self.dom.ui.focus_visible
            }
        }
    }
}

/// Whether `ancestor` is `node` or above it.
fn within(dom: &Dom, node: Option<usize>, ancestor: usize) -> bool {
    let mut at = node;
    while let Some(n) = at {
        if n == ancestor {
            return true;
        }
        at = dom.nodes[n].parent;
    }
    false
}

const PLACEHOLDER: Css = Css::rgba(117, 117, 117, 255);

/// The text an input shows: its value, or its placeholder in grey (`true` when it is the placeholder).
fn input_text(dom: &Dom, id: usize) -> (String, bool) {
    let kind = dom.attr(id, "type").unwrap_or("text").to_ascii_lowercase();
    let composed = crate::input::displayed_value(dom, id);
    let value = if dom.ui.focused == Some(id) {
        composed.as_str()
    } else {
        dom.attr(id, "value").unwrap_or("")
    };
    let shown = match kind.as_str() {
        "password" => "\u{2022}".repeat(value.chars().count()),
        "date" => match value.split('-').collect::<Vec<_>>().as_slice() {
            [y, m, d] if !value.is_empty() => format!("{m}/{d}/{y}"),
            _ => String::new(),
        },
        "time" => match value.split_once(':') {
            Some((h, m)) if !value.is_empty() => {
                let hour: u32 = h.parse().unwrap_or(0);
                format!(
                    "{:02}:{m} {}",
                    if hour.is_multiple_of(12) {
                        12
                    } else {
                        hour % 12
                    },
                    if hour < 12 { "AM" } else { "PM" }
                )
            }
            _ => String::new(),
        },
        _ => value.to_string(),
    };
    if !shown.is_empty() {
        return (shown, false);
    }
    let placeholder = match kind.as_str() {
        "date" => "mm/dd/yyyy".to_string(),
        "time" => "--:-- --".to_string(),
        _ => dom.attr(id, "placeholder").unwrap_or("").to_string(),
    };
    (placeholder, true)
}

fn is_text_input(kind: &str) -> bool {
    !matches!(
        kind,
        "checkbox" | "radio" | "range" | "color" | "file" | "hidden" | "image"
    )
}

fn inline_style(dom: &Dom, id: usize) -> Vec<quench_css::Declaration> {
    let Kind::Element { styles, .. } = &dom.nodes[id].kind else {
        return Vec::new();
    };
    let mut text = dom.attr(id, "style").unwrap_or("").to_string();
    for (property, value) in styles {
        let _ = write!(text, ";{property}:{value}");
    }
    parse_declarations(&text)
}

fn build(
    dom: &Dom,
    cascade: &Cascade,
    id: usize,
    parent: &Rc<ComputedStyle>,
    root_font: f32,
) -> Option<Rc<Node>> {
    match &dom.nodes[id].kind {
        Kind::Anchor => None,
        Kind::Text(text) => {
            let mut style = parent.inherited();
            style.display = Display::Inline;
            Some(Node::text(id, &Rc::new(style), text))
        }
        Kind::Element { tag, .. } => {
            let mut style = cascade.compute_with(
                &El { dom, id },
                Some(parent),
                root_font,
                &inline_style(dom, id),
            );
            let root_font = if tag == "html" {
                style.font_size
            } else {
                root_font
            };
            let mut kids: Vec<Rc<Node>> = Vec::new();
            let text_child = |style: &Rc<ComputedStyle>, text: &str, color: Option<Css>| {
                let mut inherited = style.inherited();
                inherited.display = Display::Inline;
                if let Some(c) = color {
                    inherited.color = c;
                }
                Node::text(id, &Rc::new(inherited), text)
            };
            let mut keep_children = true;
            match tag.as_str() {
                "br" => {
                    let shared = Rc::new(style.clone());
                    kids.push(text_child(&shared, "\u{2028}", None));
                    keep_children = false;
                }
                "input" => {
                    keep_children = false;
                    let kind = dom.attr(id, "type").unwrap_or("text").to_ascii_lowercase();
                    if is_text_input(&kind) {
                        let shared = Rc::new(style.clone());
                        let (text, placeholder) = input_text(dom, id);
                        if !text.is_empty() {
                            kids.push(text_child(
                                &shared,
                                &text,
                                (placeholder && !matches!(kind.as_str(), "date" | "time"))
                                    .then_some(PLACEHOLDER),
                            ));
                        }
                        // An empty field is still one line tall.
                        let line = cascade_line_height(dom, &style);
                        let extra = if style.border_box {
                            style.padding[0].resolve(0.0)
                                + style.padding[2].resolve(0.0)
                                + style.border_width[0]
                                + style.border_width[2]
                        } else {
                            0.0
                        };
                        if style.height.is_none() && style.min_height.is_none() {
                            style.min_height = Some(Length::Px(line + extra));
                        }
                    }
                }
                "textarea" => {
                    keep_children = false;
                    let shared = Rc::new(style.clone());
                    let text: String = dom.nodes[id]
                        .children
                        .iter()
                        .filter_map(|c| {
                            if let Kind::Text(t) = &dom.nodes[*c].kind {
                                Some(t.as_str())
                            } else {
                                None
                            }
                        })
                        .collect();
                    let value = dom.attr(id, "value").map_or(text, String::from);
                    if value.is_empty() {
                        if let Some(p) = dom.attr(id, "placeholder") {
                            kids.push(text_child(&shared, p, Some(PLACEHOLDER)));
                        }
                    } else {
                        kids.push(text_child(&shared, &value, None));
                    }
                    if style.height.is_none() && style.min_height.is_none() {
                        let rows: f32 = dom
                            .attr(id, "rows")
                            .and_then(|r| r.parse().ok())
                            .unwrap_or(2.0);
                        style.min_height =
                            Some(Length::Px(cascade_line_height(dom, &style) * rows));
                    }
                }
                _ => {}
            }
            // A button or a text field centres what is in it between its top and its bottom.
            let is_control = tag == "button"
                || (tag == "input"
                    && is_text_input(&dom.attr(id, "type").unwrap_or("text").to_ascii_lowercase()));
            let centre =
                is_control && matches!(style.display, Display::InlineBlock | Display::Block);
            if centre {
                style.display = if style.display == Display::Block {
                    Display::Flex
                } else {
                    Display::InlineFlex
                };
                style.flex_direction = quench_css::FlexDirection::Column;
                style.justify_content = quench_css::Justify::Center;
            }
            let style = Rc::new(style);
            if keep_children {
                for child in &dom.nodes[id].children {
                    if let Some(node) = build(dom, cascade, *child, &style, root_font) {
                        kids.push(node);
                    }
                }
            }
            if centre && !kids.is_empty() {
                // Everything in it is one block, so inline content flows together as it does in a button.
                let mut inner = style.inherited();
                inner.display = Display::Block;
                kids = vec![Rc::new(Node {
                    key: usize::MAX,
                    style: Rc::new(inner),
                    content: quench_layout::Content::Children(kids),
                })];
            }
            Some(Rc::new(Node {
                key: id,
                style,
                content: quench_layout::Content::Children(kids),
            }))
        }
    }
}

fn cascade_line_height(dom: &Dom, style: &ComputedStyle) -> f32 {
    dom.fonts.line_height(style)
}

impl NativeRenderer {
    fn cascade(&self) -> Rc<Cascade> {
        if let Some(c) = self.cascade.borrow().as_ref() {
            return Rc::clone(c);
        }
        let dom = self.dom.borrow();
        let mut cascade = Cascade::new(dom.media);
        cascade.safe_area = dom.safe_area;
        cascade.zoom = dom.zoom;
        cascade.add(&parse(UA_STYLESHEET));
        for sheet in &dom.sheets {
            cascade.add(&parse(sheet));
        }
        let cascade = Rc::new(cascade);
        *self.cascade.borrow_mut() = Some(Rc::clone(&cascade));
        cascade
    }

    /// Styles and lays out the document in the current viewport. The layout is kept: it is what pointer events
    /// are matched against.
    #[must_use]
    pub fn layout(&self) -> Rc<LayoutBox> {
        let cascade = self.cascade();
        let boxes = {
            let dom = self.dom.borrow();
            let root_style = Rc::new(ComputedStyle {
                font_size: 16.0 * dom.zoom,
                ..ComputedStyle::default()
            });
            let tree = build(&dom, &cascade, 0, &root_style, 16.0 * dom.zoom)
                .expect("the document has a root");
            Rc::new(layout(&tree, dom.viewport, &dom.fonts))
        };
        {
            let mut dom = self.dom.borrow_mut();
            dom.layout = Some(Rc::clone(&boxes));
            dom.ui.stale = false;
        }
        boxes
    }

    /// Draws the document into `canvas` (laid out for the viewport that was set, not for the canvas).
    pub fn render<S: AsRef<[u8]> + AsMut<[u8]>>(&self, canvas: &mut Canvas<S>) -> Rc<LayoutBox> {
        let boxes = self.layout();
        // The page background is the root's, or else the body's, and it fills the whole canvas.
        let page = [Some(&*boxes), boxes.children.first()]
            .into_iter()
            .flatten()
            .map(|b| b.style.resolve(b.style.background))
            .find(|c| c.a > 0)
            .map_or(quench_raster::Color::WHITE, quench_paint::to_raster);
        canvas.clear(page);
        let dom = self.dom.borrow();
        let decorations = |key: usize| -> Vec<Decoration> {
            let mut out = Vec::new();
            if dom.tag(key) == Some("input") && dom.attr(key, "checked").is_some() {
                match dom.attr(key, "type") {
                    Some("checkbox") => out.push(Decoration::Check(Css::rgba(255, 255, 255, 255))),
                    Some("radio") => out.push(Decoration::Dot(Css::rgba(255, 255, 255, 255))),
                    _ => {}
                }
            }
            if dom.ui.focused == Some(key) && crate::input::is_text_field(&dom, key) {
                out.extend(field_decorations(&dom, &boxes, key));
            }
            out
        };
        let scroll = |key: usize| dom.ui.scroll.get(&key).copied().unwrap_or((0.0, 0.0));
        paint(canvas, &boxes, &dom.fonts, &scroll, &decorations);
        boxes
    }
}

/// The selection and the caret of the focused text field, from the top left of its box.
fn field_decorations(dom: &Dom, boxes: &LayoutBox, f: usize) -> Vec<Decoration> {
    let Some(field) = boxes.find(f) else {
        return Vec::new();
    };
    let Some((bx, by)) = boxes.position_of(f) else {
        return Vec::new();
    };
    let value: Vec<char> = crate::input::displayed_value(dom, f).chars().collect();
    let plain = crate::input::field_value(dom, f).chars().count();
    let (mut caret, mut anchor) = (dom.ui.caret.min(plain), dom.ui.anchor.min(plain));
    // While an input method composes, its text stands where the selection was and the cursor is in it.
    let composing = dom
        .ui
        .preedit
        .as_ref()
        .filter(|(t, _)| !t.is_empty())
        .map(|(t, c)| (caret.min(anchor), t.chars().count(), *c));
    if let Some((at, _, cursor)) = composing {
        (caret, anchor) = (at + cursor, at + cursor);
    }
    let (lo, hi) = (caret.min(anchor), caret.max(anchor));
    let frags = boxes.text_fragments(f);
    let color = field.style.color;
    let mut out = Vec::new();
    if frags.is_empty() {
        let (cx, cy, _, _) = field.content_rect();
        let m = dom.fonts.metrics(&field.style);
        if dom.ui.caret_on {
            out.push(Decoration::Bar {
                x: cx,
                y: cy,
                width: 1.0,
                height: m.ascent + m.descent,
                color,
            });
        }
        return out;
    }
    let mut at = 0;
    let mut caret_drawn = false;
    for (fx, baseline, frag) in &frags {
        let shown: Vec<char> = frag.text.chars().collect();
        let rest: String = value[at.min(value.len())..].iter().collect();
        let start = rest
            .find(frag.text.as_str())
            .map_or(at, |b| at + rest[..b].chars().count());
        let end = start + shown.len();
        let m = dom.fonts.metrics(&frag.style);
        let top = baseline - m.ascent - by;
        let x_of = |i: usize| {
            let prefix: String = shown[..i.min(shown.len())].iter().collect();
            fx - bx + crate::input::prefix_width(dom, &frag.style, &prefix)
        };
        if let Some((at_c, len, _)) = composing
            && at_c < end
            && at_c + len > start
        {
            let (a, b) = (at_c.max(start) - start, (at_c + len).min(end) - start);
            let (x0, x1) = (x_of(a), x_of(b));
            out.push(Decoration::Bar {
                x: x0,
                y: top + m.ascent + 1.0,
                width: (x1 - x0).max(1.0),
                height: 1.0,
                color,
            });
        }
        if lo < hi && lo < end && hi > start {
            let (a, b) = (lo.max(start) - start, hi.min(end) - start);
            let (x0, x1) = (x_of(a), x_of(b));
            out.push(Decoration::Bar {
                x: x0,
                y: top,
                width: (x1 - x0).max(1.0),
                height: m.ascent + m.descent,
                color: Css::rgba(70, 130, 255, 110),
            });
        }
        if !caret_drawn && dom.ui.caret_on && caret >= start && caret <= end {
            out.push(Decoration::Bar {
                x: x_of(caret - start),
                y: top,
                width: 1.0,
                height: m.ascent + m.descent,
                color,
            });
            caret_drawn = true;
        }
        at = end;
    }
    out
}

/// Where the caret of text field `f` is in the window: `(x, top, height)`.
pub(crate) fn caret_spot(dom: &Dom, boxes: &LayoutBox, f: usize) -> Option<(f32, f32, f32)> {
    let field = boxes.find(f)?;
    let (bx, by) = boxes.position_of(f)?;
    let frags = boxes.text_fragments(f);
    let value: Vec<char> = crate::input::displayed_value(dom, f).chars().collect();
    let plain = crate::input::field_value(dom, f).chars().count();
    let caret = match &dom.ui.preedit {
        Some((text, cursor)) if !text.is_empty() => {
            dom.ui.caret.min(dom.ui.anchor).min(plain) + cursor
        }
        _ => dom.ui.caret.min(plain),
    };
    if frags.is_empty() {
        let (cx, cy, _, _) = field.content_rect();
        let m = dom.fonts.metrics(&field.style);
        return Some((bx + cx, by + cy, m.ascent + m.descent));
    }
    let mut at = 0;
    let mut last = None;
    for (fx, baseline, frag) in &frags {
        let shown: Vec<char> = frag.text.chars().collect();
        let rest: String = value[at.min(value.len())..].iter().collect();
        let start = rest
            .find(frag.text.as_str())
            .map_or(at, |b| at + rest[..b].chars().count());
        let end = start + shown.len();
        let m = dom.fonts.metrics(&frag.style);
        let index = caret.saturating_sub(start).min(shown.len());
        let prefix: String = shown[..index].iter().collect();
        last = Some((
            fx + crate::input::prefix_width(dom, &frag.style, &prefix),
            baseline - m.ascent,
            m.ascent + m.descent,
        ));
        if caret <= end {
            break;
        }
        at = end;
    }
    last
}
