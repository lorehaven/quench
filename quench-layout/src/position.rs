//! The entry point, and positioned boxes: they wait for the size of their containing block.

use std::rc::Rc;

use quench_css::{ComputedStyle, Position};

use crate::engine::{Constraint, Edges, Engine, Width};
use crate::fonts::Fonts;
use crate::node::{LayoutBox, Node};

/// Lays out `root` in a viewport of `viewport` pixels; `root`'s box is at the origin.
#[must_use]
pub fn layout(root: &Rc<Node>, viewport: (f32, f32), fonts: &Fonts) -> LayoutBox {
    let engine = Engine { fonts };
    let c = Constraint {
        avail_w: viewport.0,
        avail_h: Some(viewport.1),
        width: Width::Fill,
        height: None,
        collapse: false,
    };
    let mut root_box = engine.layout_node(root, &c);
    root_box.x = root_box.margin[3];
    root_box.y = root_box.margin[0];
    let cb = Rect {
        x: 0.0,
        y: 0.0,
        w: viewport.0,
        h: viewport.1,
    };
    let origin = (root_box.x, root_box.y);
    engine.resolve(&mut root_box, origin, cb, cb);
    root_box
}

/// A containing block in page coordinates.
#[derive(Debug, Clone, Copy)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Engine<'_> {
    /// Lays out the positioned boxes under `b` (at `origin` on the page) against their containing blocks.
    fn resolve(&self, b: &mut LayoutBox, origin: (f32, f32), cb: Rect, viewport: Rect) {
        let positioned = b.style.position != Position::Static;
        let mine = if positioned {
            let (px, py, pw, ph) = b.padding_rect();
            Rect {
                x: origin.0 + px,
                y: origin.1 + py,
                w: pw,
                h: ph,
            }
        } else {
            cb
        };
        for child in &mut b.children {
            let at = (origin.0 + child.x, origin.1 + child.y);
            self.resolve(child, at, mine, viewport);
        }
        for p in std::mem::take(&mut b.pending) {
            let block = if p.node.style.position == Position::Fixed {
                viewport
            } else {
                mine
            };
            let mut child = self.layout_positioned(&p.node, block);
            // Static position, for the sides that are auto.
            let style: &ComputedStyle = &p.node.style;
            let static_x = origin.0 + p.x;
            let static_y = origin.1 + p.y;
            let left = style.inset[3].as_ref().map(|l| l.resolve(block.w));
            let right = style.inset[1].as_ref().map(|l| l.resolve(block.w));
            let top = style.inset[0].as_ref().map(|l| l.resolve(block.h));
            let bottom = style.inset[2].as_ref().map(|l| l.resolve(block.h));
            let page_x = match (left, right) {
                (Some(l), _) => block.x + l + child.margin[3],
                (None, Some(r)) => block.x + block.w - r - child.width - child.margin[1],
                (None, None) => static_x + child.margin[3],
            };
            let page_y = match (top, bottom) {
                (Some(t), _) => block.y + t + child.margin[0],
                (None, Some(bt)) => block.y + block.h - bt - child.height - child.margin[2],
                (None, None) => static_y + child.margin[0],
            };
            child.x = page_x - origin.0;
            child.y = page_y - origin.1;
            self.resolve(&mut child, (page_x, page_y), mine, viewport);
            b.overflow.0 = b.overflow.0.max(child.x + child.width);
            b.overflow.1 = b.overflow.1.max(child.y + child.height);
            b.children.push(child);
        }
    }

    /// An absolutely positioned box, sized by its insets, its size or its content.
    fn layout_positioned(&self, node: &Rc<Node>, block: Rect) -> LayoutBox {
        let style = &node.style;
        let e = Edges::of(style, block.w);
        let side = |i: usize, basis: f32| style.inset[i].as_ref().map(|l| l.resolve(basis));
        let (left, right) = (side(3, block.w), side(1, block.w));
        let (top, bottom) = (side(0, block.h), side(2, block.h));
        let margin_w = e.margin[1].unwrap_or(0.0) + e.margin[3].unwrap_or(0.0);
        let margin_h = e.margin[0].unwrap_or(0.0) + e.margin[2].unwrap_or(0.0);
        let width = match (style.width.as_ref(), left, right) {
            (None, Some(l), Some(r)) => Width::Exact((block.w - l - r - margin_w).max(0.0)),
            _ => Width::Shrink,
        };
        let height = match (style.height.as_ref(), top, bottom) {
            (None, Some(t), Some(bt)) => Some((block.h - t - bt - margin_h).max(0.0)),
            _ => None,
        };
        let c = Constraint {
            avail_w: block.w,
            avail_h: Some(block.h),
            width,
            height,
            collapse: false,
        };
        self.layout_node(node, &c)
    }
}
