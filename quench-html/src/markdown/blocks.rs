//! Block structure: containers (block quotes, lists) and leaves (paragraphs, headings, code, HTML).
//!
//! This follows the reference algorithm of the `CommonMark` spec: each line first continues the open
//! containers, then may start new blocks, and what is left is added to the innermost open block. With the
//! extensions of [`Options`] it also reads tables, footnote definitions and `$$` math blocks.

use super::ast::Options;
use super::scan;
use std::collections::HashMap;

/// How deep containers (block quotes, list items, footnotes) may nest. A marker beyond it is text, so input
/// built to nest thousands deep cannot exhaust the stack of the recursive steps after parsing.
pub const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Document,
    BlockQuote,
    List,
    Item,
    Paragraph,
    Heading,
    ThematicBreak,
    CodeBlock,
    HtmlBlock,
    /// A GFM table: `rows` holds the header row first, then the body rows, as raw text.
    Table,
    /// `[^label]: ...`, a container of the blocks of a footnote.
    FootnoteDefinition,
    /// A display formula between `$$` lines.
    MathBlock,
}

/// How a table column is aligned, from its delimiter cell (`:--`, `:-:`, `--:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Default)]
pub struct ListData {
    pub ordered: bool,
    pub tight: bool,
    pub bullet: u8,
    pub start: usize,
    pub delimiter: u8,
    pub padding: usize,
    pub marker_offset: usize,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: Kind,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    open: bool,
    last_line_blank: bool,
    last_line_checked: bool,
    /// Raw text of paragraphs and headings, before inline parsing.
    pub content: String,
    /// Final text of code and HTML blocks.
    pub literal: String,
    pub level: usize,
    pub info: String,
    pub fenced: bool,
    fence_char: u8,
    fence_length: usize,
    fence_offset: usize,
    html_type: u8,
    pub list: ListData,
    start_line: usize,
    /// A table's column alignments.
    pub aligns: Vec<Align>,
    /// A table's rows (the header first), each the raw text of the line.
    pub rows: Vec<String>,
    /// A footnote definition's label, as written.
    pub label: String,
}

impl Node {
    fn new(kind: Kind, start_line: usize) -> Self {
        Self {
            kind,
            parent: None,
            children: Vec::new(),
            open: true,
            last_line_blank: false,
            last_line_checked: false,
            content: String::new(),
            literal: String::new(),
            level: 0,
            info: String::new(),
            fenced: false,
            fence_char: 0,
            fence_length: 0,
            fence_offset: 0,
            html_type: 0,
            list: ListData::default(),
            start_line,
            aligns: Vec::new(),
            rows: Vec::new(),
            label: String::new(),
        }
    }
}

#[derive(Debug)]
pub struct Reference {
    pub destination: String,
    pub title: String,
}

#[derive(Debug)]
pub struct Document {
    pub nodes: Vec<Node>,
    pub references: HashMap<String, Reference>,
    /// Footnote definitions by normalized label: the node of the first definition of each.
    pub footnotes: HashMap<String, usize>,
}

struct Parser {
    nodes: Vec<Node>,
    tip: usize,
    old_tip: usize,
    last_matched: usize,
    all_closed: bool,
    line: String,
    line_number: usize,
    offset: usize,
    column: usize,
    next_nonspace: usize,
    next_nonspace_column: usize,
    indent: usize,
    indented: bool,
    blank: bool,
    partially_consumed_tab: bool,
    references: HashMap<String, Reference>,
    footnotes: HashMap<String, usize>,
    options: Options,
}

const fn is_space_or_tab(c: Option<u8>) -> bool {
    matches!(c, Some(b' ' | b'\t'))
}

const fn accepts_lines(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Paragraph | Kind::CodeBlock | Kind::HtmlBlock | Kind::MathBlock
    )
}

const fn can_contain(parent: Kind, child: Kind) -> bool {
    match parent {
        Kind::Document | Kind::BlockQuote | Kind::Item | Kind::FootnoteDefinition => {
            !matches!(child, Kind::Item)
        }
        Kind::List => matches!(child, Kind::Item),
        _ => false,
    }
}

/// Parses Markdown into its block tree.
pub fn parse(text: &str, options: Options) -> Document {
    let text = text.replace('\0', "\u{FFFD}");
    let mut parser = Parser {
        nodes: vec![Node::new(Kind::Document, 1)],
        tip: 0,
        old_tip: 0,
        last_matched: 0,
        all_closed: true,
        line: String::new(),
        line_number: 0,
        offset: 0,
        column: 0,
        next_nonspace: 0,
        next_nonspace_column: 0,
        indent: 0,
        indented: false,
        blank: false,
        partially_consumed_tab: false,
        references: HashMap::new(),
        footnotes: HashMap::new(),
        options,
    };
    let mut lines: Vec<&str> = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                lines.push(&text[start..i]);
                start = i + 1;
            }
            b'\r' => {
                lines.push(&text[start..i]);
                if bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    for line in lines {
        parser.incorporate_line(line);
    }
    while parser.tip != 0 || parser.nodes[0].open {
        let tip = parser.tip;
        parser.finalize(tip);
        if tip == 0 {
            break;
        }
    }
    Document {
        nodes: parser.nodes,
        references: parser.references,
        footnotes: parser.footnotes,
    }
}

impl Parser {
    fn peek(&self, i: usize) -> Option<u8> {
        self.line.as_bytes().get(i).copied()
    }

    fn find_next_nonspace(&mut self) {
        let bytes = self.line.as_bytes();
        let (mut i, mut cols) = (self.offset, self.column);
        while let Some(&c) = bytes.get(i) {
            match c {
                b' ' => {
                    i += 1;
                    cols += 1;
                }
                b'\t' => {
                    i += 1;
                    cols += 4 - (cols % 4);
                }
                _ => break,
            }
        }
        self.blank = bytes.get(i).is_none_or(|&c| c == b'\n' || c == b'\r');
        self.next_nonspace = i;
        self.next_nonspace_column = cols;
        self.indent = cols - self.column;
        self.indented = self.indent >= 4;
    }

    const fn advance_next_nonspace(&mut self) {
        self.offset = self.next_nonspace;
        self.column = self.next_nonspace_column;
        self.partially_consumed_tab = false;
    }

    fn advance_offset(&mut self, mut count: usize, columns: bool) {
        while count > 0 {
            let Some(c) = self.peek(self.offset) else {
                break;
            };
            if c == b'\t' {
                let to_tab = 4 - (self.column % 4);
                if columns {
                    self.partially_consumed_tab = to_tab > count;
                    let advance = to_tab.min(count);
                    self.column += advance;
                    self.offset += usize::from(!self.partially_consumed_tab);
                    count -= advance;
                } else {
                    self.partially_consumed_tab = false;
                    self.column += to_tab;
                    self.offset += 1;
                    count -= 1;
                }
            } else {
                self.partially_consumed_tab = false;
                self.offset += 1;
                self.column += 1;
                count -= 1;
            }
        }
    }

    fn add_child(&mut self, kind: Kind) -> usize {
        while !can_contain(self.nodes[self.tip].kind, kind) {
            let tip = self.tip;
            self.finalize(tip);
        }
        let id = self.nodes.len();
        let mut node = Node::new(kind, self.line_number);
        node.parent = Some(self.tip);
        self.nodes.push(node);
        self.nodes[self.tip].children.push(id);
        self.tip = id;
        id
    }

    fn close_unmatched(&mut self) {
        if !self.all_closed {
            while self.old_tip != self.last_matched {
                let parent = self.nodes[self.old_tip].parent.unwrap_or(0);
                let old = self.old_tip;
                self.finalize(old);
                self.old_tip = parent;
            }
            self.all_closed = true;
        }
    }

    fn add_line(&mut self) {
        let mut text = String::new();
        let mut from = self.offset;
        if self.partially_consumed_tab {
            from += 1;
            let to_tab = 4 - (self.column % 4);
            text.push_str(&" ".repeat(to_tab));
        }
        text.push_str(&self.line[from.min(self.line.len())..]);
        text.push('\n');
        let tip = self.tip;
        self.nodes[tip].content.push_str(&text);
    }

    /// Strips leading link reference definitions from a paragraph's content.
    fn strip_references(&mut self, id: usize) {
        while self.nodes[id].content.starts_with('[') {
            let content = self.nodes[id].content.clone();
            let used = self.parse_reference(&content);
            if used == 0 {
                break;
            }
            self.nodes[id].content = content[used..].to_string();
        }
    }

    fn parse_reference(&mut self, s: &str) -> usize {
        let label_len = scan::link_label(s);
        if label_len == 0 {
            return 0;
        }
        let raw = &s[..label_len];
        let mut pos = label_len;
        if s.as_bytes().get(pos) != Some(&b':') {
            return 0;
        }
        pos += 1;
        pos += spnl(&s[pos..]);
        let Some((destination, used)) = scan::link_destination(&s[pos..]) else {
            return 0;
        };
        if used == 0 && !s[pos..].starts_with("<>") {
            return 0;
        }
        pos += used;
        let before_title = pos;
        pos += spnl(&s[pos..]);
        let mut title = String::new();
        if pos != before_title
            && let Some((t, used)) = scan::link_title(&s[pos..])
        {
            title = t;
            pos += used;
        } else {
            pos = before_title;
        }
        let at_line_end = |from: usize| {
            let rest = &s[from..];
            let spaces = rest.bytes().take_while(|&c| c == b' ').count();
            match rest.as_bytes().get(spaces) {
                None => Some(from + spaces),
                Some(b'\n') => Some(from + spaces + 1),
                _ => None,
            }
        };
        let end = match at_line_end(pos) {
            Some(end) => end,
            None if !title.is_empty() => {
                // The title is not at the end of its line; the definition may still stand without it.
                title.clear();
                match at_line_end(before_title) {
                    Some(end) => end,
                    None => return 0,
                }
            }
            None => return 0,
        };
        let label = scan::normalize_label(raw);
        if label.is_empty() {
            return 0;
        }
        self.references
            .entry(label)
            .or_insert(Reference { destination, title });
        end
    }

    fn finalize(&mut self, id: usize) {
        let above = self.nodes[id].parent;
        self.nodes[id].open = false;
        match self.nodes[id].kind {
            Kind::Paragraph => {
                self.strip_references(id);
                if self.nodes[id].content.trim().is_empty()
                    && let Some(parent) = above
                {
                    self.nodes[parent].children.retain(|&c| c != id);
                }
            }
            Kind::CodeBlock => {
                let content = std::mem::take(&mut self.nodes[id].content);
                if self.nodes[id].fenced {
                    let (first, rest) = content.split_once('\n').unwrap_or((&content, ""));
                    self.nodes[id].info = scan::unescape(first.trim());
                    self.nodes[id].literal = rest.to_string();
                } else {
                    // Trailing blank lines are not part of the code.
                    let trimmed = scan::strip_trailing_blank_lines(&content);
                    self.nodes[id].literal = if trimmed.is_empty() {
                        String::new()
                    } else {
                        format!("{trimmed}\n")
                    };
                }
            }
            Kind::HtmlBlock => {
                let content = std::mem::take(&mut self.nodes[id].content);
                self.nodes[id].literal = scan::strip_trailing_blank_lines(&content).to_string();
            }
            Kind::MathBlock => {
                // The first line is what followed the opening `$$`, which is nothing.
                let content = std::mem::take(&mut self.nodes[id].content);
                let rest = content.split_once('\n').map_or("", |(_, rest)| rest);
                self.nodes[id].literal = rest.to_string();
            }
            Kind::List => {
                let items = self.nodes[id].children.clone();
                let mut tight = true;
                for (n, &item) in items.iter().enumerate() {
                    let has_next = n + 1 < items.len();
                    if self.ends_with_blank_line(item) && has_next {
                        tight = false;
                        break;
                    }
                    let subs = self.nodes[item].children.clone();
                    for (m, &sub) in subs.iter().enumerate() {
                        if self.ends_with_blank_line(sub) && (has_next || m + 1 < subs.len()) {
                            tight = false;
                            break;
                        }
                    }
                    if !tight {
                        break;
                    }
                }
                self.nodes[id].list.tight = tight;
            }
            _ => {}
        }
        if let Some(parent) = above {
            self.tip = parent;
        }
    }

    fn ends_with_blank_line(&mut self, mut id: usize) -> bool {
        loop {
            if self.nodes[id].last_line_blank {
                return true;
            }
            let kind = self.nodes[id].kind;
            if !self.nodes[id].last_line_checked && matches!(kind, Kind::List | Kind::Item) {
                self.nodes[id].last_line_checked = true;
                match self.nodes[id].children.last() {
                    Some(&last) => id = last,
                    None => return false,
                }
            } else {
                self.nodes[id].last_line_checked = true;
                return false;
            }
        }
    }

    /// 0: the container continues on this line, 1: it does not, 2: the line is used up (a closing fence).
    fn continue_block(&mut self, id: usize) -> u8 {
        match self.nodes[id].kind {
            Kind::Document | Kind::List => 0,
            Kind::BlockQuote => {
                if !self.indented && self.peek(self.next_nonspace) == Some(b'>') {
                    self.advance_next_nonspace();
                    self.advance_offset(1, false);
                    if is_space_or_tab(self.peek(self.offset)) {
                        self.advance_offset(1, true);
                    }
                    0
                } else {
                    1
                }
            }
            Kind::Item => {
                let list = self.nodes[id].list.clone();
                if self.blank {
                    if self.nodes[id].children.is_empty() {
                        return 1;
                    }
                    self.advance_next_nonspace();
                } else if self.indent >= list.marker_offset + list.padding {
                    self.advance_offset(list.marker_offset + list.padding, true);
                } else {
                    return 1;
                }
                0
            }
            Kind::Heading | Kind::ThematicBreak => 1,
            Kind::CodeBlock => {
                if self.nodes[id].fenced {
                    let fence_char = self.nodes[id].fence_char;
                    let rest = &self.line[self.next_nonspace.min(self.line.len())..];
                    let run = rest.bytes().take_while(|&c| c == fence_char).count();
                    if self.indent <= 3
                        && self.peek(self.next_nonspace) == Some(fence_char)
                        && run >= self.nodes[id].fence_length
                        && rest[run..].bytes().all(|c| c == b' ' || c == b'\t')
                    {
                        self.finalize(id);
                        return 2;
                    }
                    let mut skip = self.nodes[id].fence_offset;
                    while skip > 0 && is_space_or_tab(self.peek(self.offset)) {
                        self.advance_offset(1, true);
                        skip -= 1;
                    }
                    0
                } else if self.indent >= 4 {
                    self.advance_offset(4, true);
                    0
                } else if self.blank {
                    self.advance_next_nonspace();
                    0
                } else {
                    1
                }
            }
            Kind::HtmlBlock => u8::from(self.blank && matches!(self.nodes[id].html_type, 6 | 7)),
            Kind::Paragraph | Kind::Table => u8::from(self.blank),
            Kind::FootnoteDefinition => {
                if self.blank {
                    self.advance_next_nonspace();
                    0
                } else if self.indent >= 4 {
                    self.advance_offset(4, true);
                    0
                } else {
                    1
                }
            }
            Kind::MathBlock => {
                let rest = &self.line[self.next_nonspace.min(self.line.len())..];
                if self.indent <= 3 && rest.trim_end_matches([' ', '\t']) == "$$" {
                    self.finalize(id);
                    return 2;
                }
                let mut skip = self.nodes[id].fence_offset;
                while skip > 0 && is_space_or_tab(self.peek(self.offset)) {
                    self.advance_offset(1, true);
                    skip -= 1;
                }
                0
            }
        }
    }

    fn incorporate_line(&mut self, line: &str) {
        self.line_number += 1;
        self.line = line.to_string();
        self.offset = 0;
        self.column = 0;
        self.blank = false;
        self.partially_consumed_tab = false;
        self.old_tip = self.tip;

        // Continue the open containers.
        let mut container = 0usize;
        self.all_closed = true;
        while let Some(&last) = self.nodes[container].children.last() {
            if !self.nodes[last].open {
                break;
            }
            container = last;
            self.find_next_nonspace();
            match self.continue_block(container) {
                0 => {}
                1 => {
                    container = self.nodes[container].parent.unwrap_or(0);
                    break;
                }
                _ => return,
            }
        }
        self.all_closed = container == self.old_tip;
        self.last_matched = container;

        let mut matched_leaf = self.nodes[container].kind != Kind::Paragraph
            && accepts_lines(self.nodes[container].kind);
        while !matched_leaf {
            self.find_next_nonspace();
            let tip_is_table = self.nodes[self.tip].kind == Kind::Table;
            let special = self.peek(self.next_nonspace).is_some_and(|c| {
                matches!(
                    c,
                    b'#' | b'`' | b'~' | b'*' | b'+' | b'_' | b'=' | b'<' | b'>' | b'-' | b'0'
                        ..=b'9'
                ) || (c == b'[' && self.options.footnotes)
                    || (c == b'$' && self.options.math)
                    || (c == b'|' && self.options.tables)
                    || (self.options.tables
                        && self.nodes[container].kind == Kind::Paragraph
                        && !tip_is_table)
            });
            if !self.indented && !special {
                self.advance_next_nonspace();
                break;
            }
            match self.try_block_starts(container) {
                0 => {
                    self.advance_next_nonspace();
                    break;
                }
                result => {
                    container = self.tip;
                    if result == 2 {
                        matched_leaf = true;
                    }
                }
            }
        }

        // What remains is text for the innermost block.
        let tip_is_paragraph = self.nodes[self.tip].kind == Kind::Paragraph;
        if !self.all_closed && !self.blank && tip_is_paragraph {
            self.add_line();
            return;
        }
        self.close_unmatched();
        if self.blank
            && let Some(&last) = self.nodes[container].children.last()
        {
            self.nodes[last].last_line_blank = true;
        }
        let kind = self.nodes[container].kind;
        let last_line_blank = self.blank
            && !(kind == Kind::BlockQuote
                || (kind == Kind::CodeBlock && self.nodes[container].fenced)
                || (kind == Kind::Item
                    && self.nodes[container].children.is_empty()
                    && self.nodes[container].start_line == self.line_number));
        let mut cursor = Some(container);
        while let Some(id) = cursor {
            self.nodes[id].last_line_blank = last_line_blank;
            cursor = self.nodes[id].parent;
        }
        if kind == Kind::Table {
            if self.offset < self.line.len() && !self.blank {
                let row = self.line[self.offset..].to_string();
                self.nodes[container].rows.push(row);
            }
        } else if accepts_lines(kind) {
            self.add_line();
            if kind == Kind::HtmlBlock {
                let html_type = self.nodes[container].html_type;
                if (1..=5).contains(&html_type)
                    && scan::html_block_end(
                        html_type,
                        &self.line[self.offset.min(self.line.len())..],
                    )
                {
                    self.finalize(container);
                }
            }
        } else if self.offset < self.line.len() && !self.blank {
            self.add_child(Kind::Paragraph);
            self.advance_next_nonspace();
            self.add_line();
        }
    }

    /// Tries each kind of block start; 0 none, 1 a container started, 2 a leaf started.
    fn try_block_starts(&mut self, container: usize) -> u8 {
        let first = self.peek(self.next_nonspace);
        let rest = self.line[self.next_nonspace.min(self.line.len())..].to_string();
        let room = self.depth(container) < MAX_DEPTH;

        // Footnote definition: `[^label]:`.
        if self.options.footnotes
            && room
            && !self.indented
            && first == Some(b'[')
            && let Some((label, used)) = footnote_definition_start(&rest)
        {
            self.advance_next_nonspace();
            self.advance_offset(used, false);
            self.close_unmatched();
            let id = self.add_child(Kind::FootnoteDefinition);
            self.footnotes
                .entry(scan::normalize_label(&format!("[{label}]")))
                .or_insert(id);
            self.nodes[id].label = label;
            // What follows the marker on its line starts the first block, whatever its indent.
            self.find_next_nonspace();
            self.advance_next_nonspace();
            return 1;
        }

        // Math block: a line that is only `$$`.
        if self.options.math
            && !self.indented
            && first == Some(b'$')
            && rest.trim_end_matches([' ', '\t']) == "$$"
        {
            self.close_unmatched();
            let id = self.add_child(Kind::MathBlock);
            self.nodes[id].fence_offset = self.indent;
            self.advance_next_nonspace();
            self.advance_offset(2, false);
            return 2;
        }

        // Block quote.
        if room && !self.indented && first == Some(b'>') {
            self.advance_next_nonspace();
            self.advance_offset(1, false);
            if is_space_or_tab(self.peek(self.offset)) {
                self.advance_offset(1, true);
            }
            self.close_unmatched();
            self.add_child(Kind::BlockQuote);
            return 1;
        }

        // ATX heading.
        if !self.indented && first == Some(b'#') {
            let hashes = rest.bytes().take_while(|&c| c == b'#').count();
            let after = rest.as_bytes().get(hashes).copied();
            if hashes <= 6 && matches!(after, None | Some(b' ' | b'\t')) {
                self.advance_next_nonspace();
                let gap = rest[hashes..]
                    .bytes()
                    .take_while(|&c| c == b' ' || c == b'\t')
                    .count();
                self.advance_offset(hashes + gap, false);
                self.close_unmatched();
                let id = self.add_child(Kind::Heading);
                self.nodes[id].level = hashes;
                let text = self.line[self.offset.min(self.line.len())..].to_string();
                self.nodes[id].content = strip_closing_hashes(&text);
                self.advance_offset(self.line.len() - self.offset, false);
                return 2;
            }
        }

        // Fenced code block.
        if !self.indented && matches!(first, Some(b'`' | b'~')) {
            let fence = first.unwrap_or(b'`');
            let run = rest.bytes().take_while(|&c| c == fence).count();
            if run >= 3 && !(fence == b'`' && rest[run..].contains('`')) {
                self.close_unmatched();
                let id = self.add_child(Kind::CodeBlock);
                let indent = self.indent;
                let node = &mut self.nodes[id];
                node.fenced = true;
                node.fence_char = fence;
                node.fence_length = run;
                node.fence_offset = indent;
                self.advance_next_nonspace();
                self.advance_offset(run, false);
                return 2;
            }
        }

        // HTML block.
        if !self.indented && first == Some(b'<') {
            let tip_paragraph =
                !self.all_closed && !self.blank && self.nodes[self.tip].kind == Kind::Paragraph;
            for kind in 1..=7u8 {
                if scan::html_block_start(kind, &rest)
                    && (kind < 7
                        || (self.nodes[container].kind != Kind::Paragraph && !tip_paragraph))
                {
                    self.close_unmatched();
                    let id = self.add_child(Kind::HtmlBlock);
                    self.nodes[id].html_type = kind;
                    return 2;
                }
            }
        }

        // Table: a delimiter row under a paragraph line with as many cells.
        if self.options.tables
            && !self.indented
            && self.nodes[container].kind == Kind::Paragraph
            && let Some(aligns) = delimiter_row(&rest)
        {
            let content = self.nodes[container].content.clone();
            let mut lines: Vec<&str> = content.lines().collect();
            if let Some(header) = lines.pop()
                && split_row(header).len() == aligns.len()
            {
                let header = header.to_string();
                let before = if lines.is_empty() {
                    String::new()
                } else {
                    format!("{}\n", lines.join("\n"))
                };
                self.close_unmatched();
                self.nodes[container].content = before;
                self.finalize(container);
                let id = self.add_child(Kind::Table);
                self.nodes[id].aligns = aligns;
                self.nodes[id].rows.push(header);
                self.advance_offset(self.line.len() - self.offset, false);
                return 2;
            }
        }

        // Setext heading.
        if !self.indented && self.nodes[container].kind == Kind::Paragraph {
            let marker = rest.bytes().next();
            if matches!(marker, Some(b'=' | b'-')) {
                let run = rest.bytes().take_while(|&c| Some(c) == marker).count();
                if rest[run..].bytes().all(|c| c == b' ' || c == b'\t') {
                    self.close_unmatched();
                    self.strip_references(container);
                    if !self.nodes[container].content.is_empty() {
                        let id = self.nodes.len();
                        let mut heading = Node::new(Kind::Heading, self.line_number);
                        heading.level = if marker == Some(b'=') { 1 } else { 2 };
                        heading.content = std::mem::take(&mut self.nodes[container].content);
                        heading.parent = self.nodes[container].parent;
                        heading.open = true;
                        self.nodes.push(heading);
                        if let Some(parent) = self.nodes[container].parent {
                            for child in &mut self.nodes[parent].children {
                                if *child == container {
                                    *child = id;
                                }
                            }
                        }
                        self.nodes[container].open = false;
                        self.tip = id;
                        self.advance_offset(self.line.len() - self.offset, false);
                        return 2;
                    }
                }
            }
        }

        // Thematic break.
        if !self.indented && matches!(first, Some(b'*' | b'_' | b'-')) {
            let marker = first.unwrap_or(b'-');
            let marks = rest.bytes().filter(|&c| c == marker).count();
            if marks >= 3 && rest.bytes().all(|c| c == marker || c == b' ' || c == b'\t') {
                self.close_unmatched();
                self.add_child(Kind::ThematicBreak);
                self.advance_offset(self.line.len() - self.offset, false);
                return 2;
            }
        }

        // List item.
        if room
            && (!self.indented || self.nodes[container].kind == Kind::List)
            && let Some(data) = self.parse_list_marker(container, &rest)
        {
            self.close_unmatched();
            let tip = self.tip;
            let reuse =
                self.nodes[tip].kind == Kind::List && lists_match(&self.nodes[tip].list, &data);
            if !reuse {
                let id = self.add_child(Kind::List);
                self.nodes[id].list = data.clone();
            }
            let item = self.add_child(Kind::Item);
            self.nodes[item].list = data;
            return 1;
        }

        // Indented code block (a table's rows may be indented).
        if self.indented
            && !matches!(self.nodes[self.tip].kind, Kind::Paragraph | Kind::Table)
            && !self.blank
        {
            self.advance_offset(4, true);
            self.close_unmatched();
            self.add_child(Kind::CodeBlock);
            return 2;
        }
        0
    }

    /// How many containers `id` is inside of.
    fn depth(&self, mut id: usize) -> usize {
        let mut depth = 0;
        while let Some(parent) = self.nodes[id].parent {
            depth += 1;
            id = parent;
        }
        depth
    }

    fn parse_list_marker(&mut self, container: usize, rest: &str) -> Option<ListData> {
        if self.indent >= 4 {
            return None;
        }
        let mut data = ListData {
            tight: true,
            marker_offset: self.indent,
            ..ListData::default()
        };

        let b = rest.as_bytes();
        let marker_len = if matches!(b.first(), Some(b'*' | b'+' | b'-')) {
            data.bullet = b[0];
            1
        } else {
            let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
            if !(1..=9).contains(&digits) || !matches!(b.get(digits), Some(b'.' | b')')) {
                return None;
            }
            let number: usize = rest[..digits].parse().ok()?;
            if self.nodes[container].kind == Kind::Paragraph && number != 1 {
                return None;
            }
            data.ordered = true;
            data.start = number;
            data.delimiter = b[digits];
            digits + 1
        };
        let next = b.get(marker_len).copied();
        if !matches!(next, None | Some(b' ' | b'\t')) {
            return None;
        }
        if self.nodes[container].kind == Kind::Paragraph && rest[marker_len..].trim().is_empty() {
            return None;
        }
        self.advance_next_nonspace();
        self.advance_offset(marker_len, true);
        let spaces_start_column = self.column;
        let spaces_start_offset = self.offset;
        loop {
            self.advance_offset(1, true);
            if !(self.column - spaces_start_column < 5 && is_space_or_tab(self.peek(self.offset))) {
                break;
            }
        }
        let blank_item = self.peek(self.offset).is_none();
        let spaces_after = self.column - spaces_start_column;
        if !(1..5).contains(&spaces_after) || blank_item {
            data.padding = marker_len + 1;
            self.column = spaces_start_column;
            self.offset = spaces_start_offset;
            if is_space_or_tab(self.peek(self.offset)) {
                self.advance_offset(1, true);
            }
        } else {
            data.padding = marker_len + spaces_after;
        }
        Some(data)
    }
}

const fn lists_match(a: &ListData, b: &ListData) -> bool {
    a.ordered == b.ordered && a.delimiter == b.delimiter && a.bullet == b.bullet
}

/// Spaces and at most one newline.
fn spnl(s: &str) -> usize {
    let spaces = s.bytes().take_while(|&c| c == b' ').count();
    if s.as_bytes().get(spaces) == Some(&b'\n') {
        let more = s[spaces + 1..].bytes().take_while(|&c| c == b' ').count();
        spaces + 1 + more
    } else {
        spaces
    }
}

/// An ATX heading's text without its optional closing sequence of `#`.
fn strip_closing_hashes(text: &str) -> String {
    let trimmed = text.trim_end_matches([' ', '\t']);
    let hashes = trimmed.bytes().rev().take_while(|&c| c == b'#').count();
    if hashes == 0 {
        return trimmed.trim_start_matches([' ', '\t']).to_string();
    }
    let before = &trimmed[..trimmed.len() - hashes];
    if before.is_empty() {
        return String::new();
    }
    if before.ends_with([' ', '\t']) {
        return before.trim().to_string();
    }
    trimmed.trim_start_matches([' ', '\t']).to_string()
}

/// `[^label]:` at the start of `s`: the label and the bytes the marker takes. A label is not empty and has no
/// whitespace or brackets.
fn footnote_definition_start(s: &str) -> Option<(String, usize)> {
    let inner = s.strip_prefix("[^")?;
    let end = inner.find(']')?;
    let label = &inner[..end];
    if label.is_empty() || label.contains(|c: char| c.is_whitespace() || c == '[') {
        return None;
    }
    inner[end + 1..]
        .starts_with(':')
        .then(|| (label.to_string(), 2 + end + 2))
}

/// The cells of a table row: split at pipes that are not escaped, without a leading and a trailing pipe, each
/// trimmed. `\|` stays as it is here; it becomes `|` before the cell's inlines are read.
pub fn split_row(line: &str) -> Vec<String> {
    let mut row = line.trim();
    if let Some(rest) = row.strip_prefix('|') {
        row = rest;
    }
    if row.ends_with('|') && !row.ends_with("\\|") {
        row = &row[..row.len() - 1];
    }
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = row.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                cell.push(c);
                if let Some(next) = chars.next() {
                    cell.push(next);
                }
            }
            '|' => cells.push(std::mem::take(&mut cell)),
            _ => cell.push(c),
        }
    }
    cells.push(cell);
    cells.iter().map(|c| c.trim().to_string()).collect()
}

/// The alignments of a delimiter row (`| :-- | :-: | --: |`), or `None` when the line is not one. It needs a
/// pipe, so that `---` under a line stays a setext heading.
fn delimiter_row(line: &str) -> Option<Vec<Align>> {
    let line = line.trim_end_matches([' ', '\t']);
    if !line.contains('|') {
        return None;
    }
    split_row(line)
        .iter()
        .map(|cell| {
            let left = cell.starts_with(':');
            let right = cell.ends_with(':') && cell.len() > 1;
            let dashes = cell.trim_start_matches(':').trim_end_matches(':');
            if dashes.is_empty() || !dashes.bytes().all(|c| c == b'-') {
                return None;
            }
            Some(match (left, right) {
                (true, true) => Align::Center,
                (true, false) => Align::Left,
                (false, true) => Align::Right,
                (false, false) => Align::None,
            })
        })
        .collect()
}
