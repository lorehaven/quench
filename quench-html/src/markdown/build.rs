//! From the parser's block tree to the public [`ast`](super::ast): inlines read, footnotes numbered in the
//! order of their first reference, task list markers taken off their items, display math recognised.

use super::ast::{Alignment, Block, Document, Footnote, Inline, List, ListItem, Options, Table};
use super::blocks::{self, Align, Kind};
use super::{inlines, scan};
use std::collections::HashMap;

struct Builder<'a> {
    document: &'a blocks::Document,
    options: Options,
    /// Normalized label to the label as defined, for the inline parser.
    labels: HashMap<String, String>,
    /// Normalized label to (number, references so far).
    numbers: HashMap<String, (usize, usize)>,
    /// Normalized labels in the order of their first reference.
    order: Vec<String>,
}

pub fn build(document: &blocks::Document, options: Options) -> Document {
    let labels = document
        .footnotes
        .iter()
        .map(|(key, &id)| (key.clone(), document.nodes[id].label.clone()))
        .collect();
    let mut builder = Builder {
        document,
        options,
        labels,
        numbers: HashMap::new(),
        order: Vec::new(),
    };
    let blocks = builder.blocks(&document.nodes[0].children);
    // A footnote may reference another one: keep going until no new footnote is referenced.
    let mut footnotes = Vec::new();
    let mut next = 0;
    while next < builder.order.len() {
        let key = builder.order[next].clone();
        next += 1;
        let id = document.footnotes[&key];
        let blocks = builder.blocks(&document.nodes[id].children);
        footnotes.push(Footnote {
            label: document.nodes[id].label.clone(),
            number: next,
            references: 0,
            blocks,
        });
    }
    for footnote in &mut footnotes {
        let key = footnote_key(&footnote.label);
        footnote.references = builder.numbers.get(&key).map_or(1, |&(_, count)| count);
    }
    Document { blocks, footnotes }
}

fn footnote_key(label: &str) -> String {
    scan::normalize_label(&format!("[{label}]"))
}

impl Builder<'_> {
    fn blocks(&mut self, ids: &[usize]) -> Vec<Block> {
        ids.iter().filter_map(|&id| self.block(id)).collect()
    }

    fn block(&mut self, id: usize) -> Option<Block> {
        let node = &self.document.nodes[id];
        Some(match node.kind {
            Kind::Document | Kind::Item | Kind::FootnoteDefinition => return None,
            Kind::Paragraph => {
                if self.options.math
                    && let Some(tex) = display_math(&node.content)
                {
                    Block::Math(tex)
                } else {
                    Block::Paragraph(self.inlines(&node.content))
                }
            }
            Kind::Heading => Block::Heading {
                level: u8::try_from(node.level).unwrap_or(6),
                content: self.inlines(&node.content),
            },
            Kind::ThematicBreak => Block::ThematicBreak,
            Kind::CodeBlock => {
                let word = node.info.split_whitespace().next().unwrap_or("");
                if self.options.math && node.fenced && word == "math" {
                    Block::Math(node.literal.trim_end_matches('\n').to_string())
                } else {
                    Block::CodeBlock {
                        info: node.info.clone(),
                        literal: node.literal.clone(),
                        fenced: node.fenced,
                    }
                }
            }
            Kind::HtmlBlock => Block::Html(node.literal.clone()),
            Kind::MathBlock => Block::Math(node.literal.trim_end_matches('\n').to_string()),
            Kind::BlockQuote => {
                let children = node.children.clone();
                Block::BlockQuote(self.blocks(&children))
            }
            Kind::List => self.list(id),
            Kind::Table => self.table(id),
        })
    }

    fn list(&mut self, id: usize) -> Block {
        let node = &self.document.nodes[id];
        let data = node.list.clone();
        let items = node.children.clone();
        let items = items
            .iter()
            .map(|&item| {
                let children = self.document.nodes[item].children.clone();
                let task = self.task(&children);
                let mut blocks = Vec::new();
                for (n, &child) in children.iter().enumerate() {
                    let block = if n == 0
                        && let Some((_, rest)) = &task
                    {
                        Some(Block::Paragraph(self.inlines(rest)))
                    } else {
                        self.block(child)
                    };
                    blocks.extend(block);
                }
                ListItem {
                    task: task.map(|(checked, _)| checked),
                    blocks,
                }
            })
            .collect();
        Block::List(List {
            ordered: data.ordered,
            start: data.start,
            tight: data.tight,
            marker: char::from(if data.ordered {
                data.delimiter
            } else {
                data.bullet
            }),
            items,
        })
    }

    /// `[ ] ` or `[x] ` at the start of an item's first paragraph: whether it is checked, and the rest.
    fn task(&self, children: &[usize]) -> Option<(bool, String)> {
        if !self.options.task_lists {
            return None;
        }
        let first = &self.document.nodes[*children.first()?];
        if first.kind != Kind::Paragraph {
            return None;
        }
        let content = &first.content;
        let checked = match content.get(..3)? {
            "[ ]" => false,
            "[x]" | "[X]" => true,
            _ => return None,
        };
        let rest = &content[3..];
        rest.starts_with([' ', '\t', '\n'])
            .then(|| (checked, rest.to_string()))
    }

    fn table(&mut self, id: usize) -> Block {
        let node = &self.document.nodes[id];
        let alignments: Vec<Alignment> = node
            .aligns
            .iter()
            .map(|a| match a {
                Align::None => Alignment::None,
                Align::Left => Alignment::Left,
                Align::Center => Alignment::Center,
                Align::Right => Alignment::Right,
            })
            .collect();
        let width = alignments.len();
        let rows = node.rows.clone();
        let mut cells = |line: &str| -> Vec<Vec<Inline>> {
            let mut row: Vec<Vec<Inline>> = blocks::split_row(line)
                .iter()
                .take(width)
                .map(|cell| self.inlines(&cell.replace("\\|", "|")))
                .collect();
            row.resize(width, Vec::new());
            row
        };
        let header = rows.first().map_or_else(Vec::new, |line| cells(line));
        let body = rows.iter().skip(1).map(|line| cells(line)).collect();
        Block::Table(Table {
            alignments,
            header,
            rows: body,
        })
    }

    /// The inlines of `content`, with the footnote references in it numbered.
    fn inlines(&mut self, content: &str) -> Vec<Inline> {
        let mut list = inlines::parse(
            content,
            &self.document.references,
            &self.labels,
            self.options,
        );
        self.number(&mut list);
        list
    }

    fn number(&mut self, list: &mut [Inline]) {
        for inline in list {
            match inline {
                Inline::FootnoteReference {
                    label,
                    number,
                    occurrence,
                } => {
                    let key = footnote_key(label);
                    let next = self.order.len() + 1;
                    let entry = self.numbers.entry(key.clone()).or_insert((next, 0));
                    if entry.0 == next && entry.1 == 0 {
                        self.order.push(key);
                    }
                    entry.1 += 1;
                    *number = entry.0;
                    *occurrence = entry.1;
                }
                Inline::Emph(c)
                | Inline::Strong(c)
                | Inline::Strikethrough(c)
                | Inline::Link { children: c, .. }
                | Inline::Image { children: c, .. } => self.number(c),
                _ => {}
            }
        }
    }
}

/// A paragraph that is only `$$...$$`: its TeX.
fn display_math(content: &str) -> Option<String> {
    let inner = content.trim().strip_prefix("$$")?.strip_suffix("$$")?;
    (!inner.trim().is_empty() && !inner.contains("$$")).then(|| inner.trim().to_string())
}
