//! Arrays: `matrix` and its delimited forms, `cases`, `aligned`, `gathered`, `array`, as LaTeX and amsmath set
//! them up with TeX's `\halign`.
//!
//! Cells are in text style (display style in `aligned` and `gathered`, script style in `smallmatrix`), each column
//! as wide as its widest cell, with `\arraycolsep` (5 pt) on both sides of a column boundary, a quad in `cases`,
//! nothing inside an `aligned` column pair. Rows are kept apart by struts (`\arraystretch` 1.2 in `cases`) or, in
//! `aligned` and `gathered`, by the baseline skip opened up by `\jot`. The array is centred on the math axis, and
//! its delimiters grow around it. Lengths are TeX's at 10 pt: 1 pt is a tenth of an em.

use super::Error;
use super::boxes::{Bx, Engine};
use super::layout::{Ctx, St};
use crate::{Align, Array, ArrayKind, Column, Node, Style};

/// Lays out an array with its delimiters.
#[allow(clippy::too_many_lines)] // one pass of `\halign`, columns then rows, reads best in one piece
pub fn array(engine: &Engine<'_>, array: &Array, ctx: Ctx) -> Result<Bx, Error> {
    let kind = array.kind;
    let cell_style = match kind {
        ArrayKind::Aligned | ArrayKind::Gathered => St::of(Style::Display),
        ArrayKind::SmallMatrix => St::of(Style::Script),
        ArrayKind::Matrix | ArrayKind::Cases | ArrayKind::Array => St::of(Style::Text),
    };
    let unit = engine.em
        * if kind == ArrayKind::SmallMatrix {
            engine.scale(cell_style)
        } else {
            1.0
        };
    let cells = array
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(c, cell)| {
                    // After an `&` in an aligned environment, a relation is spaced as if something came first.
                    if kind == ArrayKind::Aligned && c % 2 == 1 {
                        let mut nodes = vec![Node::Group(Vec::new())];
                        nodes.extend(cell.iter().cloned());
                        engine.list(&nodes, ctx.with(cell_style))
                    } else {
                        engine.list(cell, ctx.with(cell_style))
                    }
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let count = cells.iter().map(Vec::len).max().unwrap_or(0);
    let aligns: Vec<Align> = array
        .columns
        .iter()
        .filter_map(|c| match c {
            Column::Align(a) => Some(*a),
            Column::Rule => None,
        })
        .collect();
    let mut rules_at = vec![0_usize; count + 1];
    let mut boundary = 0;
    for column in &array.columns {
        match column {
            Column::Rule => rules_at[boundary.min(count)] += 1,
            Column::Align(_) => boundary += 1,
        }
    }
    let widths: Vec<f32> = (0..count)
        .map(|c| {
            cells
                .iter()
                .filter_map(|row| row.get(c))
                .map(|b| b.width)
                .fold(0.0, f32::max)
        })
        .collect();

    let colsep = 0.5 * unit;
    let rule_width = 0.04 * unit;
    let double_sep = 0.2 * unit;
    let gap = |b: usize| match kind {
        ArrayKind::Matrix | ArrayKind::Array => 2.0 * colsep,
        ArrayKind::Cases | ArrayKind::Gathered => unit,
        ArrayKind::Aligned if b % 2 == 1 => 0.0,
        ArrayKind::Aligned => 2.0 * unit,
        ArrayKind::SmallMatrix => 5.0 / 18.0 * unit,
    };
    // Left to right: each boundary (its width and the x of its rules), then the column after it.
    let mut x = 0.0;
    let mut column_x = Vec::with_capacity(count);
    let mut rule_x = Vec::new();
    for b in 0..=count {
        let n = rules_at[b];
        let edge = b == 0 || b == count;
        if n == 0 {
            x += if edge {
                if kind == ArrayKind::Array {
                    colsep
                } else {
                    0.0
                }
            } else {
                gap(b)
            };
        } else {
            x += if b == 0 { 0.0 } else { colsep };
            for i in 0..n {
                rule_x.push(x);
                x += rule_width + if i + 1 < n { double_sep } else { 0.0 };
            }
            x += if b == count { 0.0 } else { colsep };
        }
        if b < count {
            column_x.push(x);
            x += widths[b];
        }
    }
    let width = x;

    // Top to bottom, y going down from 0: the rules at each row boundary, then the row.
    let struts = !matches!(kind, ArrayKind::Aligned | ArrayKind::Gathered);
    let stretch = if kind == ArrayKind::Cases { 1.2 } else { 1.0 };
    let (strut_height, strut_depth) = (0.84 * unit * stretch, 0.36 * unit * stretch);
    let mut y = 0.0;
    let mut hline_y = Vec::new();
    let mut baselines = Vec::with_capacity(cells.len());
    let mut previous: Option<(f32, f32)> = None; // baseline and depth of the row above
    let mut hlines = |boundary: usize, y: &mut f32| {
        let n = array.hlines.iter().filter(|&&b| b == boundary).count();
        for i in 0..n {
            *y -= rule_width;
            hline_y.push(*y);
            if i + 1 < n {
                *y -= double_sep;
            }
        }
        n > 0
    };
    for (r, row) in cells.iter().enumerate() {
        let ruled = hlines(r, &mut y);
        let mut height = row.iter().map(|b| b.height).fold(0.0, f32::max);
        let mut depth = row.iter().map(|b| b.depth).fold(0.0, f32::max);
        if struts {
            height = height.max(strut_height);
            depth = depth.max(strut_depth);
        }
        let baseline = match previous {
            Some((above, above_depth)) if !struts && !ruled => {
                // The baseline skip with `\jot` opened up, or the line skip when the rows would come closer.
                let skip = 1.5 * unit;
                let distance = if skip - above_depth - height >= 0.3 * unit {
                    skip
                } else {
                    above_depth + height + 0.4 * unit
                };
                above - distance
            }
            _ => y - height,
        };
        baselines.push(baseline);
        y = baseline - depth;
        previous = Some((baseline, depth));
    }
    hlines(cells.len(), &mut y);
    let total = -y;

    let axis = engine.axis(ctx.style);
    let top = axis + total / 2.0;
    let mut out = Bx::default();
    for (r, row) in cells.into_iter().enumerate() {
        for (c, cell) in row.into_iter().enumerate() {
            let free = widths[c] - cell.width;
            let offset = match aligns.get(c).copied().unwrap_or(Align::Center) {
                Align::Left => 0.0,
                Align::Center => free / 2.0,
                Align::Right => free,
            };
            out.add(cell, column_x[c] + offset, baselines[r] + top);
        }
    }
    for y in hline_y {
        out.rule(0.0, y + top, width, rule_width, ctx.colour);
    }
    for x in rule_x {
        out.rule(x, top - total, rule_width, total, ctx.colour);
    }
    out.height = out.height.max(top);
    out.depth = out.depth.max(total - top);
    out.width = width;
    if array.open.is_none() && array.close.is_none() {
        return Ok(out);
    }
    engine.delimited(array.open, out, array.close, ctx)
}
