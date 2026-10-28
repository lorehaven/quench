//! Block layout: boxes, widths, heights, margin collapsing, and the dispatch to inline and flex layout.

use std::rc::Rc;

use quench_css::{ComputedStyle, Display, Length, Overflow, Position};

use crate::fonts::Fonts;
use crate::node::{Content, Fragment, LayoutBox, Node, Pending};

pub struct Engine<'a> {
    pub fonts: &'a Fonts,
}

/// How the width of a box is found when its style does not say.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Width {
    /// The space the container offers.
    Fill,
    /// As wide as the content wants, up to the space offered.
    Shrink,
    /// This border box width, decided by whoever lays the box out (a flex item).
    Exact(f32),
}

#[derive(Debug, Clone, Copy)]
pub struct Constraint {
    /// The width of the containing block's content box: what percentages and `Fill` refer to.
    pub avail_w: f32,
    /// The height of the containing block's content box when it is known.
    pub avail_h: Option<f32>,
    pub width: Width,
    /// A border box height decided by whoever lays the box out.
    pub height: Option<f32>,
    /// Whether the box may let its children's margins out (it does not for flex items and inline blocks).
    pub collapse: bool,
}

impl Constraint {
    pub(crate) const fn fill(avail_w: f32, avail_h: Option<f32>) -> Self {
        Self {
            avail_w,
            avail_h,
            width: Width::Fill,
            height: None,
            collapse: true,
        }
    }
}

/// What a block container's children produced, positioned from the top left of its content box.
#[derive(Debug, Default)]
pub struct Flow {
    pub children: Vec<LayoutBox>,
    pub fragments: Vec<Fragment>,
    pub pending: Vec<Pending>,
    pub height: f32,
    pub baseline: Option<f32>,
    /// The margin of the first child, when it collapsed through the top of this box.
    pub top_escape: Option<f32>,
    /// The margin of the last child, when it collapsed through the bottom.
    pub bottom_escape: Option<f32>,
}

pub struct Edges {
    pub margin: [Option<f32>; 4],
    pub padding: [f32; 4],
    pub border: [f32; 4],
}

impl Edges {
    pub(crate) fn of(style: &ComputedStyle, basis: f32) -> Self {
        let basis = if basis.is_finite() { basis } else { 0.0 };
        Self {
            margin: std::array::from_fn(|i| style.margin[i].as_ref().map(|l| l.resolve(basis))),
            padding: std::array::from_fn(|i| style.padding[i].resolve(basis).max(0.0)),
            border: style.border_width,
        }
    }

    pub(crate) fn horizontal(&self) -> f32 {
        self.padding[1] + self.padding[3] + self.border[1] + self.border[3]
    }

    pub(crate) fn vertical(&self) -> f32 {
        self.padding[0] + self.padding[2] + self.border[0] + self.border[2]
    }
}

/// Two adjoining margins collapse into one.
pub fn collapse(a: f32, b: f32) -> f32 {
    if a >= 0.0 && b >= 0.0 {
        a.max(b)
    } else if a <= 0.0 && b <= 0.0 {
        a.min(b)
    } else {
        a + b
    }
}

/// Whether a node takes part in layout as a box or as inline content.
pub fn is_hidden(node: &Node) -> bool {
    !node.is_text() && node.style.display == Display::None
}

pub fn is_out_of_flow(node: &Node) -> bool {
    !node.is_text() && matches!(node.style.position, Position::Absolute | Position::Fixed)
}

pub fn is_inline_level(node: &Node) -> bool {
    node.is_text()
        || (!is_out_of_flow(node)
            && matches!(
                node.style.display,
                Display::Inline | Display::InlineBlock | Display::InlineFlex | Display::InlineGrid
            ))
}

fn only_white_space(items: &[Rc<Node>]) -> bool {
    items
        .iter()
        .all(|n| matches!(&n.content, Content::Text(t) if t.chars().all(char::is_whitespace)))
}

/// Children grouped the way a block container lays them out.
pub enum Group {
    Inline(Vec<Rc<Node>>),
    Block(Rc<Node>),
    OutOfFlow(Rc<Node>),
}

pub fn groups(children: &[Rc<Node>]) -> Vec<Group> {
    let mut out = Vec::new();
    let mut run: Vec<Rc<Node>> = Vec::new();
    let flush = |run: &mut Vec<Rc<Node>>, out: &mut Vec<Group>| {
        if !run.is_empty() && !only_white_space(run) {
            out.push(Group::Inline(std::mem::take(run)));
        }
        run.clear();
    };
    for child in children {
        if is_hidden(child) {
            continue;
        }
        if is_out_of_flow(child) {
            out.push(Group::OutOfFlow(Rc::clone(child)));
        } else if is_inline_level(child) {
            run.push(Rc::clone(child));
        } else {
            flush(&mut run, &mut out);
            out.push(Group::Block(Rc::clone(child)));
        }
    }
    flush(&mut run, &mut out);
    out
}

/// The offset of a `position: relative` box from where it was laid out.
pub fn relative_offset(style: &ComputedStyle, width: f32, height: Option<f32>) -> (f32, f32) {
    if style.position != Position::Relative {
        return (0.0, 0.0);
    }
    let px = |l: &Option<Length>, basis: f32| l.as_ref().map(|l| l.resolve(basis));
    let dx = px(&style.inset[3], width)
        .or_else(|| px(&style.inset[1], width).map(|r| -r))
        .unwrap_or(0.0);
    let dy = px(&style.inset[0], height.unwrap_or(0.0))
        .or_else(|| px(&style.inset[2], height.unwrap_or(0.0)).map(|b| -b))
        .unwrap_or(0.0);
    (dx, dy)
}

impl Engine<'_> {
    /// Lays out one box.
    pub(crate) fn layout_node(&self, node: &Rc<Node>, c: &Constraint) -> LayoutBox {
        let style = &node.style;
        let e = Edges::of(style, c.avail_w);
        let extra_w = e.horizontal();
        let extra_v = e.vertical();
        let to_content_w = |bb: f32| {
            if style.border_box {
                (bb - extra_w).max(0.0)
            } else {
                bb.max(0.0)
            }
        };
        let to_content_h = |bb: f32| {
            if style.border_box {
                (bb - extra_v).max(0.0)
            } else {
                bb.max(0.0)
            }
        };
        let margin_fixed = e.margin[1].unwrap_or(0.0) + e.margin[3].unwrap_or(0.0);

        let replaced = if let Content::Replaced(w, h) = node.content {
            Some((w, h))
        } else {
            None
        };
        let specified_w = style
            .width
            .as_ref()
            .filter(|l| c.avail_w.is_finite() || l.as_px().is_some())
            .map(|l| to_content_w(l.resolve(c.avail_w)));
        let mut content_w = match (c.width, specified_w, replaced) {
            (Width::Exact(bb), _, _) => (bb - extra_w).max(0.0),
            (_, Some(w), _) => w,
            // Only its height given: the width follows the proportions.
            (_, None, Some((w, h)))
                if h > 0.0 && style.height.as_ref().is_some_and(|l| l.as_px().is_some()) =>
            {
                let wanted = style
                    .height
                    .as_ref()
                    .and_then(quench_css::Length::as_px)
                    .map_or(h, |px| {
                        if style.border_box {
                            (px - e.vertical()).max(0.0)
                        } else {
                            px
                        }
                    });
                wanted * w / h
            }
            (_, None, Some((w, _))) => w,
            (Width::Fill, None, None) => (c.avail_w - margin_fixed - extra_w).max(0.0),
            (Width::Shrink, None, None) => {
                let (min, max) = self.intrinsic_content(node);
                let room = (c.avail_w - margin_fixed - extra_w).max(0.0);
                max.min(room.max(min))
            }
        };
        if !matches!(c.width, Width::Exact(_)) {
            if let Some(max) = style
                .max_width
                .as_ref()
                .map(|l| to_content_w(l.resolve(c.avail_w)))
            {
                content_w = content_w.min(max);
            }
            if let Some(min) = style
                .min_width
                .as_ref()
                .map(|l| to_content_w(l.resolve(c.avail_w)))
            {
                content_w = content_w.max(min);
            }
        }

        // Auto margins on a block level box with a width take what is left.
        let mut margin = [
            e.margin[0].unwrap_or(0.0),
            e.margin[1].unwrap_or(0.0),
            e.margin[2].unwrap_or(0.0),
            e.margin[3].unwrap_or(0.0),
        ];
        if !matches!(c.width, Width::Shrink | Width::Exact(_)) {
            let free = c.avail_w - content_w - extra_w - margin_fixed;
            if c.avail_w.is_finite() {
                match (e.margin[3], e.margin[1]) {
                    (None, None) => {
                        margin[3] = (free / 2.0).max(0.0);
                        margin[1] = margin[3];
                    }
                    (None, Some(_)) => margin[3] = free.max(0.0),
                    (Some(_), None) => margin[1] = free.max(0.0),
                    _ => {}
                }
            }
        }

        // The height the box has before its content is known.
        let definite_h = match (c.height, style.height.as_ref(), replaced) {
            (Some(bb), _, _) => Some((bb - extra_v).max(0.0)),
            (None, Some(l), _) if l.as_px().is_some() || c.avail_h.is_some() => {
                Some(to_content_h(l.resolve(c.avail_h.unwrap_or(0.0))))
            }
            // A replaced box with only its width given keeps its proportions.
            (None, None, Some((w, h))) if specified_w.is_some() && w > 0.0 => {
                Some(content_w * h / w)
            }
            (None, _, Some((_, h))) => Some(h),
            _ => None,
        };

        let collapse_ok = c.collapse
            && e.padding[0] == 0.0
            && e.border[0] == 0.0
            && style.overflow == (Overflow::Visible, Overflow::Visible)
            && matches!(style.display, Display::Block | Display::ListItem)
            && !is_out_of_flow(node);
        let collapse_bottom =
            collapse_ok && e.padding[2] == 0.0 && e.border[2] == 0.0 && definite_h.is_none();

        let flow = match &node.content {
            Content::Children(children)
                if matches!(style.display, Display::Flex | Display::InlineFlex) =>
            {
                let limit = |l: &Option<Length>| {
                    l.as_ref()
                        .filter(|l| l.as_px().is_some() || c.avail_h.is_some())
                        .map(|l| to_content_h(l.resolve(c.avail_h.unwrap_or(0.0))))
                };
                let limits = (
                    limit(&style.min_height).unwrap_or(0.0),
                    limit(&style.max_height).unwrap_or(f32::INFINITY),
                );
                self.flex_flow(style, children, content_w, definite_h, limits)
            }
            Content::Children(children)
                if matches!(style.display, Display::Grid | Display::InlineGrid) =>
            {
                self.grid_flow(style, children, content_w, definite_h)
            }
            Content::Children(children) => self.block_flow(
                style,
                children,
                content_w,
                definite_h,
                collapse_ok,
                collapse_bottom,
            ),
            Content::Text(_) => self.block_flow(
                style,
                std::slice::from_ref(node),
                content_w,
                definite_h,
                false,
                false,
            ),
            Content::Replaced(..) => Flow::default(),
        };

        let mut content_h = definite_h.unwrap_or(flow.height);
        if c.height.is_none() {
            if let Some(max) = style
                .max_height
                .as_ref()
                .filter(|l| l.as_px().is_some() || c.avail_h.is_some())
                .map(|l| to_content_h(l.resolve(c.avail_h.unwrap_or(0.0))))
            {
                content_h = content_h.min(max);
            }
            if let Some(min) = style
                .min_height
                .as_ref()
                .filter(|l| l.as_px().is_some() || c.avail_h.is_some())
                .map(|l| to_content_h(l.resolve(c.avail_h.unwrap_or(0.0))))
            {
                content_h = content_h.max(min);
            }
        }
        if let Some(escape) = flow.top_escape {
            margin[0] = collapse(margin[0], escape);
        }
        if let Some(escape) = flow.bottom_escape {
            margin[2] = collapse(margin[2], escape);
        }

        let (ox, oy) = (e.border[3] + e.padding[3], e.border[0] + e.padding[0]);
        let width = content_w + extra_w;
        let height = content_h + extra_v;
        let mut children = flow.children;
        let mut fragments = flow.fragments;
        for child in &mut children {
            child.x += ox;
            child.y += oy;
        }
        shift_fragments(&mut fragments, ox, oy);
        let mut pending = flow.pending;
        for p in &mut pending {
            p.x += ox;
            p.y += oy;
        }
        let mut overflow = (width, height);
        for child in &children {
            // What a box that clips holds does not spill out of it.
            let clips = child.style.overflow != (Overflow::Visible, Overflow::Visible);
            let (reach_x, reach_y) = if clips {
                (child.width, child.height)
            } else {
                (
                    child.overflow.0.max(child.width),
                    child.overflow.1.max(child.height),
                )
            };
            overflow.0 = overflow.0.max(child.x + reach_x);
            overflow.1 = overflow.1.max(child.y + reach_y);
        }
        for f in &fragments {
            match f {
                Fragment::Text(t) => {
                    overflow = (overflow.0.max(t.x + t.width), overflow.1.max(t.baseline));
                }
                Fragment::Inline(i) => {
                    overflow = (
                        overflow.0.max(i.x + i.width),
                        overflow.1.max(i.y + i.height),
                    );
                }
            }
        }
        LayoutBox {
            key: node.key,
            style: Rc::clone(style),
            x: 0.0,
            y: 0.0,
            width,
            height,
            margin,
            padding: e.padding,
            border: e.border,
            children,
            fragments,
            baseline: flow.baseline.map_or(height, |b| b + oy),
            overflow,
            pending,
        }
    }

    /// The children of a block container, stacked.
    pub(crate) fn block_flow(
        &self,
        style: &ComputedStyle,
        children: &[Rc<Node>],
        cw: f32,
        ch: Option<f32>,
        collapse_top: bool,
        collapse_bottom: bool,
    ) -> Flow {
        let mut flow = Flow::default();
        let mut cursor = 0.0_f32;
        let mut pending_margin = 0.0_f32;
        let mut first = true;
        for group in groups(children) {
            match group {
                crate::engine::Group::OutOfFlow(n) => {
                    flow.pending.push(Pending {
                        node: n,
                        x: 0.0,
                        y: cursor + pending_margin,
                    });
                }
                crate::engine::Group::Block(n) => {
                    let mut b = self.layout_node(&n, &Constraint::fill(cw, ch));
                    let y = if first && collapse_top {
                        flow.top_escape = Some(b.margin[0]);
                        0.0
                    } else {
                        cursor + collapse(pending_margin, b.margin[0])
                    };
                    let (dx, dy) = relative_offset(&n.style, cw, ch);
                    b.x = b.margin[3] + dx;
                    b.y = y + dy;
                    cursor = y + b.height;
                    pending_margin = b.margin[2];
                    if flow.baseline.is_none() {
                        flow.baseline = Some(y + b.baseline);
                    }
                    first = false;
                    flow.children.push(b);
                }
                crate::engine::Group::Inline(items) => {
                    let lines = self.inline_flow(style, &items, cw);
                    let y0 = if first && collapse_top {
                        0.0
                    } else {
                        cursor + pending_margin
                    };
                    if flow.baseline.is_none() {
                        flow.baseline = lines.baseline.map(|b| y0 + b);
                    }
                    let mut fragments = lines.fragments;
                    shift_fragments(&mut fragments, 0.0, y0);
                    flow.fragments.extend(fragments);
                    for mut atom in lines.atoms {
                        atom.y += y0;
                        flow.children.push(atom);
                    }
                    for mut p in lines.pending {
                        p.y += y0;
                        flow.pending.push(p);
                    }
                    cursor = y0 + lines.height;
                    pending_margin = 0.0;
                    first = false;
                }
            }
        }
        if collapse_bottom {
            flow.bottom_escape = Some(pending_margin);
            flow.height = cursor;
        } else {
            flow.height = cursor + pending_margin;
        }
        flow
    }
}

pub fn shift_fragments(fragments: &mut [Fragment], dx: f32, dy: f32) {
    for f in fragments {
        match f {
            Fragment::Text(t) => {
                t.x += dx;
                t.baseline += dy;
            }
            Fragment::Inline(i) => {
                i.x += dx;
                i.y += dy;
            }
        }
    }
}
