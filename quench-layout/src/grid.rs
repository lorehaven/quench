//! CSS grid: explicit and implicit tracks, `fr`, `auto` and `minmax()` sizes, `repeat()` with a count or
//! `auto-fill`/`auto-fit`, gaps, items placed by line numbers and spans or flowed into the first free cell, and
//! stretched or aligned inside their areas.
//!
//! Not built: `grid-template-areas`, `grid-auto-flow: column` and `dense`, `justify-items`/`justify-content`
//! (items stretch across their columns and the tracks fill the box), subgrids, and the contribution of items that
//! span several tracks to the sizes of the tracks they cross (a spanning item grows the last track it crosses).

#![allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "line numbers are small counts, kept in range before they are used as indexes"
)]

use std::rc::Rc;

use quench_css::{Align, ComputedStyle, GridLine, Length, Track};

use crate::engine::{
    Constraint, Edges, Engine, Flow, Width, is_hidden, is_out_of_flow, relative_offset,
};
use crate::node::{LayoutBox, Node, Pending};

/// Where an item sits: its first track and how many tracks it covers, in columns and rows.
#[derive(Debug, Clone, Copy)]
struct Place {
    col: usize,
    row: usize,
    col_span: usize,
    row_span: usize,
}

/// One end pair of lines as a first track (when it is fixed) and a span.
fn resolve_lines(lines: (GridLine, GridLine), tracks: usize) -> (Option<usize>, usize) {
    // Line numbers count lines; the track after line `n` is track `n - 1`; negative numbers count from the end.
    let line = |n: i32| -> usize {
        if n > 0 {
            (n - 1) as usize
        } else {
            (tracks as i32 + n + 1).max(0) as usize
        }
    };
    match lines {
        (GridLine::Line(a), GridLine::Line(b)) => {
            let (a, b) = (line(a), line(b));
            (Some(a.min(b)), a.abs_diff(b).max(1))
        }
        (GridLine::Line(a), GridLine::Span(n)) => (Some(line(a)), n as usize),
        (GridLine::Span(n), GridLine::Line(b)) => {
            (Some(line(b).saturating_sub(n as usize)), n as usize)
        }
        (GridLine::Line(a), GridLine::Auto) => (Some(line(a)), 1),
        (GridLine::Auto, GridLine::Line(b)) => (Some(line(b).saturating_sub(1)), 1),
        (GridLine::Span(n), GridLine::Span(_) | GridLine::Auto)
        | (GridLine::Auto, GridLine::Span(n)) => (None, n as usize),
        (GridLine::Auto, GridLine::Auto) => (None, 1),
    }
}

/// Where `grid-template-areas` puts the area `name`: first row, first column, rows and columns it covers.
fn area_of(style: &ComputedStyle, name: &str) -> Option<(usize, usize, usize, usize)> {
    let cells = |pick: &dyn Fn(usize, usize) -> usize| -> Option<(usize, usize)> {
        let found: Vec<usize> = style
            .grid_areas
            .iter()
            .enumerate()
            .flat_map(|(r, row)| {
                row.iter()
                    .enumerate()
                    .filter(|(_, n)| n.as_str() == name)
                    .map(move |(c, _)| (r, c))
            })
            .map(|(r, c)| pick(r, c))
            .collect();
        Some((*found.iter().min()?, *found.iter().max()?))
    };
    let (r0, r1) = cells(&|r, _| r)?;
    let (c0, c1) = cells(&|_, c| c)?;
    Some((r0, c0, r1 - r0 + 1, c1 - c0 + 1))
}

/// The px a track asks for before sharing out the rest: a length, or a `minmax()` minimum.
fn fixed_size(track: &Track, basis: f32) -> Option<f32> {
    match track {
        Track::Length(l) => Some(l.resolve(basis).max(0.0)),
        Track::MinMax(min, max) => match (&**min, &**max) {
            (Track::Length(a), Track::Length(b)) => Some(a.resolve(basis).max(b.resolve(basis))),
            _ => None,
        },
        _ => None,
    }
}

/// The least a track may be: the length in `minmax(<length>, ...)`.
fn minimum_size(track: &Track, basis: f32) -> f32 {
    match track {
        Track::MinMax(min, _) => match &**min {
            Track::Length(l) => l.resolve(basis).max(0.0),
            _ => 0.0,
        },
        Track::Length(l) => l.resolve(basis).max(0.0),
        _ => 0.0,
    }
}

/// The `fr` a track takes, if it is flexible.
fn flex_factor(track: &Track) -> Option<f32> {
    match track {
        Track::Fr(f) => Some(*f),
        Track::MinMax(_, max) => match &**max {
            Track::Fr(f) => Some(*f),
            _ => None,
        },
        _ => None,
    }
}

/// The tracks a template makes in a box `basis` wide, with `repeat(auto-fill, ...)` filled to the box.
fn expand(list: &quench_css::TrackList, basis: Option<f32>, gap: f32) -> Vec<Track> {
    let mut tracks = list.tracks.clone();
    if let Some((pattern, _fit)) = &list.repeat_to_fill {
        let size = |t: &Track| {
            fixed_size(t, basis.unwrap_or(0.0))
                .unwrap_or_else(|| minimum_size(t, basis.unwrap_or(0.0)))
        };
        let used: f32 = tracks.iter().map(&size).sum::<f32>() + gap * tracks.len() as f32;
        let unit: f32 = pattern.iter().map(&size).sum::<f32>() + gap * pattern.len() as f32;
        let times = match basis {
            Some(b) if unit > 0.0 => {
                (((b - used + gap) / unit).floor() as i64).clamp(1, 1000) as usize
            }
            _ => 1,
        };
        for _ in 0..times {
            tracks.extend(pattern.iter().cloned());
        }
    }
    tracks
}

/// Sizes the tracks of one axis. `content` is each track's `(min, max)` content size; `available` the room the
/// tracks have (`None`: as much as they want).
fn size_tracks(
    tracks: &[Track],
    content: &[(f32, f32)],
    available: Option<f32>,
    gap: f32,
    basis: f32,
) -> Vec<f32> {
    let gaps = gap * tracks.len().saturating_sub(1) as f32;
    let mut sizes = vec![0.0_f32; tracks.len()];
    let mut floors = vec![0.0_f32; tracks.len()];
    for (i, track) in tracks.iter().enumerate() {
        floors[i] = minimum_size(track, basis).max(0.0);
        sizes[i] = match (fixed_size(track, basis), flex_factor(track)) {
            (Some(px), _) => px,
            // A flexible track is never smaller than what its content needs at the least.
            (None, Some(_)) => floors[i].max(content[i].0),
            (None, None) => floors[i].max(content[i].1),
        };
    }
    let Some(avail) = available else {
        // No room set: the flexible tracks want their content's widest.
        for (i, track) in tracks.iter().enumerate() {
            if flex_factor(track).is_some() {
                sizes[i] = floors[i].max(content[i].1);
            }
        }
        return sizes;
    };
    let room = (avail - gaps).max(0.0);
    let flexible: Vec<usize> = (0..tracks.len())
        .filter(|i| flex_factor(&tracks[*i]).is_some())
        .collect();
    let others: f32 = (0..tracks.len())
        .filter(|i| !flexible.contains(i))
        .map(|i| sizes[i])
        .sum();
    if flexible.is_empty() {
        let total: f32 = sizes.iter().sum();
        let autos: Vec<usize> = (0..tracks.len())
            .filter(|i| fixed_size(&tracks[*i], basis).is_none())
            .collect();
        if total > room && !autos.is_empty() {
            // Too wide: the content-sized tracks give way, down to their narrowest content.
            let mut over = total - room;
            for &i in &autos {
                let give = (sizes[i] - floors[i].max(content[i].0)).max(0.0).min(over);
                sizes[i] -= give;
                over -= give;
            }
        } else if total < room && !autos.is_empty() {
            // Room to spare: the content-sized tracks share it.
            let extra = (room - total) / autos.len() as f32;
            for &i in &autos {
                sizes[i] += extra;
            }
        }
        return sizes;
    }
    // Flexible tracks share what the others leave, each at least its floor; a track held at its floor leaves the
    // rest to the others.
    let mut fixed_at_floor = vec![false; tracks.len()];
    let mut unit = 0.0;
    for _ in 0..=flexible.len() {
        let taken: f32 = flexible
            .iter()
            .filter(|i| fixed_at_floor[**i])
            .map(|i| sizes[*i])
            .sum();
        let shares: f32 = flexible
            .iter()
            .filter(|i| !fixed_at_floor[**i])
            .map(|i| flex_factor(&tracks[*i]).unwrap_or(0.0))
            .sum();
        if shares <= 0.0 {
            break;
        }
        // Less than one `fr` in all leaves some room unused.
        unit = (room - others - taken).max(0.0) / shares.max(1.0);
        let mut changed = false;
        for &i in &flexible {
            let floor = floors[i].max(content[i].0);
            if !fixed_at_floor[i] && flex_factor(&tracks[i]).unwrap_or(0.0) * unit < floor {
                fixed_at_floor[i] = true;
                sizes[i] = floor;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for &i in &flexible {
        if !fixed_at_floor[i] {
            sizes[i] = flex_factor(&tracks[i]).unwrap_or(0.0) * unit;
        }
    }
    sizes
}

impl Engine<'_> {
    /// Places the items on the grid: their cells, in the order of the items.
    fn grid_place(
        &self,
        style: &ComputedStyle,
        items: &[Rc<Node>],
        columns: usize,
        rows: usize,
    ) -> (Vec<Place>, usize, usize) {
        let mut taken: Vec<Vec<bool>> = Vec::new();
        let mut columns_total = columns.max(1);
        let free = |taken: &Vec<Vec<bool>>, row: usize, col: usize, cs: usize, rs: usize| {
            (row..row + rs).all(|r| {
                (col..col + cs).all(|c| {
                    !taken
                        .get(r)
                        .and_then(|line| line.get(c))
                        .copied()
                        .unwrap_or(false)
                })
            })
        };
        let mark = |taken: &mut Vec<Vec<bool>>, p: &Place| {
            for r in p.row..p.row + p.row_span {
                if taken.len() <= r {
                    taken.resize(r + 1, Vec::new());
                }
                if taken[r].len() < p.col + p.col_span {
                    taken[r].resize(p.col + p.col_span, false);
                }
                taken[r][p.col..p.col + p.col_span].fill(true);
            }
        };
        let mut places = Vec::new();
        let (mut cursor_row, mut cursor_col) = (0, 0);
        // Items with a fixed place first (so flowing items do not take their cells), then the rest in order.
        let wanted: Vec<_> = items
            .iter()
            .map(|n| {
                // A named area is a fixed place; a name the template does not have is placed by flow.
                if let Some((row, col, row_span, col_span)) = n
                    .style
                    .grid_area
                    .as_deref()
                    .and_then(|name| area_of(style, name))
                {
                    return (Some(col), col_span, Some(row), row_span);
                }
                let (col, col_span) = resolve_lines(n.style.grid_column, columns);
                let (row, row_span) = resolve_lines(n.style.grid_row, rows);
                (col, col_span, row, row_span)
            })
            .collect();
        let mut placed: Vec<Option<Place>> = vec![None; items.len()];
        for (i, (col, col_span, row, row_span)) in wanted.iter().enumerate() {
            if let (Some(c), Some(r)) = (col, row) {
                let p = Place {
                    col: *c,
                    row: *r,
                    col_span: *col_span,
                    row_span: *row_span,
                };
                mark(&mut taken, &p);
                columns_total = columns_total.max(c + col_span);
                placed[i] = Some(p);
            }
        }
        for (i, (col, col_span, row, row_span)) in wanted.iter().enumerate() {
            if placed[i].is_some() {
                continue;
            }
            columns_total = columns_total.max(*col_span);
            let place = match (col, row) {
                (Some(c), None) => {
                    let mut r = if *c < cursor_col {
                        cursor_row + 1
                    } else {
                        cursor_row
                    };
                    while !free(&taken, r, *c, *col_span, *row_span) {
                        r += 1;
                    }
                    columns_total = columns_total.max(c + col_span);
                    Place {
                        col: *c,
                        row: r,
                        col_span: *col_span,
                        row_span: *row_span,
                    }
                }
                (None, Some(r)) => {
                    let mut c = 0;
                    while !free(&taken, *r, c, *col_span, *row_span) {
                        c += 1;
                    }
                    columns_total = columns_total.max(c + col_span);
                    Place {
                        col: c,
                        row: *r,
                        col_span: *col_span,
                        row_span: *row_span,
                    }
                }
                _ => {
                    let (mut r, mut c) = (cursor_row, cursor_col);
                    loop {
                        if c + col_span > columns_total {
                            r += 1;
                            c = 0;
                        } else if free(&taken, r, c, *col_span, *row_span) {
                            break;
                        } else {
                            c += 1;
                        }
                    }
                    (cursor_row, cursor_col) = (r, c + col_span);
                    Place {
                        col: c,
                        row: r,
                        col_span: *col_span,
                        row_span: *row_span,
                    }
                }
            };
            mark(&mut taken, &place);
            placed[i] = Some(place);
        }
        places.extend(placed.into_iter().flatten());
        let rows_total = places
            .iter()
            .map(|p| p.row + p.row_span)
            .max()
            .unwrap_or(0)
            .max(rows);
        let columns_total = places
            .iter()
            .map(|p| p.col + p.col_span)
            .max()
            .unwrap_or(0)
            .max(columns);
        let _ = style;
        (places, columns_total, rows_total)
    }

    /// The tracks of a grid box: the explicit ones, then implicit ones of the `grid-auto-*` size.
    fn grid_tracks(style: &ComputedStyle, cw: f32, ch: Option<f32>) -> (Vec<Track>, Vec<Track>) {
        let columns = expand(&style.grid_columns, Some(cw), style.column_gap.resolve(cw));
        let rows = expand(
            &style.grid_rows,
            ch,
            style.row_gap.resolve(ch.unwrap_or(0.0)),
        );
        (columns, rows)
    }

    /// The widest the items of a grid want to be (for a box that is as wide as its content).
    pub(crate) fn grid_intrinsic(&self, node: &Rc<Node>, items: &[Rc<Node>]) -> (f32, f32) {
        let style = &node.style;
        let (columns, rows) = Self::grid_tracks(style, 0.0, None);
        let (places, ncols, _) = self.grid_place(style, items, columns.len(), rows.len());
        let mut tracks = columns;
        tracks.resize(ncols.max(tracks.len()), style.grid_auto_columns.clone());
        let mut content = vec![(0.0_f32, 0.0_f32); tracks.len()];
        for (item, p) in items.iter().zip(&places) {
            let (min, max) = self.intrinsic_outer(item);
            let share = |v: f32| v / p.col_span as f32;
            let last = p.col + p.col_span - 1;
            if p.col_span == 1 {
                content[last].0 = content[last].0.max(min);
                content[last].1 = content[last].1.max(max);
            } else {
                content[last].0 = content[last].0.max(share(min));
                content[last].1 = content[last].1.max(share(max));
            }
        }
        let gap = style.column_gap.as_px().unwrap_or(0.0) * tracks.len().saturating_sub(1) as f32;
        let sum = |pick: fn(&(f32, f32)) -> f32| {
            tracks
                .iter()
                .zip(&content)
                .map(|(t, c)| {
                    fixed_size(t, 0.0).unwrap_or_else(|| minimum_size(t, 0.0).max(pick(c)))
                })
                .sum::<f32>()
                + gap
        };
        (sum(|c| c.0), sum(|c| c.1))
    }

    pub(crate) fn grid_flow(
        &self,
        style: &ComputedStyle,
        children: &[Rc<Node>],
        cw: f32,
        ch: Option<f32>,
    ) -> Flow {
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
        let items = self.flex_items(children);
        if items.is_empty() {
            return flow;
        }
        let (column_gap, row_gap) = (
            style.column_gap.resolve(cw),
            style.row_gap.resolve(ch.unwrap_or(0.0)),
        );
        let (columns, rows) = Self::grid_tracks(style, cw, ch);
        let (area_rows, area_cols) = (
            style.grid_areas.len(),
            style.grid_areas.first().map_or(0, Vec::len),
        );
        let (places, ncols, nrows) = self.grid_place(
            style,
            &items,
            columns.len().max(area_cols),
            rows.len().max(area_rows),
        );
        let mut columns = columns;
        columns.resize(ncols.max(columns.len()), style.grid_auto_columns.clone());
        let mut rows = rows;
        rows.resize(nrows.max(rows.len()), style.grid_auto_rows.clone());

        // Columns: sized by what their items want, then shared out across the width of the box.
        let mut content = vec![(0.0_f32, 0.0_f32); columns.len()];
        let mut spanning: Vec<(usize, usize, f32)> = Vec::new();
        for (item, p) in items.iter().zip(&places) {
            let (min, max) = self.intrinsic_outer(item);
            if p.col_span == 1 {
                content[p.col].0 = content[p.col].0.max(min);
                content[p.col].1 = content[p.col].1.max(max);
            } else {
                spanning.push((p.col, p.col + p.col_span - 1, max));
            }
        }
        // An item that crosses several columns makes the last of them wider when the columns are too narrow.
        let sizes = size_tracks(&columns, &content, Some(cw), column_gap, cw);
        for (from, to, wanted) in spanning {
            let have: f32 = sizes[from..=to].iter().sum::<f32>() + column_gap * (to - from) as f32;
            if wanted > have && columns[to..=to].iter().all(|t| fixed_size(t, cw).is_none()) {
                content[to].1 += wanted - have;
                content[to].0 += wanted - have;
            }
        }
        let col_sizes = size_tracks(&columns, &content, Some(cw), column_gap, cw);

        // Items are laid out at the width of their columns to learn how tall they are.
        let mut laid: Vec<(LayoutBox, [f32; 4])> = Vec::new();
        for (item, p) in items.iter().zip(&places) {
            let e = Edges::of(&item.style, cw);
            let margin = [
                e.margin[0].unwrap_or(0.0),
                e.margin[1].unwrap_or(0.0),
                e.margin[2].unwrap_or(0.0),
                e.margin[3].unwrap_or(0.0),
            ];
            let area: f32 = col_sizes[p.col..p.col + p.col_span].iter().sum::<f32>()
                + column_gap * (p.col_span - 1) as f32;
            let c = Constraint {
                avail_w: area, // `Fill` takes the margins off it
                avail_h: None,
                width: Width::Fill,
                height: None,
                collapse: false,
            };
            laid.push((self.layout_node(item, &c), margin));
        }
        // Rows: sized by the tallest item in them, and by the box's height when it has one.
        let mut row_content = vec![(0.0_f32, 0.0_f32); rows.len()];
        let mut tall: Vec<(usize, usize, f32)> = Vec::new();
        for ((b, margin), p) in laid.iter().zip(&places) {
            let height = b.height + margin[0] + margin[2];
            if p.row_span == 1 {
                row_content[p.row].0 = row_content[p.row].0.max(height);
                row_content[p.row].1 = row_content[p.row].1.max(height);
            } else {
                tall.push((p.row, p.row + p.row_span - 1, height));
            }
        }
        let provisional = size_tracks(&rows, &row_content, None, row_gap, ch.unwrap_or(0.0));
        for (from, to, wanted) in tall {
            let have: f32 =
                provisional[from..=to].iter().sum::<f32>() + row_gap * (to - from) as f32;
            if wanted > have {
                row_content[to].0 += wanted - have;
                row_content[to].1 += wanted - have;
            }
        }
        let row_sizes = size_tracks(&rows, &row_content, ch, row_gap, ch.unwrap_or(0.0));
        let offsets = |sizes: &[f32], gap: f32| -> Vec<f32> {
            let mut at = 0.0;
            sizes
                .iter()
                .map(|s| {
                    let start = at;
                    at += s + gap;
                    start
                })
                .collect()
        };
        let (col_at, row_at) = (
            offsets(&col_sizes, column_gap),
            offsets(&row_sizes, row_gap),
        );

        for ((item, p), (mut b, margin)) in items.iter().zip(&places).zip(laid) {
            let area_h: f32 = row_sizes[p.row..p.row + p.row_span].iter().sum::<f32>()
                + row_gap * (p.row_span - 1) as f32;
            let area_w: f32 = col_sizes[p.col..p.col + p.col_span].iter().sum::<f32>()
                + column_gap * (p.col_span - 1) as f32;
            let align = item.style.align_self.unwrap_or(style.align_items);
            let inner_h = (area_h - margin[0] - margin[2]).max(0.0);
            if align == Align::Stretch
                && item.style.height.is_none()
                && (b.height - inner_h).abs() > 0.01
            {
                let c = Constraint {
                    avail_w: area_w,
                    avail_h: Some(inner_h),
                    width: Width::Fill,
                    height: Some(inner_h),
                    collapse: false,
                };
                b = self.layout_node(item, &c);
            }
            let free = (inner_h - b.height).max(0.0);
            let offset = match align {
                Align::End => free,
                Align::Center => free / 2.0,
                _ => 0.0,
            };
            let (dx, dy) = relative_offset(&item.style, cw, ch);
            b.x = col_at[p.col] + margin[3] + dx;
            b.y = row_at[p.row] + margin[0] + offset + dy;
            flow.children.push(b);
        }
        flow.baseline = flow.children.first().map(|b| b.y + b.baseline);
        flow.height = row_sizes.iter().sum::<f32>() + row_gap * rows.len().saturating_sub(1) as f32;
        flow
    }
}

/// A grid item's `Length` margin is resolved against the grid's width: the same helper the other flows use.
#[allow(dead_code, reason = "documents the basis margins resolve against")]
const fn _margin_basis(_: &Length) {}
