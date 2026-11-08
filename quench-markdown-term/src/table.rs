//! Tables sized to their content: each column as wide as its content when that fits, otherwise the widest
//! columns give up cells (never below their longest word) and their cells wrap; when even that does not fit,
//! each row is stacked as `header: value` lines.

use crate::layout::Ctx;
use crate::model::{Line, Role, Style};
use crate::wrap::{Mark, Out, Piece, measure, repeat, spaces, wrap};
use quench_html::markdown::{Alignment, Table};

const BORDER: Style = Style {
    bold: false,
    italic: false,
    underline: false,
    strikethrough: false,
    dim: false,
    code: false,
    role: Role::Rule,
    link: None,
};

fn bold(pieces: &[Piece]) -> Vec<Piece> {
    pieces
        .iter()
        .map(|piece| match piece {
            Piece::Text { text, style, glue } => Piece::Text {
                text: text.clone(),
                style: Style {
                    bold: true,
                    ..*style
                },
                glue: *glue,
            },
            other => other.clone(),
        })
        .collect()
}

/// Column widths for `width`, from each column's natural and least widths; `None` when the least do not fit.
fn widths(natural: &[usize], least: &[usize], width: usize) -> Option<Vec<usize>> {
    let overhead = 3 * natural.len() + 1;
    let available = width.checked_sub(overhead)?;
    if least.iter().sum::<usize>() > available {
        return None;
    }
    let mut widths = natural.to_vec();
    let mut total: usize = widths.iter().sum();
    while total > available {
        let (column, _) = widths
            .iter()
            .enumerate()
            .filter(|&(j, &w)| w > least[j])
            .max_by_key(|&(j, &w)| (w, std::cmp::Reverse(j)))?;
        widths[column] -= 1;
        total -= 1;
    }
    Some(widths)
}

struct Borders {
    horizontal: &'static str,
    vertical: &'static str,
    /// Left, middle and right junctions of the top, the header rule and the bottom.
    top: [&'static str; 3],
    middle: [&'static str; 3],
    bottom: [&'static str; 3],
}

const UNICODE: Borders = Borders {
    horizontal: "─",
    vertical: "│",
    top: ["┌", "┬", "┐"],
    middle: ["├", "┼", "┤"],
    bottom: ["└", "┴", "┘"],
};

const ASCII: Borders = Borders {
    horizontal: "-",
    vertical: "|",
    top: ["+", "+", "+"],
    middle: ["+", "+", "+"],
    bottom: ["+", "+", "+"],
};

impl Borders {
    fn rule(&self, junctions: [&str; 3], widths: &[usize]) -> Line {
        let mut line = Line::default();
        line.push(junctions[0], BORDER);
        for (j, w) in widths.iter().enumerate() {
            if j > 0 {
                line.push(junctions[1], BORDER);
            }
            line.push(&self.horizontal.repeat(w + 2), BORDER);
        }
        line.push(junctions[2], BORDER);
        line
    }

    /// One row: each cell wrapped to its column and aligned, the cells side by side between borders.
    fn row(&self, cells: &[Vec<Piece>], widths: &[usize], alignments: &[Alignment]) -> Out {
        let laid: Vec<Out> = cells
            .iter()
            .zip(widths)
            .map(|(cell, &w)| wrap(cell, w))
            .collect();
        let height = laid.iter().map(|c| c.lines.len()).max().unwrap_or(0).max(1);
        let mut out = Out::default();
        let mut offsets = Vec::new();
        for k in 0..height {
            let mut line = Line::default();
            line.push(self.vertical, BORDER);
            let mut column = 1;
            for (j, cell) in laid.iter().enumerate() {
                let text = cell.lines.get(k).cloned().unwrap_or_default();
                let free = widths[j] - text.width();
                let left = match alignments.get(j) {
                    Some(Alignment::Right) => free,
                    Some(Alignment::Center) => free / 2,
                    _ => 0,
                };
                line.push(" ", Style::default());
                line.extend(&spaces(left));
                line.extend(&text);
                line.extend(&spaces(free - left));
                line.push(" ", Style::default());
                line.push(self.vertical, BORDER);
                offsets.push(((j, k), column + 1 + left));
                column += widths[j] + 3;
            }
            out.lines.push(line);
        }
        for (j, cell) in laid.iter().enumerate() {
            for mark in &cell.marks {
                let shift = offsets
                    .iter()
                    .find(|((c, k), _)| *c == j && *k == mark.line)
                    .map_or(0, |(_, s)| *s);
                out.marks.push(Mark {
                    column: mark.column + shift,
                    ..*mark
                });
            }
        }
        out
    }
}

impl Ctx<'_> {
    pub fn table(&mut self, table: &Table, width: usize) -> Out {
        let header: Vec<Vec<Piece>> = table
            .header
            .iter()
            .map(|cell| bold(&self.pieces(cell, Style::default())))
            .collect();
        if header.is_empty() {
            return Out::default();
        }
        let rows: Vec<Vec<Vec<Piece>>> = table
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| self.pieces(cell, Style::default()))
                    .collect()
            })
            .collect();
        let columns = header.len();
        let (mut natural, mut least) = (vec![1; columns], vec![1; columns]);
        for row in std::iter::once(&header).chain(&rows) {
            for (j, cell) in row.iter().enumerate().take(columns) {
                let (n, l) = measure(cell);
                natural[j] = natural[j].max(n);
                least[j] = least[j].max(l);
            }
        }
        let borders = if self.options.unicode {
            &UNICODE
        } else {
            &ASCII
        };
        let Some(widths) = widths(&natural, &least, width) else {
            return stacked(&header, &rows, width, borders.horizontal);
        };
        let mut out = Out::line(borders.rule(borders.top, &widths));
        out.append(borders.row(&header, &widths, &table.alignments));
        out.append(Out::line(borders.rule(borders.middle, &widths)));
        for row in &rows {
            out.append(borders.row(row, &widths, &table.alignments));
        }
        out.append(Out::line(borders.rule(borders.bottom, &widths)));
        out
    }
}

/// Each row as `header: value` lines, rows apart by a rule.
fn stacked(header: &[Vec<Piece>], rows: &[Vec<Vec<Piece>>], width: usize, rule: &str) -> Out {
    let mut out = Out::default();
    let records: Vec<&[Vec<Piece>]> = if rows.is_empty() {
        vec![&[]]
    } else {
        rows.iter().map(Vec::as_slice).collect()
    };
    for (i, row) in records.into_iter().enumerate() {
        if i > 0 {
            out.append(Out::line(repeat(rule, width, BORDER)));
        }
        for (j, name) in header.iter().enumerate() {
            let value = row.get(j).filter(|v| measure(v).0 > 0);
            // A column without a header shows its value alone.
            let mut pieces = if measure(name).0 == 0 {
                Vec::new()
            } else {
                let mut label = name.clone();
                label.push(Piece::glued(
                    ":",
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                ));
                label
            };
            if let Some(value) = value {
                if !pieces.is_empty() {
                    pieces.push(Piece::text(" ", Style::default()));
                }
                pieces.extend(value.iter().cloned());
            }
            out.append(wrap(&pieces, width));
        }
    }
    out
}
