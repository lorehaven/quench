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
            dom.anim
                .borrow_mut()
                .apply(id, &mut style, cascade, parent, root_font);
            let root_font = if tag == "html" {
                style.font_size
            } else {
                root_font
            };
            let mut kids: Vec<Rc<Node>> = Vec::new();
            let mut replaced: Option<(f32, f32)> = None;
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
                "img" => {
                    keep_children = false;
                    let found = dom
                        .attr(id, "src")
                        .and_then(|src| dom.images.get(src))
                        .and_then(Clone::clone);
                    let number = |name: &str| {
                        dom.attr(id, name)
                            .and_then(|v| v.parse::<f32>().ok())
                            .filter(|v| *v > 0.0)
                    };
                    if let Some(image) = found {
                        // Its own size in CSS pixels unless the page gave one (and the other side follows).
                        let (nw, nh) = (
                            image.width as f32 / image.scale,
                            image.height as f32 / image.scale,
                        );
                        let (w, h) = match (number("width"), number("height")) {
                            (Some(w), Some(h)) => (w, h),
                            (Some(w), None) => (w, w * nh / nw),
                            (None, Some(h)) => (h * nw / nh, h),
                            (None, None) => (nw, nh),
                        };
                        replaced = Some((w * dom.zoom, h * dom.zoom));
                        if matches!(style.display, Display::Inline) {
                            style.display = Display::InlineBlock;
                        }
                    } else if let Some(alt) = dom.attr(id, "alt").filter(|a| !a.is_empty()) {
                        // A picture that is not there is its description.
                        let shared = Rc::new(style.clone());
                        kids.push(text_child(&shared, alt, None));
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
            if let Some((w, h)) = replaced {
                return Some(Rc::new(Node {
                    key: id,
                    style,
                    content: quench_layout::Content::Replaced(w, h),
                }));
            }
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

/// The boxes that have a `background-image`, by key.
fn collect_backgrounds(b: &LayoutBox, out: &mut Vec<(usize, String)>) {
    if let Some(url) = &b.style.background_image {
        out.push((b.key, url.clone()));
    }
    for child in &b.children {
        collect_backgrounds(child, out);
    }
}

fn box_of(b: &LayoutBox, key: usize) -> Option<&LayoutBox> {
    if b.key == key {
        return Some(b);
    }
    b.children.iter().find_map(|c| box_of(c, key))
}

/// The bytes in a `data:` URL (after the scheme): base64, or text with %-escapes.
fn data_url(rest: &str) -> Option<Vec<u8>> {
    let (head, data) = rest.split_once(',')?;
    if head.ends_with(";base64") {
        return quench_core::base64::STANDARD.decode(data).ok();
    }
    let raw = data.as_bytes();
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        match raw[i] {
            b'%' if i + 2 < raw.len() && hex(raw[i + 1]).is_some() && hex(raw[i + 2]).is_some() => {
                out.push((hex(raw[i + 1])? * 16 + hex(raw[i + 2])?) as u8);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    Some(out)
}

/// A picture from a file's bytes: a bitmap as it is, a drawing made at `zoom` device pixels per CSS pixel.
pub(crate) fn picture(bytes: &[u8], zoom: f32) -> Result<crate::dom::Picture, quench_image::Error> {
    match quench_image::decode(bytes) {
        Ok(image) => Ok(crate::dom::Picture::from(image)),
        Err(quench_image::Error::Unsupported("SVG" | "unknown")) => {
            let drawn = std::str::from_utf8(bytes)
                .ok()
                .and_then(|text| quench_svg::render(text, zoom).ok())
                .ok_or(quench_image::Error::Corrupt("not a readable drawing"))?;
            Ok(crate::dom::Picture {
                scale: zoom,
                svg: true,
                ..crate::dom::Picture::from(drawn)
            })
        }
        Err(e) => Err(e),
    }
}

/// How a progress bar, a meter or a slider shows its value: bars from the border box's corner.
fn gauge_decorations(dom: &Dom, key: usize, b: &LayoutBox) -> Vec<Decoration> {
    let slider = crate::input::is_range(dom, key);
    let role = if slider {
        crate::access::Role::Slider
    } else {
        crate::access::Role::ProgressBar
    };
    let (min, max, now) = crate::access::range_of(dom, key, role);
    let Some(now) = now else {
        return Vec::new();
    };
    let frac = if max > min {
        ((now - min) / (max - min)) as f32
    } else {
        0.0
    };
    let accent = Css::rgba(0, 117, 255, 255);
    let z = dom.zoom;
    if slider {
        let track = 4.0 * z;
        let thumb = 10.0 * z;
        let y = (b.height - track) / 2.0;
        return vec![
            Decoration::Bar {
                x: 0.0,
                y,
                width: b.width,
                height: track,
                color: Css::rgba(196, 196, 196, 255),
            },
            Decoration::Bar {
                x: 0.0,
                y,
                width: b.width * frac,
                height: track,
                color: accent,
            },
            Decoration::Bar {
                x: (b.width - thumb) * frac,
                y: 2.0 * z,
                width: thumb,
                height: (b.height - 4.0 * z).max(0.0),
                color: accent,
            },
        ];
    }
    let edge = b.border[0];
    vec![Decoration::Bar {
        x: edge,
        y: edge,
        width: (b.width - 2.0 * edge).max(0.0) * frac,
        height: (b.height - 2.0 * edge).max(0.0),
        color: accent,
    }]
}

fn cascade_line_height(dom: &Dom, style: &ComputedStyle) -> f32 {
    dom.fonts.line_height(style)
}

impl NativeRenderer {
    /// Loads and decodes the images the document asks for that are not known yet (a failure is remembered).
    fn resolve_images(&self) {
        let wanted: Vec<String> = {
            let dom = self.dom.borrow();
            (0..dom.nodes.len())
                .filter(|n| dom.tag(*n) == Some("img"))
                .filter_map(|n| dom.attr(n, "src").map(String::from))
                .filter(|src| !dom.images.contains_key(src))
                .collect()
        };
        self.load_images(wanted);
    }

    /// Loads and decodes `wanted` (those not known yet).
    fn load_images(&self, wanted: Vec<String>) {
        for src in wanted {
            if self.dom.borrow().images.contains_key(&src) {
                continue;
            }
            let loader = self.dom.borrow().loader.clone();
            let bytes = match src.strip_prefix("data:") {
                Some(rest) => data_url(rest),
                None => loader.and_then(|load| load(&src)),
            };
            let zoom = self.dom.borrow().zoom;
            let image = bytes.and_then(|b| picture(&b, zoom).ok());
            self.dom.borrow_mut().images.insert(src, image);
        }
    }

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
        self.resolve_images();
        let cascade = self.cascade();
        let reduced = quench_ui::platform::Platform::prefers_reduced_motion(&*self.platform);
        let boxes = {
            let dom = self.dom.borrow();
            let root_style = Rc::new(ComputedStyle {
                font_size: 16.0 * dom.zoom,
                ..ComputedStyle::default()
            });
            dom.anim.borrow_mut().begin(reduced);
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
        // The pictures that boxes ask for with `background-image`.
        let mut backgrounds: Vec<(usize, String)> = Vec::new();
        collect_backgrounds(&boxes, &mut backgrounds);
        self.load_images(backgrounds.iter().map(|(_, url)| url.clone()).collect());
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
            if dom.tag(key) == Some("img")
                && let Some(Some(image)) = dom.attr(key, "src").and_then(|src| dom.images.get(src))
            {
                out.push(Decoration::Image {
                    rgba: Rc::clone(&image.rgba),
                    width: image.width,
                    height: image.height,
                });
            }
            if let Some((_, url)) = backgrounds.iter().find(|(k, _)| *k == key)
                && let Some(Some(image)) = dom.images.get(url)
                && let Some(b) = box_of(&boxes, key)
            {
                out.push(Decoration::Background {
                    rgba: Rc::clone(&image.rgba),
                    width: image.width,
                    height: image.height,
                    size: b.style.background_size,
                    repeat: b.style.background_repeat,
                    position: b.style.background_position,
                    scale: dom.zoom / image.scale,
                });
            }
            if let Some(b) = matches!(dom.tag(key), Some("progress" | "meter"))
                .then(|| box_of(&boxes, key))
                .flatten()
                .or_else(|| {
                    crate::input::is_range(&dom, key)
                        .then(|| box_of(&boxes, key))
                        .flatten()
                })
            {
                out.extend(gauge_decorations(&dom, key, b));
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
    let plain = crate::input::field_value(dom, f).chars().count();
    let caret = match &dom.ui.preedit {
        Some((text, cursor)) if !text.is_empty() => {
            dom.ui.caret.min(dom.ui.anchor).min(plain) + cursor
        }
        _ => dom.ui.caret.min(plain),
    };
    spot_at(dom, boxes, f, caret)
}

/// Where the character position `caret` of a field is on the screen: `(x, top, height)`.
pub(crate) fn spot_at(
    dom: &Dom,
    boxes: &LayoutBox,
    f: usize,
    caret: usize,
) -> Option<(f32, f32, f32)> {
    let field = boxes.find(f)?;
    let (bx, by) = boxes.position_of(f)?;
    let frags = boxes.text_fragments(f);
    let value: Vec<char> = crate::input::displayed_value(dom, f).chars().collect();
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
