//! Blocks laid out for a width: paragraphs, headings, code, quotes, lists, rules, HTML, display math,
//! callouts and footnotes. Containers render their children narrower and prefix the lines; a container that
//! would leave its content too little room drops its decoration instead, so no line is ever wider than asked.

use crate::inline::{clean, replace_baseurl, strip_raw, superscript};
use crate::model::{
    CalloutKind, FootnoteSpot, FormulaSpot, HeadingSpot, LinkTarget, Rendered, Role, Style,
};
use crate::slug::Slugger;
use crate::width::{clusters, display_width};
use crate::wrap::{Mark, MarkKind, Out, Piece, hard_lines, repeat, spaces, split_cells, wrap};
use crate::{MathMode, MathText, Options};
use quench_html::markdown::{Block, Document, Footnote, Inline, List};
use std::collections::{HashMap, HashSet};

/// The least room a container leaves its content; below it, the decoration goes.
pub const MIN_INNER: usize = 4;
/// Narrower widths are laid out at this width.
pub const MIN_WIDTH: usize = 4;

/// The renderer's state while laying out one document.
pub struct Ctx<'a> {
    pub options: &'a Options,
    pub math: &'a dyn MathText,
    pub footnotes: &'a [Footnote],
    pub links: Vec<LinkTarget>,
    /// Destinations numbered `[n]`, in order.
    pub numbered: Vec<String>,
    pub footnote_links: HashMap<(usize, bool), usize>,
    headings: Vec<(u8, String, String)>,
    slugger: Slugger,
    formulas: Vec<FormulaSpot>,
    /// Footnotes whose definitions are laid out already.
    emitted: HashSet<usize>,
    depth: usize,
}

impl std::fmt::Debug for Ctx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ctx")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy)]
struct Callout {
    kind: CalloutKind,
    /// `{: .note-title }`: the quote's first paragraph is the label.
    titled: bool,
}

#[derive(Debug)]
enum Item<'b> {
    Block(&'b Block),
    Paragraph(Vec<Inline>),
}

#[derive(Debug)]
struct Entry<'b> {
    item: Item<'b>,
    callout: Option<Callout>,
}

fn split_lines(inlines: &[Inline]) -> Vec<&[Inline]> {
    inlines.split(|i| matches!(i, Inline::SoftBreak)).collect()
}

/// The inside of a kramdown attribute list (`{: .note }`, `{:toc}`) when `line` is only that.
fn attribute_list(line: &[Inline]) -> Option<String> {
    if line.is_empty() || !line.iter().all(|i| matches!(i, Inline::Text(_))) {
        return None;
    }
    let text = Inline::plain_text(line);
    let text = text.trim();
    (text.starts_with("{:") && text.ends_with('}')).then(|| text[2..text.len() - 1].to_owned())
}

fn callout_of(list: &str) -> Option<Callout> {
    list.split_whitespace().find_map(|token| {
        let class = token.strip_prefix('.')?;
        let (class, titled) = class
            .strip_suffix("-title")
            .map_or((class, false), |c| (c, true));
        CalloutKind::from_class(class).map(|kind| Callout { kind, titled })
    })
}

fn is_toc(list: &str) -> bool {
    list.split_whitespace().any(|t| t == "toc")
}

fn only_attribute_lists(block: Option<&Block>) -> Option<Vec<String>> {
    let Some(Block::Paragraph(inlines)) = block else {
        return None;
    };
    split_lines(inlines)
        .into_iter()
        .map(attribute_list)
        .collect()
}

/// A kramdown table of contents placeholder: a list of one item `TOC` followed by `{:toc}`.
fn toc_list(list: &List, next: Option<&Block>) -> bool {
    let [item] = list.items.as_slice() else {
        return false;
    };
    let [Block::Paragraph(inlines)] = item.blocks.as_slice() else {
        return false;
    };
    let lines = split_lines(inlines);
    if Inline::plain_text(lines[0]).trim() != "TOC" {
        return false;
    }
    match lines.as_slice() {
        [_] => only_attribute_lists(next).is_some_and(|lists| lists.iter().any(|l| is_toc(l))),
        [_, second] => attribute_list(second).is_some_and(|l| is_toc(&l)),
        _ => false,
    }
}

fn join_lines(lines: &[&[Inline]]) -> Vec<Inline> {
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push(Inline::SoftBreak);
        }
        out.extend(line.iter().cloned());
    }
    out
}

/// The blocks with kramdown's attribute lists taken out, and the callouts they make.
fn entries(blocks: &[Block]) -> Vec<Entry<'_>> {
    let mut entries = Vec::new();
    let mut pending: Option<Callout> = None;
    for (i, block) in blocks.iter().enumerate() {
        match block {
            Block::Paragraph(inlines) => {
                if let Some(lists) = only_attribute_lists(Some(block)) {
                    pending = lists.iter().find_map(|l| callout_of(l));
                    continue;
                }
                let lines = split_lines(inlines);
                let (mut start, mut end) = (0, lines.len());
                if let Some(list) = attribute_list(lines[0]) {
                    pending = callout_of(&list);
                    start = 1;
                }
                let mut own = None;
                if end > start + 1
                    && let Some(list) = attribute_list(lines[end - 1])
                {
                    own = callout_of(&list);
                    end -= 1;
                }
                let item = if start == 0 && end == lines.len() {
                    Item::Block(block)
                } else {
                    Item::Paragraph(join_lines(&lines[start..end]))
                };
                entries.push(Entry {
                    item,
                    callout: own.or_else(|| pending.take()),
                });
            }
            Block::List(list) if toc_list(list, blocks.get(i + 1)) => pending = None,
            _ => entries.push(Entry {
                item: Item::Block(block),
                callout: pending.take(),
            }),
        }
    }
    entries
}

/// A heading's own anchor, `{#id}` at its end, taken out of its content.
fn header_id(content: &[Inline]) -> (Vec<Inline>, Option<String>) {
    let mut content = content.to_vec();
    if let Some(Inline::Text(text)) = content.last_mut() {
        let trimmed = text.trim_end();
        if trimmed.ends_with('}')
            && let Some(open) = trimmed.rfind("{#").or_else(|| trimmed.rfind("{: #"))
        {
            let inside = &trimmed[open..trimmed.len() - 1];
            let id = inside.trim_start_matches(['{', ':', ' ', '#']);
            let id = id.split_whitespace().next().unwrap_or("").to_owned();
            if !id.is_empty() {
                text.truncate(open);
                let kept = text.trim_end().len();
                text.truncate(kept);
                return (content, Some(id));
            }
        }
    }
    (content, None)
}

/// `line` with tabs expanded to stops of 4 and control characters taken out.
fn expand_tabs(line: &str) -> String {
    let mut out = String::new();
    let mut column = 0;
    let visible: String = line
        .chars()
        .filter(|&c| c == '\t' || !c.is_control())
        .collect();
    for (cluster, cells) in clusters(&visible) {
        if cluster == "\t" {
            let next = (column / 4 + 1) * 4;
            out.extend(std::iter::repeat_n(' ', next - column));
            column = next;
        } else {
            out.push_str(cluster);
            column += cells;
        }
    }
    out
}

fn code_rows(text: &str) -> Vec<String> {
    let text = text.strip_suffix('\n').unwrap_or(text);
    text.split('\n').map(expand_tabs).collect()
}

const fn style(role: Role) -> Style {
    Style {
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        dim: false,
        code: false,
        role,
        link: None,
    }
}

impl<'a> Ctx<'a> {
    pub fn new(options: &'a Options, math: &'a dyn MathText, footnotes: &'a [Footnote]) -> Self {
        Self {
            options,
            math,
            footnotes,
            links: Vec::new(),
            numbered: Vec::new(),
            footnote_links: HashMap::new(),
            headings: Vec::new(),
            slugger: Slugger::new(),
            formulas: Vec::new(),
            emitted: HashSet::new(),
            depth: 0,
        }
    }

    /// Records a formula of `width` × `height` cells; its position comes from its mark.
    pub fn formula(&mut self, tex: &str, display: bool, width: usize, height: usize) -> usize {
        self.formulas.push(FormulaSpot {
            line: 0,
            column: 0,
            width,
            height,
            tex: tex.to_owned(),
            display,
        });
        self.formulas.len() - 1
    }

    const fn unicode(&self) -> bool {
        self.options.unicode
    }

    const fn pick(&self, unicode: &'static str, ascii: &'static str) -> &'static str {
        if self.unicode() { unicode } else { ascii }
    }

    /// Lays out `blocks` one under the other; one blank line between them unless `tight`.
    pub fn blocks(&mut self, blocks: &[Block], width: usize, tight: bool) -> Out {
        let mut out = Out::default();
        for entry in entries(blocks) {
            let rendered = match entry.callout {
                Some(callout) => self.callout(&entry.item, callout, width),
                None => self.item(&entry.item, width),
            };
            if rendered.is_empty() {
                continue;
            }
            let mut referenced = Vec::new();
            if self.inline_footnotes() {
                for mark in &rendered.marks {
                    if let MarkKind::FootnoteRef(number) = mark.kind
                        && !self.emitted.contains(&number)
                        && !referenced.contains(&number)
                    {
                        referenced.push(number);
                    }
                }
            }
            join(&mut out, rendered, tight);
            for number in referenced {
                if self.emitted.insert(number) {
                    let definition = self.footnote_inline(number, width);
                    join(&mut out, definition, tight);
                }
            }
        }
        out
    }

    fn item(&mut self, item: &Item<'_>, width: usize) -> Out {
        match item {
            Item::Block(block) => self.block(block, width),
            Item::Paragraph(inlines) => self.paragraph(inlines, width),
        }
    }

    fn block(&mut self, block: &Block, width: usize) -> Out {
        match block {
            Block::Paragraph(inlines) => self.paragraph(inlines, width),
            Block::Heading { level, content } => self.heading(*level, content, width),
            Block::ThematicBreak => {
                Out::line(repeat(self.pick("─", "-"), width, style(Role::Rule)))
            }
            Block::CodeBlock { info, literal, .. } => {
                let language = clean(info.split_whitespace().next().unwrap_or(""));
                let body = Style {
                    code: true,
                    ..style(Role::Code)
                };
                self.framed(
                    &language,
                    &code_rows(&strip_raw(literal)),
                    width,
                    body,
                    style(Role::Rule),
                )
            }
            Block::Html(html) => {
                let mut out = Out::default();
                let dim = Style {
                    dim: true,
                    ..style(Role::Html)
                };
                for row in code_rows(html.trim_end_matches('\n')) {
                    out.append(hard_lines(&row, width, dim));
                }
                out
            }
            Block::BlockQuote(children) => self.quote(children, width),
            Block::List(list) => self.list(list, width),
            Block::Table(table) => self.table(table, width),
            Block::Math(tex) => self.display_math(tex, width),
        }
    }

    fn paragraph(&mut self, inlines: &[Inline], width: usize) -> Out {
        let pieces = self.pieces(inlines, Style::default());
        wrap(&pieces, width)
    }

    fn heading(&mut self, level: u8, content: &[Inline], width: usize) -> Out {
        let (content, id) = header_id(content);
        let heading = Style {
            bold: true,
            ..style(Role::Heading(level))
        };
        let pieces = self.pieces(&content, heading);
        let mut out = wrap(&pieces, width);
        if out.is_empty() {
            return out;
        }
        let text = clean(&replace_baseurl(
            &Inline::plain_text(&content),
            &self.options.baseurl,
        ));
        let text = text.trim().to_owned();
        let slug = match id {
            Some(id) => self.slugger.unique(&id),
            None => self.slugger.slug(&text),
        };
        self.headings.push((level, text, slug));
        out.marks.insert(
            0,
            Mark {
                line: 0,
                column: 0,
                kind: MarkKind::Heading(self.headings.len() - 1),
            },
        );
        if level <= 2 {
            let rule = if level == 1 {
                self.pick("═", "=")
            } else {
                self.pick("─", "-")
            };
            out.lines
                .push(repeat(rule, width, style(Role::Heading(level))));
        }
        out
    }

    /// `rows` in a frame with `label` in its top border; without the frame when it would not fit.
    fn framed(
        &self,
        label: &str,
        rows: &[String],
        width: usize,
        body: Style,
        border: Style,
    ) -> Out {
        let (tl, tr, bl, br, h, v) = if self.unicode() {
            ("╭", "╮", "╰", "╯", "─", "│")
        } else {
            ("+", "+", "+", "+", "-", "|")
        };
        let continued = self.pick("↪ ", "> ");
        let mark = Style {
            dim: true,
            ..border
        };
        let mut out = Out::default();
        if width < 4 + MIN_INNER {
            for row in rows {
                for (k, chunk) in split_cells(row, width, width - 2).into_iter().enumerate() {
                    let mut line = crate::model::Line::default();
                    if k > 0 {
                        line.push(continued, mark);
                    }
                    line.push(&chunk, body);
                    out.lines.push(line);
                }
            }
            return out;
        }
        let inner = width - 4;
        let mut top = crate::model::Line::default();
        top.push(tl, border);
        top.push(h, border);
        let label_width = display_width(label);
        if !label.is_empty() && label_width + 6 <= width {
            top.push(" ", border);
            top.push(
                label,
                Style {
                    dim: border.dim,
                    ..style(Role::Marker)
                },
            );
            top.push(" ", border);
            top.push(&h.repeat(width - 5 - label_width), border);
        } else {
            top.push(&h.repeat(width - 3), border);
        }
        top.push(tr, border);
        out.lines.push(top);
        for row in rows {
            for (k, chunk) in split_cells(row, inner, inner - 2).into_iter().enumerate() {
                let mut line = crate::model::Line::default();
                line.push(v, border);
                line.push(" ", border);
                let mut used = display_width(&chunk);
                if k > 0 {
                    line.push(continued, mark);
                    used += 2;
                }
                line.push(&chunk, body);
                line.push(&" ".repeat(inner.saturating_sub(used)), body);
                line.push(" ", border);
                line.push(v, border);
                out.lines.push(line);
            }
        }
        let mut bottom = crate::model::Line::default();
        bottom.push(bl, border);
        bottom.push(&h.repeat(width - 2), border);
        bottom.push(br, border);
        out.lines.push(bottom);
        out
    }

    fn quote(&mut self, children: &[Block], width: usize) -> Out {
        if width < 2 + MIN_INNER {
            return self.blocks(children, width, false);
        }
        let bar = repeat(self.pick("│ ", "> "), 1, style(Role::Quote));
        self.blocks(children, width - 2, false).prefixed(&bar, &bar)
    }

    fn callout(&mut self, item: &Item<'_>, callout: Callout, width: usize) -> Out {
        let role = Role::Callout(callout.kind);
        let label_style = Style {
            bold: true,
            ..style(role)
        };
        let (inner, bar) = if width < 2 + MIN_INNER {
            (width, None)
        } else {
            (
                width - 2,
                Some(repeat(self.pick("┃ ", "| "), 1, style(role))),
            )
        };
        let mut label = vec![Piece::text(callout.kind.label(), label_style)];
        let body = match item {
            Item::Block(Block::BlockQuote(children)) => {
                let mut children = children.as_slice();
                if callout.titled
                    && let Some((Block::Paragraph(title), rest)) = children.split_first()
                {
                    label = self.pieces(title, label_style);
                    children = rest;
                }
                self.blocks(children, inner, false)
            }
            _ => self.item(item, inner),
        };
        let mut out = wrap(&label, inner);
        out.append(body);
        match bar {
            Some(bar) => out.prefixed(&bar, &bar),
            None => out,
        }
    }

    /// `marker` before the first line of what `render` lays out, the other lines indented to `indent`; the
    /// marker on a line of its own when the indent would leave too little room.
    fn hanging(
        &mut self,
        marker: &crate::model::Line,
        indent: usize,
        width: usize,
        render: impl FnOnce(&mut Self, usize) -> Out,
    ) -> Out {
        if indent + MIN_INNER > width {
            let mut out = Out::default();
            for span in &marker.spans {
                out.append(hard_lines(&span.text, width, span.style));
            }
            out.append(render(self, width));
            return out;
        }
        let content = render(self, width - indent);
        let mut first = marker.clone();
        first.extend(&spaces(indent - marker.width()));
        if content.is_empty() {
            return Out::line(marker.clone());
        }
        content.prefixed(&first, &spaces(indent))
    }

    fn list(&mut self, list: &List, width: usize) -> Out {
        let bullets: [&str; 3] = if self.unicode() {
            ["•", "◦", "▪"]
        } else {
            ["*", "-", "+"]
        };
        let (unchecked, checked) = (self.pick("☐", "[ ]"), self.pick("☑", "[x]"));
        let numbers: Vec<String> = (0..list.items.len())
            .map(|i| format!("{}{}", list.start.saturating_add(i), list.marker))
            .collect();
        let number_width = numbers.iter().map(String::len).max().unwrap_or(0);
        let markers: Vec<String> = list
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let base = if list.ordered {
                    format!("{:>number_width$}", numbers[i])
                } else {
                    bullets[self.depth % 3].to_owned()
                };
                match item.task {
                    Some(done) if list.ordered => {
                        format!("{base} {}", if done { checked } else { unchecked })
                    }
                    Some(done) => (if done { checked } else { unchecked }).to_owned(),
                    None => base,
                }
            })
            .collect();
        // Numbers are right-aligned to the widest; a bullet or task box sits just before its own text.
        let widest = markers.iter().map(|m| display_width(m)).max().unwrap_or(0);
        self.depth += 1;
        let mut out = Out::default();
        for (item, marker) in list.items.iter().zip(&markers) {
            let indent = if list.ordered {
                widest + 1
            } else {
                display_width(marker) + 1
            };
            let marker = repeat(
                &format!("{marker:>width$}", width = indent - 1),
                1,
                style(Role::Marker),
            );
            let rendered = self.hanging(&marker, indent, width, |ctx, inner| {
                ctx.blocks(&item.blocks, inner, list.tight)
            });
            join(&mut out, rendered, list.tight);
        }
        self.depth -= 1;
        out
    }

    fn display_math(&mut self, tex: &str, width: usize) -> Out {
        let lines = match self.options.math {
            MathMode::Text => self.math.display(tex),
            MathMode::Source => None,
        };
        let lines: Option<Vec<String>> = lines
            .map(|lines| {
                lines
                    .iter()
                    .map(|l| expand_tabs(l).trim_end().to_owned())
                    .collect::<Vec<_>>()
            })
            .filter(|lines| {
                lines.iter().any(|l| !l.is_empty())
                    && lines.iter().all(|l| display_width(l) <= width)
            });
        if let Some(lines) = lines {
            let widest = lines.iter().map(|l| display_width(l)).max().unwrap_or(0);
            let left = (width - widest) / 2;
            let mut out = Out::default();
            for line in &lines {
                let mut row = crate::model::Line::default();
                if !line.is_empty() {
                    row.push(&" ".repeat(left), Style::default());
                    row.push(line, style(Role::Math));
                }
                out.lines.push(row);
            }
            let index = self.formula(tex, true, widest, lines.len());
            out.marks.push(Mark {
                line: 0,
                column: left,
                kind: MarkKind::Formula(index),
            });
            return out;
        }
        let dim = Style {
            dim: true,
            ..style(Role::Math)
        };
        let mut out = self.framed("tex", &code_rows(tex.trim()), width, dim, dim);
        let index = self.formula(
            tex,
            true,
            out.lines
                .iter()
                .map(crate::model::Line::width)
                .max()
                .unwrap_or(0),
            out.lines.len(),
        );
        out.mark(MarkKind::Formula(index));
        out
    }

    fn footnote(&self, number: usize) -> Option<&'a Footnote> {
        self.footnotes.iter().find(|f| f.number == number)
    }

    /// A definition as a dim indented block, for [`FootnotePlacement::Inline`](crate::FootnotePlacement).
    fn footnote_inline(&mut self, number: usize, width: usize) -> Out {
        let Some(footnote) = self.footnote(number) else {
            return Out::default();
        };
        let mut marker = spaces(2);
        marker.push(
            &superscript(number, self.unicode()),
            Style {
                dim: true,
                ..style(Role::Footnote)
            },
        );
        let indent = marker.width() + 1;
        let mut out = self.hanging(&marker, indent, width, |ctx, inner| {
            ctx.blocks(&footnote.blocks, inner, false)
        });
        out.restyle(|s| s.dim = true);
        out.marks.insert(
            0,
            Mark {
                line: 0,
                column: 0,
                kind: MarkKind::FootnoteDef(number),
            },
        );
        out
    }

    /// The definitions of `numbers` under a rule, each `1. text ↩` (`[^1] text ^` without Unicode).
    fn footnotes_end(&mut self, numbers: &[usize], width: usize) -> Out {
        let mut out = Out::line(repeat(self.pick("─", "-"), width, style(Role::Rule)));
        // `1.` with Unicode; `[^1]` without, like the reference, so it cannot be taken for a link number.
        let unicode = self.unicode();
        let label = |number: usize| {
            if unicode {
                format!("{number}.")
            } else {
                format!("[^{number}]")
            }
        };
        let widest = numbers.iter().map(|&n| label(n).len()).max().unwrap_or(0);
        let spaced = numbers
            .iter()
            .filter_map(|&n| self.footnote(n))
            .any(|f| f.blocks.len() > 1);
        let back = self.pick("↩", "^");
        for &number in numbers {
            let Some(footnote) = self.footnote(number) else {
                continue;
            };
            self.emitted.insert(number);
            let marker = repeat(
                &format!("{:>widest$}", label(number)),
                1,
                style(Role::Footnote),
            );
            let link = self.footnote_link(number, true);
            let mut definition = self.hanging(&marker, widest + 1, width, |ctx, inner| {
                let mut content = ctx.blocks(&footnote.blocks, inner, false);
                let back_style = Style {
                    link: Some(link),
                    ..style(Role::Footnote)
                };
                let ends_in_text = matches!(footnote.blocks.last(), Some(Block::Paragraph(_)));
                match content.lines.last_mut() {
                    Some(last) if ends_in_text && last.width() + 2 <= inner => {
                        last.push(" ", Style::default());
                        last.push(back, back_style);
                    }
                    _ => content.append(Out::line(repeat(back, 1, back_style))),
                }
                content
            });
            definition.marks.insert(
                0,
                Mark {
                    line: 0,
                    column: 0,
                    kind: MarkKind::FootnoteDef(number),
                },
            );
            out.append(definition);
            if spaced {
                out.blank();
            }
        }
        if spaced {
            out.lines.pop();
        }
        out
    }

    /// The numbered links' destinations, `[n] url`, under a rule.
    fn link_list(&self, width: usize) -> Out {
        let mut out = Out::line(repeat(self.pick("─", "-"), width, style(Role::Rule)));
        for (i, destination) in self.numbered.iter().enumerate() {
            let number = i + 1;
            let link = self.links.iter().position(|l| l.number == Some(number));
            let dim = Style {
                dim: true,
                link,
                ..style(Role::Link)
            };
            let pieces = [
                Piece::glued(format!("[{number}]"), dim),
                Piece::text(" ", dim),
                Piece::text(clean(destination), dim),
            ];
            out.append(wrap(&pieces, width));
        }
        out
    }

    /// Lays out `document`.
    pub fn render(mut self, document: &Document) -> Rendered {
        let width = self.options.width.max(MIN_WIDTH);
        let mut out = self.blocks(&document.blocks, width, false);
        let rest: Vec<usize> = document
            .footnotes
            .iter()
            .map(|f| f.number)
            .filter(|n| !self.emitted.contains(n))
            .collect();
        if !rest.is_empty() {
            let notes = self.footnotes_end(&rest, width);
            join(&mut out, notes, false);
        }
        if !self.numbered.is_empty() {
            let list = self.link_list(width);
            join(&mut out, list, false);
        }
        self.finish(out)
    }

    fn finish(self, out: Out) -> Rendered {
        let mut headings = Vec::new();
        let mut formulas: Vec<Option<FormulaSpot>> = vec![None; self.formulas.len()];
        let mut definitions: HashMap<usize, usize> = HashMap::new();
        let mut references: HashMap<usize, Vec<usize>> = HashMap::new();
        for mark in &out.marks {
            match mark.kind {
                MarkKind::Heading(index) => {
                    let (level, text, slug) = &self.headings[index];
                    headings.push(HeadingSpot {
                        level: *level,
                        text: text.clone(),
                        slug: slug.clone(),
                        line: mark.line,
                    });
                }
                MarkKind::FootnoteRef(number) => {
                    references.entry(number).or_default().push(mark.line);
                }
                MarkKind::FootnoteDef(number) => {
                    definitions.entry(number).or_insert(mark.line);
                }
                MarkKind::Formula(index) => {
                    if formulas[index].is_none() {
                        formulas[index] = Some(FormulaSpot {
                            line: mark.line,
                            column: mark.column,
                            ..self.formulas[index].clone()
                        });
                    }
                }
            }
        }
        headings.sort_by_key(|h| h.line);
        let footnotes = self
            .footnotes
            .iter()
            .map(|f| {
                let mut reference_lines = references.remove(&f.number).unwrap_or_default();
                reference_lines.sort_unstable();
                FootnoteSpot {
                    number: f.number,
                    label: f.label.clone(),
                    definition_line: definitions.get(&f.number).copied().unwrap_or(0),
                    reference_lines,
                }
            })
            .collect();
        Rendered {
            lines: out.lines,
            links: self.links,
            headings,
            footnotes,
            formulas: formulas.into_iter().flatten().collect(),
        }
    }
}

fn join(out: &mut Out, next: Out, tight: bool) {
    if !out.is_empty() && !tight {
        out.blank();
    }
    out.append(next);
}
