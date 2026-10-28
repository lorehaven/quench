//! Intrinsic widths: the narrowest a box can be without overflowing (min-content) and the widest it wants to be
//! (max-content). Shrink-to-fit widths and flex bases come from these.

use std::rc::Rc;

use quench_css::{Display, FlexDirection, Overflow};

use crate::engine::{Edges, Engine, Group, groups};
use crate::node::{Content, Node};

impl Engine<'_> {
    /// `(min, max)` of the content box of `node`, without padding and borders.
    pub(crate) fn intrinsic_content(&self, node: &Rc<Node>) -> (f32, f32) {
        match &node.content {
            Content::Replaced(w, _) => (*w, *w),
            Content::Text(_) => self.inline_intrinsic(std::slice::from_ref(node)),
            Content::Children(children)
                if matches!(node.style.display, Display::Flex | Display::InlineFlex) =>
            {
                let items = self.flex_items(children);
                let sizes: Vec<(f32, f32)> =
                    items.iter().map(|i| self.intrinsic_outer(i)).collect();
                let style = &node.style;
                let row = matches!(
                    style.flex_direction,
                    FlexDirection::Row | FlexDirection::RowReverse
                );
                let gap =
                    style.column_gap.as_px().unwrap_or(0.0) * sizes.len().saturating_sub(1) as f32;
                if row {
                    let max: f32 = sizes.iter().map(|s| s.1).sum::<f32>() + gap;
                    let min = if style.flex_wrap {
                        sizes.iter().map(|s| s.0).fold(0.0, f32::max)
                    } else {
                        sizes.iter().map(|s| s.0).sum::<f32>() + gap
                    };
                    (min, max)
                } else {
                    (
                        sizes.iter().map(|s| s.0).fold(0.0, f32::max),
                        sizes.iter().map(|s| s.1).fold(0.0, f32::max),
                    )
                }
            }
            Content::Children(children) => {
                let (mut min, mut max) = (0.0_f32, 0.0_f32);
                for group in groups(children) {
                    let (a, b) = match group {
                        Group::Inline(items) => self.inline_intrinsic(&items),
                        Group::Block(n) => self.intrinsic_outer(&n),
                        Group::OutOfFlow(_) => (0.0, 0.0),
                    };
                    min = min.max(a);
                    max = max.max(b);
                }
                (min, max)
            }
        }
    }

    /// `(min, max)` of the margin box of `node`.
    pub(crate) fn intrinsic_outer(&self, node: &Rc<Node>) -> (f32, f32) {
        let style = &node.style;
        let e = Edges::of(style, 0.0);
        let extra = e.horizontal();
        let margin = e.margin[1].unwrap_or(0.0) + e.margin[3].unwrap_or(0.0);
        let to_bb = |v: f32| {
            if style.border_box {
                v.max(extra)
            } else {
                v + extra
            }
        };
        let (mut min, mut max) =
            if let Some(w) = style.width.as_ref().and_then(quench_css::Length::as_px) {
                (to_bb(w), to_bb(w))
            } else {
                let (a, b) = self.intrinsic_content(node);
                (a + extra, b + extra)
            };
        if let Some(limit) = style.max_width.as_ref().and_then(quench_css::Length::as_px) {
            min = min.min(to_bb(limit));
            max = max.min(to_bb(limit));
        }
        if let Some(limit) = style.min_width.as_ref().and_then(quench_css::Length::as_px) {
            min = min.max(to_bb(limit));
            max = max.max(to_bb(limit));
        }
        (min + margin, max + margin)
    }

    /// Whether the overflow of a box clips its content, so it has no automatic minimum size.
    pub(crate) fn clips(node: &Node, row: bool) -> bool {
        let (x, y) = node.style.overflow;
        (if row { x } else { y }) != Overflow::Visible
    }
}
