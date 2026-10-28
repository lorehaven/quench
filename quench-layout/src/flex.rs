//! Flexbox: one or several lines, growing and shrinking, alignment, auto margins and gaps.

use std::rc::Rc;

use quench_css::{Align, ComputedStyle, Display, FlexDirection, Justify};

use crate::engine::{
    Constraint, Edges, Engine, Flow, Width, is_hidden, is_out_of_flow, relative_offset,
};
use crate::node::{Content, LayoutBox, Node, Pending};

struct Item {
    node: Rc<Node>,
    /// Margins on the main and cross axes, `None` for auto: `[main start, main end, cross start, cross end]`.
    margin: [Option<f32>; 4],
    extra_main: f32,
    basis: f32,
    min: f32,
    max: f32,
    hypothetical: f32,
    target: f32,
    frozen: bool,
    laid: Option<LayoutBox>,
    cross: f32,
}

impl Item {
    fn margin_main(&self) -> f32 {
        self.margin[0].unwrap_or(0.0) + self.margin[1].unwrap_or(0.0)
    }

    fn margin_cross(&self) -> f32 {
        self.margin[2].unwrap_or(0.0) + self.margin[3].unwrap_or(0.0)
    }

    fn outer(&self, size: f32) -> f32 {
        size + self.margin_main()
    }
}

impl Engine<'_> {
    /// The items of a flex container: boxes as they are, text wrapped in an anonymous box of its own.
    pub(crate) fn flex_items(&self, children: &[Rc<Node>]) -> Vec<Rc<Node>> {
        let mut items = Vec::new();
        for child in children {
            if is_hidden(child) || is_out_of_flow(child) {
                continue;
            }
            match &child.content {
                Content::Text(t) => {
                    if t.chars().all(char::is_whitespace) {
                        continue;
                    }
                    let mut style: ComputedStyle = (*child.style).clone();
                    style.display = Display::Block;
                    items.push(Rc::new(Node {
                        key: usize::MAX,
                        style: Rc::new(style),
                        content: Content::Children(vec![Rc::clone(child)]),
                    }));
                }
                _ => items.push(Rc::clone(child)),
            }
        }
        items
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the algorithm of the specification, step by step"
    )]
    pub(crate) fn flex_flow(
        &self,
        style: &ComputedStyle,
        children: &[Rc<Node>],
        cw: f32,
        ch: Option<f32>,
        height_limits: (f32, f32),
    ) -> Flow {
        let row = matches!(
            style.flex_direction,
            FlexDirection::Row | FlexDirection::RowReverse
        );
        let reverse = matches!(
            style.flex_direction,
            FlexDirection::RowReverse | FlexDirection::ColumnReverse
        );
        let (main_gap, cross_gap) = {
            let c = style.column_gap.resolve(cw);
            let r = style.row_gap.resolve(ch.unwrap_or(0.0));
            if row { (c, r) } else { (r, c) }
        };
        let inner_main = if row { Some(cw) } else { ch };
        let inner_cross = if row { ch } else { Some(cw) };
        let mut flow = Flow::default();
        for child in children {
            if !is_hidden(child) && is_out_of_flow(child) {
                flow.pending.push(Pending {
                    node: Rc::clone(child),
                    x: 0.0,
                    y: 0.0,
                });
            }
        }

        let mut items: Vec<Item> = self
            .flex_items(children)
            .into_iter()
            .map(|node| self.flex_item(style, node, row, cw, ch, inner_main))
            .collect();
        if items.is_empty() {
            return flow;
        }
        if reverse {
            items.reverse();
        }

        // Lines.
        let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
        if style.flex_wrap
            && let Some(limit) = inner_main
        {
            let mut used = 0.0;
            for (i, item) in items.iter().enumerate() {
                let outer = item.outer(item.hypothetical);
                let line = lines.last_mut().expect("one line");
                if !line.is_empty() && used + main_gap + outer > limit + 0.01 {
                    lines.push(Vec::new());
                    used = 0.0;
                }
                let line = lines.last_mut().expect("one line");
                used += if line.is_empty() {
                    outer
                } else {
                    main_gap + outer
                };
                line.push(i);
            }
        } else {
            lines[0] = (0..items.len()).collect();
        }

        // Flexible lengths, line by line.
        for line in &lines {
            let gaps = main_gap * (line.len() - 1) as f32;
            let hypo_sum: f32 = line
                .iter()
                .map(|&i| items[i].outer(items[i].hypothetical))
                .sum::<f32>()
                + gaps;
            let inner = inner_main.unwrap_or(hypo_sum);
            let growing = hypo_sum < inner;
            let initial_free = inner - hypo_sum;
            for &i in line {
                let it = &mut items[i];
                let flex = if growing {
                    it.node.style.flex_grow
                } else {
                    it.node.style.flex_shrink
                };
                it.frozen = flex == 0.0
                    || (growing && it.basis > it.hypothetical)
                    || (!growing && it.basis < it.hypothetical);
                it.target = it.hypothetical;
            }
            for _ in 0..=line.len() {
                let unfrozen: Vec<usize> =
                    line.iter().copied().filter(|&i| !items[i].frozen).collect();
                if unfrozen.is_empty() {
                    break;
                }
                let taken: f32 = line
                    .iter()
                    .map(|&i| {
                        if items[i].frozen {
                            items[i].outer(items[i].target)
                        } else {
                            items[i].outer(items[i].basis)
                        }
                    })
                    .sum::<f32>()
                    + gaps;
                let mut free = inner - taken;
                let factor = |it: &Item| {
                    if growing {
                        it.node.style.flex_grow
                    } else {
                        it.node.style.flex_shrink * it.basis
                    }
                };
                let total: f32 = unfrozen.iter().map(|&i| factor(&items[i])).sum();
                if growing && total < 1.0 {
                    free = free.min(initial_free * total);
                }
                let mut violation = 0.0;
                for &i in &unfrozen {
                    let it = &mut items[i];
                    let share = if total > 0.0 { factor(it) / total } else { 0.0 };
                    let wanted = it.basis + free * share;
                    let clamped = wanted.clamp(it.min.min(it.max), it.max).max(it.extra_main);
                    violation += clamped - wanted;
                    it.target = clamped;
                }
                for &i in &unfrozen {
                    let it = &mut items[i];
                    let wanted_over = it.target > it.max - 0.001;
                    let wanted_under = it.target < it.min + 0.001;
                    if violation == 0.0
                        || (violation > 0.0 && wanted_under)
                        || (violation < 0.0 && wanted_over)
                    {
                        it.frozen = true;
                    }
                }
                if violation == 0.0 {
                    break;
                }
            }
        }

        // Lay the items out at their main size to learn their cross size.
        for it in &mut items {
            let b = self.flex_layout_item(it, style, row, cw, ch, inner_cross, None);
            it.cross = if row { b.height } else { b.width } + it.margin_cross();
            it.laid = Some(b);
        }

        // Cross size of the lines.
        let mut cross_sizes: Vec<f32> = lines
            .iter()
            .map(|l| l.iter().map(|&i| items[i].cross).fold(0.0, f32::max))
            .collect();
        let cross_gaps = cross_gap * (lines.len() - 1) as f32;
        if let Some(inner) = inner_cross {
            if lines.len() == 1 {
                cross_sizes[0] = inner;
            } else {
                let extra = (inner - cross_sizes.iter().sum::<f32>() - cross_gaps).max(0.0)
                    / lines.len() as f32;
                for s in &mut cross_sizes {
                    *s += extra;
                }
            }
        }
        if inner_cross.is_none() && row && lines.len() == 1 {
            // A single line is as tall as the container's height limits allow.
            let (min, max) = height_limits;
            cross_sizes[0] = cross_sizes[0].clamp(min.min(max), max);
        }
        let cross_total = cross_sizes.iter().sum::<f32>() + cross_gaps;

        // Stretched items are laid out again at the size of their line.
        for (n, line) in lines.iter().enumerate() {
            for &i in line {
                let it = &mut items[i];
                let align = it.node.style.align_self.unwrap_or(style.align_items);
                let auto_cross = if row {
                    it.node.style.height.is_none()
                } else {
                    it.node.style.width.is_none()
                };
                if align == Align::Stretch
                    && auto_cross
                    && it.margin[2].is_some()
                    && it.margin[3].is_some()
                {
                    let size = cross_sizes[n] - it.margin_cross();
                    let b = self.flex_layout_item(it, style, row, cw, ch, inner_cross, Some(size));
                    it.cross = if row { b.height } else { b.width } + it.margin_cross();
                    it.laid = Some(b);
                }
            }
        }

        // Place the lines and the items in them.
        let mut cross_at = 0.0;
        let mut main_total = 0.0_f32;
        for (n, line) in lines.iter().enumerate() {
            let gaps = main_gap * (line.len() - 1) as f32;
            let used: f32 = line
                .iter()
                .map(|&i| items[i].outer(items[i].target))
                .sum::<f32>()
                + gaps;
            let inner = inner_main.unwrap_or_else(|| {
                if row {
                    used
                } else {
                    // A column that is as tall as its height limits make it: its items centre in that.
                    used.clamp(height_limits.0.min(height_limits.1), height_limits.1)
                }
            });
            let mut free = inner - used;
            let autos: usize = line
                .iter()
                .map(|&i| {
                    usize::from(items[i].margin[0].is_none())
                        + usize::from(items[i].margin[1].is_none())
                })
                .sum();
            let auto_share = if autos > 0 && free > 0.0 {
                free / autos as f32
            } else {
                0.0
            };
            if autos > 0 && free > 0.0 {
                free = 0.0;
            }
            let justify = match (style.justify_content, reverse) {
                (Justify::Start, true) => Justify::End,
                (Justify::End, true) => Justify::Start,
                (j, _) => j,
            };
            let count = line.len() as f32;
            let (mut at, between) = match justify {
                Justify::Start => (0.0, 0.0),
                Justify::End => (free, 0.0),
                Justify::Center => (free / 2.0, 0.0),
                Justify::SpaceBetween if line.len() > 1 && free > 0.0 => {
                    (0.0, free / (count - 1.0))
                }
                Justify::SpaceBetween => (0.0, 0.0),
                Justify::SpaceAround if free > 0.0 => (free / count / 2.0, free / count),
                Justify::SpaceEvenly if free > 0.0 => (free / (count + 1.0), free / (count + 1.0)),
                Justify::SpaceAround | Justify::SpaceEvenly => (free / 2.0, 0.0),
            };
            let baseline_above = line
                .iter()
                .filter(|&&i| {
                    items[i].node.style.align_self.unwrap_or(style.align_items) == Align::Baseline
                        && row
                })
                .map(|&i| {
                    items[i].margin[2].unwrap_or(0.0)
                        + items[i].laid.as_ref().map_or(0.0, |b| b.baseline)
                })
                .fold(0.0, f32::max);
            let mut order: Vec<usize> = line.clone();
            if reverse {
                // The line was reversed as a whole; the first item of the line is the last on screen.
                order.reverse();
                order.reverse();
            }
            for &i in &order {
                let it = &mut items[i];
                let mut b = it.laid.take().expect("laid out");
                let start_main = it.margin[0].unwrap_or(auto_share);
                let end_main = it.margin[1].unwrap_or(auto_share);
                let size_main = if row { b.width } else { b.height };
                let size_cross = if row { b.height } else { b.width };
                let free_cross = cross_sizes[n] - size_cross - it.margin_cross();
                let (cross_start, extra) = match (it.margin[2], it.margin[3]) {
                    (None, None) => (free_cross.max(0.0) / 2.0, 0.0),
                    (None, Some(_)) => (free_cross.max(0.0), 0.0),
                    (Some(_), None) => (0.0, free_cross.max(0.0)),
                    (Some(_), Some(_)) => {
                        let align = it.node.style.align_self.unwrap_or(style.align_items);
                        let off = match align {
                            Align::Start | Align::Stretch => 0.0,
                            Align::End => free_cross,
                            Align::Center => free_cross / 2.0,
                            Align::Baseline if row => {
                                baseline_above - it.margin[2].unwrap_or(0.0) - b.baseline
                            }
                            Align::Baseline => 0.0,
                        };
                        (off, 0.0)
                    }
                };
                let _ = extra;
                let cs = it.margin[2].unwrap_or(0.0);
                let main_pos = at + start_main;
                let cross_pos = cross_at + cs + cross_start;
                let (dx, dy) = relative_offset(&it.node.style, cw, ch);
                if row {
                    b.x = main_pos + dx;
                    b.y = cross_pos + dy;
                } else {
                    b.x = cross_pos + dx;
                    b.y = main_pos + dy;
                }
                at += start_main + size_main + end_main + main_gap + between;
                flow.children.push(b);
            }
            main_total = main_total.max(used);
            cross_at += cross_sizes[n] + cross_gap;
        }
        if reverse {
            // Items were pushed line by line in reverse; document order is not needed for painting, but the
            // first baseline is the first item's.
        }
        flow.baseline = flow.children.first().map(|b| b.y + b.baseline);
        flow.height = if row {
            cross_total
        } else {
            inner_main.unwrap_or(main_total)
        };
        flow
    }

    fn flex_item(
        &self,
        container: &ComputedStyle,
        node: Rc<Node>,
        row: bool,
        cw: f32,
        ch: Option<f32>,
        inner_main: Option<f32>,
    ) -> Item {
        let style = Rc::clone(&node.style);
        let e = Edges::of(&style, cw);
        let (extra_main, margin) = if row {
            (
                e.horizontal(),
                [e.margin[3], e.margin[1], e.margin[0], e.margin[2]],
            )
        } else {
            (
                e.vertical(),
                [e.margin[0], e.margin[2], e.margin[3], e.margin[1]],
            )
        };
        let to_bb = |v: f32| (if style.border_box { v } else { v + extra_main }).max(extra_main);
        let main_prop = if row {
            style.width.as_ref()
        } else {
            style.height.as_ref()
        };
        let specified = main_prop
            .filter(|l| l.as_px().is_some() || inner_main.is_some())
            .map(|l| to_bb(l.resolve(inner_main.unwrap_or(0.0))));
        let content_size = |this: &Self| -> f32 {
            if row {
                this.intrinsic_content(&node).1 + extra_main
            } else {
                let cross = if style.align_self.unwrap_or(container.align_items) == Align::Stretch {
                    Width::Fill
                } else {
                    Width::Shrink
                };
                let c = Constraint {
                    avail_w: cw,
                    avail_h: ch,
                    width: cross,
                    height: None,
                    collapse: false,
                };
                this.layout_node(&node, &c).height
            }
        };
        let basis = match style.flex_basis.as_ref() {
            Some(l) if l.as_px().is_some() || inner_main.is_some() => {
                to_bb(l.resolve(inner_main.unwrap_or(0.0)))
            }
            _ => specified.unwrap_or_else(|| content_size(self)),
        };
        let min_prop = if row {
            style.min_width.as_ref()
        } else {
            style.min_height.as_ref()
        };
        let max_prop = if row {
            style.max_width.as_ref()
        } else {
            style.max_height.as_ref()
        };
        let min = match min_prop {
            Some(l) => to_bb(l.resolve(inner_main.unwrap_or(0.0))),
            None if Self::clips(&node, row) => 0.0,
            None => {
                let content_min = if row {
                    self.intrinsic_content(&node).0 + extra_main
                } else {
                    content_size(self)
                };
                specified.map_or(content_min, |s| content_min.min(s))
            }
        };
        let max = max_prop
            .filter(|l| l.as_px().is_some() || inner_main.is_some())
            .map_or(f32::INFINITY, |l| {
                to_bb(l.resolve(inner_main.unwrap_or(0.0)))
            });
        let hypothetical = basis.clamp(min.min(max), max);
        Item {
            node,
            margin,
            extra_main,
            basis,
            min,
            max,
            hypothetical,
            target: hypothetical,
            frozen: false,
            laid: None,
            cross: 0.0,
        }
    }

    /// Lays an item out at its main size; `cross` forces the border box cross size.
    #[allow(
        clippy::too_many_arguments,
        reason = "the container's measurements travel together"
    )]
    fn flex_layout_item(
        &self,
        item: &Item,
        container: &ComputedStyle,
        row: bool,
        cw: f32,
        ch: Option<f32>,
        inner_cross: Option<f32>,
        cross: Option<f32>,
    ) -> LayoutBox {
        let _ = inner_cross;
        let align = item.node.style.align_self.unwrap_or(container.align_items);
        let c = if row {
            Constraint {
                avail_w: cw,
                avail_h: ch,
                width: Width::Exact(item.target),
                height: cross,
                collapse: false,
            }
        } else {
            let width = match cross {
                Some(w) => Width::Exact(w),
                None if align == Align::Stretch => Width::Fill,
                None => Width::Shrink,
            };
            Constraint {
                avail_w: cw,
                avail_h: ch,
                width,
                height: Some(item.target),
                collapse: false,
            }
        };
        self.layout_node(&item.node, &c)
    }
}
