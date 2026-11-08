//! Word wrapping: styled pieces of text into lines no wider than a width, and blocks of lines with the marks
//! (headings, footnotes, formulas) whose positions follow them through indents and prefixes.

use crate::model::{Line, Span, Style};
use crate::width::{clusters, display_width};

/// Inline content ready for wrapping.
#[derive(Debug, Clone)]
pub enum Piece {
    /// Text; breakable at its spaces unless `glue`, in which case it also stays joined to what is before it.
    Text {
        text: String,
        style: Style,
        glue: bool,
    },
    /// A hard line break.
    Break,
    /// A zero-width mark: its position is recorded where the text after it lands.
    Mark(MarkKind),
}

impl Piece {
    pub fn text(text: impl Into<String>, style: Style) -> Self {
        Self::Text {
            text: text.into(),
            style,
            glue: false,
        }
    }

    pub fn glued(text: impl Into<String>, style: Style) -> Self {
        Self::Text {
            text: text.into(),
            style,
            glue: true,
        }
    }
}

/// What a mark stands for: an index into the renderer's lists, or a footnote number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    Heading(usize),
    FootnoteRef(usize),
    FootnoteDef(usize),
    Formula(usize),
}

/// A mark at a position in a block of lines.
#[derive(Debug, Clone, Copy)]
pub struct Mark {
    pub line: usize,
    pub column: usize,
    pub kind: MarkKind,
}

/// Lines and the marks in them.
#[derive(Debug, Clone, Default)]
pub struct Out {
    pub lines: Vec<Line>,
    pub marks: Vec<Mark>,
}

impl Out {
    pub fn line(line: Line) -> Self {
        Self {
            lines: vec![line],
            marks: Vec::new(),
        }
    }

    pub const fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Appends `other` below.
    pub fn append(&mut self, other: Self) {
        let shift = self.lines.len();
        self.marks.extend(other.marks.into_iter().map(|m| Mark {
            line: m.line + shift,
            ..m
        }));
        self.lines.extend(other.lines);
    }

    pub fn blank(&mut self) {
        self.lines.push(Line::default());
    }

    pub fn mark(&mut self, kind: MarkKind) {
        self.marks.push(Mark {
            line: 0,
            column: 0,
            kind,
        });
    }

    /// Puts `first` before the first line and `rest` before the others; both must be as wide. Blank lines
    /// get `rest` without its trailing spaces, so they stay free of trailing whitespace.
    pub fn prefixed(mut self, first: &Line, rest: &Line) -> Self {
        let shift = first.width();
        let mut blank = Line::default();
        for span in &rest.spans {
            blank.push(&span.text, span.style);
        }
        while let Some(last) = blank.spans.last_mut() {
            let trimmed = last.text.trim_end().len();
            if trimmed == 0 {
                blank.spans.pop();
            } else {
                last.text.truncate(trimmed);
                break;
            }
        }
        for (i, line) in self.lines.iter_mut().enumerate() {
            let mut out = if i == 0 {
                first.clone()
            } else if line.is_empty() {
                blank.clone()
            } else {
                rest.clone()
            };
            if !line.is_empty() {
                out.extend(line);
            }
            *line = out;
        }
        for mark in &mut self.marks {
            mark.column += shift;
        }
        self
    }

    /// Applies `change` to every span's style.
    pub fn restyle(&mut self, change: impl Fn(&mut Style)) {
        for line in &mut self.lines {
            for span in &mut line.spans {
                change(&mut span.style);
            }
        }
    }
}

/// A line of `count` copies of `text` in `style`.
pub fn repeat(text: &str, count: usize, style: Style) -> Line {
    let mut line = Line::default();
    line.push(&text.repeat(count), style);
    line
}

pub fn spaces(count: usize) -> Line {
    repeat(" ", count, Style::default())
}

#[derive(Debug, Clone)]
enum Frag {
    Text(String, Style),
    Mark(MarkKind),
}

#[derive(Debug, Clone)]
enum Token {
    Word(Vec<Frag>),
    Space(Style),
    Newline,
}

/// The cells of a word, measured as one text: an emoji sequence split between two styles still counts once.
fn frag_width(frags: &[Frag]) -> usize {
    let text: String = frags
        .iter()
        .filter_map(|f| match f {
            Frag::Text(t, _) => Some(t.as_str()),
            Frag::Mark(_) => None,
        })
        .collect();
    display_width(&text)
}

fn has_text(frags: &[Frag]) -> bool {
    frags
        .iter()
        .any(|f| matches!(f, Frag::Text(t, _) if !t.is_empty()))
}

fn push_text(word: &mut Vec<Frag>, text: &str, style: Style) {
    if text.is_empty() {
        return;
    }
    if let Some(Frag::Text(last, last_style)) = word.last_mut()
        && *last_style == style
    {
        last.push_str(text);
        return;
    }
    word.push(Frag::Text(text.to_owned(), style));
}

fn tokens(pieces: &[Piece]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut word: Vec<Frag> = Vec::new();
    let space = |tokens: &mut Vec<Token>, word: &mut Vec<Frag>, style: Style| {
        if has_text(word) {
            tokens.push(Token::Word(std::mem::take(word)));
        }
        if matches!(tokens.last(), Some(Token::Word(_))) {
            tokens.push(Token::Space(style));
        }
    };
    for piece in pieces {
        match piece {
            Piece::Text {
                text,
                style,
                glue: true,
            } => push_text(&mut word, text, *style),
            Piece::Text {
                text,
                style,
                glue: false,
            } => {
                let mut rest = text.as_str();
                while let Some(at) = rest.find([' ', '\t', '\n']) {
                    push_text(&mut word, &rest[..at], *style);
                    space(&mut tokens, &mut word, *style);
                    rest = &rest[at + 1..];
                }
                push_text(&mut word, rest, *style);
            }
            Piece::Break => {
                if has_text(&word) {
                    tokens.push(Token::Word(std::mem::take(&mut word)));
                }
                if matches!(tokens.last(), Some(Token::Space(_))) {
                    tokens.pop();
                }
                tokens.push(Token::Newline);
            }
            Piece::Mark(kind) => word.push(Frag::Mark(*kind)),
        }
    }
    if !word.is_empty() {
        tokens.push(Token::Word(word));
    }
    if matches!(tokens.last(), Some(Token::Space(_))) {
        tokens.pop();
    }
    tokens
}

/// The width of `pieces` on one line (the widest line, with hard breaks), and of their longest word.
pub fn measure(pieces: &[Piece]) -> (usize, usize) {
    let (mut natural, mut longest, mut line) = (0, 0, 0);
    for token in tokens(pieces) {
        match token {
            Token::Word(frags) => {
                let width = frag_width(&frags);
                longest = longest.max(width);
                line += width;
            }
            Token::Space(_) => line += 1,
            Token::Newline => line = 0,
        }
        natural = natural.max(line);
    }
    (natural, longest)
}

struct Wrapper {
    width: usize,
    out: Out,
    line: Line,
    used: usize,
}

impl Wrapper {
    fn newline(&mut self) {
        self.out.lines.push(std::mem::take(&mut self.line));
        self.used = 0;
    }

    fn place(&mut self, frags: Vec<Frag>, breakable: bool) {
        // The last cluster placed, so that one continued in the next fragment (an emoji in one style, its
        // U+FE0F in another) is measured as one.
        let mut tail = String::new();
        for frag in frags {
            match frag {
                Frag::Mark(kind) => self.out.marks.push(Mark {
                    line: self.out.lines.len(),
                    column: self.used,
                    kind,
                }),
                Frag::Text(text, style) if breakable => {
                    for (cluster, cells) in clusters(&text) {
                        let joined = format!("{tail}{cluster}");
                        let (mut tail_next, mut cells) = if clusters(&joined).nth(1).is_none() {
                            let grown = display_width(&joined).saturating_sub(display_width(&tail));
                            (joined, grown)
                        } else {
                            (cluster.to_owned(), cells)
                        };
                        if self.used > 0 && self.used + cells > self.width {
                            self.newline();
                            cluster.clone_into(&mut tail_next);
                            cells = display_width(cluster);
                        }
                        self.line.push(cluster, style);
                        self.used += cells;
                        tail = tail_next;
                    }
                }
                Frag::Text(text, style) => {
                    self.used += display_width(&text);
                    self.line.push(&text, style);
                }
            }
        }
    }
}

/// Wraps `pieces` to `width` cells: breaks at spaces, and inside a word only when it is wider than a line.
pub fn wrap(pieces: &[Piece], width: usize) -> Out {
    let mut w = Wrapper {
        width: width.max(1),
        out: Out::default(),
        line: Line::default(),
        used: 0,
    };
    let mut space: Option<Style> = None;
    for token in tokens(pieces) {
        match token {
            Token::Word(frags) => {
                let cells = frag_width(&frags);
                let gap = space.take();
                if w.used > 0 && w.used + 1 + cells <= w.width {
                    w.line.push(" ", gap.unwrap_or_default());
                    w.used += 1;
                    let start = w.used;
                    w.place(frags, false);
                    w.used = start + cells;
                } else if cells <= w.width {
                    if w.used > 0 {
                        w.newline();
                    }
                    w.place(frags, false);
                    w.used = cells;
                } else {
                    if w.used > 0 {
                        w.newline();
                    }
                    w.place(frags, true);
                }
            }
            Token::Space(style) => space = Some(style),
            Token::Newline => {
                space = None;
                w.newline();
            }
        }
    }
    if !w.line.is_empty() || w.out.lines.is_empty() && !w.out.marks.is_empty() {
        w.newline();
    }
    while w.out.lines.last().is_some_and(Line::is_empty) && w.out.lines.len() > 1 {
        w.out.lines.pop();
    }
    let lines = w.out.lines.len();
    for mark in &mut w.out.marks {
        mark.line = mark.line.min(lines.saturating_sub(1));
    }
    if w.out.lines.iter().all(Line::is_empty) {
        return Out::default();
    }
    w.out
}

/// Splits `text` into pieces of at most `first` cells, then at most `rest` cells, by characters.
pub fn split_cells(text: &str, first: usize, rest: usize) -> Vec<String> {
    let mut chunks = vec![String::new()];
    let (mut used, mut limit) = (0, first.max(1));
    for (cluster, cells) in clusters(text) {
        if used > 0 && used + cells > limit {
            chunks.push(String::new());
            used = 0;
            limit = rest.max(1);
        }
        if let Some(chunk) = chunks.last_mut() {
            chunk.push_str(cluster);
        }
        used += cells;
    }
    chunks
}

/// `text` as lines of at most `width` cells, broken by characters.
pub fn hard_lines(text: &str, width: usize, style: Style) -> Out {
    let mut out = Out::default();
    for chunk in split_cells(text, width, width) {
        out.lines.push(Line {
            spans: vec![Span { text: chunk, style }],
        });
    }
    out
}
