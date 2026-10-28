//! Inline layout: text and inline boxes broken into lines.

use std::rc::Rc;

use quench_css::{ComputedStyle, Display, TextAlign, TextTransform};

use crate::engine::{Constraint, Edges, Engine, Width, is_hidden, is_out_of_flow};
use crate::node::{Content, Fragment, InlineFragment, LayoutBox, Node, Pending, TextFragment};

/// The lines of an inline formatting context, from the top left of the container's content box.
#[derive(Debug, Default)]
pub struct Lines {
    pub fragments: Vec<Fragment>,
    pub atoms: Vec<LayoutBox>,
    pub pending: Vec<Pending>,
    pub height: f32,
    pub baseline: Option<f32>,
}

enum Tok {
    Word {
        text: String,
        width: f32,
        style: Rc<ComputedStyle>,
        key: usize,
    },
    Space {
        width: f32,
        style: Rc<ComputedStyle>,
        key: usize,
    },
    Open {
        key: usize,
        style: Rc<ComputedStyle>,
        left: f32,
        right: f32,
    },
    Close {
        right: f32,
    },
    Atom(Box<LayoutBox>),
    Break,
    OutOfFlow(Rc<Node>),
}

enum Placed {
    Text {
        key: usize,
        text: String,
        style: Rc<ComputedStyle>,
        width: f32,
        trailing_space: f32,
    },
    Atom(Box<LayoutBox>),
}

struct OpenEl {
    key: usize,
    style: Rc<ComputedStyle>,
    start: f32,
    right: f32,
    first: bool,
}

struct Mark {
    key: usize,
    style: Rc<ComputedStyle>,
    x0: f32,
    x1: f32,
    first: bool,
    last: bool,
}

/// U+2028, which the host puts in a text node for `<br>`.
pub const LINE_BREAK: char = '\u{2028}';

fn transform(text: &str, how: TextTransform) -> String {
    match how {
        TextTransform::None => text.to_string(),
        TextTransform::Uppercase => text.to_uppercase(),
        TextTransform::Lowercase => text.to_lowercase(),
        TextTransform::Capitalize => {
            let mut start = true;
            text.chars()
                .flat_map(|c| {
                    let out: Vec<char> = if start && c.is_alphabetic() {
                        c.to_uppercase().collect()
                    } else {
                        vec![c]
                    };
                    start = c.is_whitespace();
                    out
                })
                .collect()
        }
    }
}

impl Engine<'_> {
    fn tokenize(&self, items: &[Rc<Node>], cw: f32, out: &mut Vec<Tok>, last_space: &mut bool) {
        for node in items {
            if is_hidden(node) {
                continue;
            }
            if is_out_of_flow(node) {
                out.push(Tok::OutOfFlow(Rc::clone(node)));
                continue;
            }
            match &node.content {
                Content::Text(text) => {
                    let text = transform(text, node.style.text_transform);
                    let mut word = String::new();
                    let flush = |word: &mut String, out: &mut Vec<Tok>, last_space: &mut bool| {
                        if !word.is_empty() {
                            let width = self.fonts.measure(&node.style, word);
                            out.push(Tok::Word {
                                text: std::mem::take(word),
                                width,
                                style: Rc::clone(&node.style),
                                key: node.key,
                            });
                            *last_space = false;
                        }
                    };
                    for c in text.chars() {
                        if c == LINE_BREAK {
                            flush(&mut word, out, last_space);
                            out.push(Tok::Break);
                            *last_space = true;
                        } else if c.is_whitespace() {
                            flush(&mut word, out, last_space);
                            if !*last_space {
                                let width = self.fonts.space(&node.style);
                                out.push(Tok::Space {
                                    width,
                                    style: Rc::clone(&node.style),
                                    key: node.key,
                                });
                                *last_space = true;
                            }
                        } else {
                            word.push(c);
                        }
                    }
                    flush(&mut word, out, last_space);
                }
                Content::Children(children) if node.style.display == Display::Inline => {
                    let e = Edges::of(&node.style, cw);
                    let left = e.margin[3].unwrap_or(0.0) + e.padding[3] + e.border[3];
                    let right = e.margin[1].unwrap_or(0.0) + e.padding[1] + e.border[1];
                    out.push(Tok::Open {
                        key: node.key,
                        style: Rc::clone(&node.style),
                        left,
                        right,
                    });
                    self.tokenize(children, cw, out, last_space);
                    out.push(Tok::Close { right });
                }
                _ => {
                    let c = Constraint {
                        avail_w: cw,
                        avail_h: None,
                        width: Width::Shrink,
                        height: None,
                        collapse: false,
                    };
                    out.push(Tok::Atom(Box::new(self.layout_node(node, &c))));
                    *last_space = false;
                }
            }
        }
    }

    /// The narrowest and widest the inline content of `items` can be.
    pub(crate) fn inline_intrinsic(&self, items: &[Rc<Node>]) -> (f32, f32) {
        let mut toks = Vec::new();
        self.tokenize(items, f32::INFINITY, &mut toks, &mut true);
        let (mut min, mut max, mut line) = (0.0_f32, 0.0_f32, 0.0_f32);
        for tok in &toks {
            match tok {
                Tok::Word { width, .. } => {
                    min = min.max(*width);
                    line += width;
                }
                Tok::Space { width, .. } => {
                    if line > 0.0 {
                        line += width;
                    }
                }
                Tok::Open { left, .. } => line += left,
                Tok::Close { right } => line += right,
                Tok::Atom(b) => {
                    let w = b.width + b.margin[1] + b.margin[3];
                    min = min.max(w);
                    line += w;
                }
                Tok::Break => {
                    max = max.max(line);
                    line = 0.0;
                }
                Tok::OutOfFlow(_) => {}
            }
        }
        (min, max.max(line))
    }

    /// Breaks `items` into lines of at most `cw` pixels (they may be longer when a word or box does not fit).
    pub(crate) fn inline_flow(&self, block: &ComputedStyle, items: &[Rc<Node>], cw: f32) -> Lines {
        let mut toks = Vec::new();
        self.tokenize(items, cw, &mut toks, &mut true);
        let mut run = Run::new(self, block, cw);
        for tok in toks {
            match tok {
                Tok::Open {
                    key,
                    style,
                    left,
                    right,
                } => {
                    run.x += left;
                    run.open.push(OpenEl {
                        key,
                        style,
                        start: run.x - left,
                        right,
                        first: true,
                    });
                }
                Tok::Close { right } => {
                    run.x += right;
                    if let Some(el) = run.open.pop() {
                        run.marks.push(Mark {
                            key: el.key,
                            style: el.style,
                            x0: el.start,
                            x1: run.x - el.right,
                            first: el.first,
                            last: true,
                        });
                    }
                }
                Tok::Space { width, style, key } => {
                    if run.has_content {
                        run.push_text(key, " ", &style, width, width);
                    }
                }
                Tok::Word {
                    text,
                    width,
                    style,
                    key,
                } => {
                    if run.has_content && run.x + width > cw + 0.01 {
                        run.finish_line();
                    }
                    run.push_text(key, &text, &style, width, 0.0);
                    run.has_content = true;
                }
                Tok::Atom(b) => {
                    let w = b.width + b.margin[1] + b.margin[3];
                    if run.has_content && run.x + w > cw + 0.01 {
                        run.finish_line();
                    }
                    run.x += w;
                    run.placed.push((run.x - w, Placed::Atom(b)));
                    run.has_content = true;
                }
                Tok::Break => run.finish_line(),
                Tok::OutOfFlow(node) => run.out.pending.push(Pending {
                    node,
                    x: run.x,
                    y: run.y,
                }),
            }
        }
        if run.has_content || !run.marks.is_empty() || !run.open.is_empty() {
            run.finish_line();
        }
        run.out.height = run.y;
        run.out
    }
}

/// Lines being built.
struct Run<'a, 'b> {
    engine: &'a Engine<'b>,
    block: &'a ComputedStyle,
    cw: f32,
    x: f32,
    y: f32,
    has_content: bool,
    placed: Vec<(f32, Placed)>,
    open: Vec<OpenEl>,
    marks: Vec<Mark>,
    out: Lines,
}

impl<'a, 'b> Run<'a, 'b> {
    fn new(engine: &'a Engine<'b>, block: &'a ComputedStyle, cw: f32) -> Self {
        Self {
            engine,
            block,
            cw,
            x: 0.0,
            y: 0.0,
            has_content: false,
            placed: Vec::new(),
            open: Vec::new(),
            marks: Vec::new(),
            out: Lines::default(),
        }
    }

    fn push_text(
        &mut self,
        key: usize,
        text: &str,
        style: &Rc<ComputedStyle>,
        width: f32,
        space: f32,
    ) {
        if let Some((
            _,
            Placed::Text {
                key: k,
                text: t,
                style: s,
                width: w,
                trailing_space,
            },
        )) = self.placed.last_mut()
            && *k == key
            && Rc::ptr_eq(s, style)
        {
            t.push_str(text);
            *w += width;
            *trailing_space = space;
        } else {
            self.placed.push((
                self.x,
                Placed::Text {
                    key,
                    text: text.to_string(),
                    style: Rc::clone(style),
                    width,
                    trailing_space: space,
                },
            ));
        }
        self.x += width;
    }

    fn finish_line(&mut self) {
        // A space at the end of a line takes no room.
        if let Some((
            _,
            Placed::Text {
                text,
                width,
                trailing_space,
                ..
            },
        )) = self.placed.last_mut()
            && *trailing_space > 0.0
        {
            text.pop();
            *width -= *trailing_space;
            self.x -= *trailing_space;
            *trailing_space = 0.0;
        }
        let fonts = self.engine.fonts;
        let strut = fonts.metrics(self.block);
        let strut_lh = fonts.line_height(self.block);
        let half = (strut_lh - (strut.ascent + strut.descent)) / 2.0;
        let (mut above, mut below) = (strut.ascent + half, strut.descent + half);
        for (_, p) in &self.placed {
            match p {
                Placed::Text { style, .. } => {
                    let m = fonts.metrics(style);
                    let half = (fonts.line_height(style) - (m.ascent + m.descent)) / 2.0;
                    above = above.max(m.ascent + half);
                    below = below.max(m.descent + half);
                }
                Placed::Atom(b) => {
                    above = above.max(b.margin[0] + b.baseline);
                    below = below.max(b.height - b.baseline + b.margin[2]);
                }
            }
        }
        let baseline = self.y + above;
        let width = self.x;
        let shift = if self.cw.is_finite() {
            match self.block.text_align {
                TextAlign::Right | TextAlign::End => (self.cw - width).max(0.0),
                TextAlign::Center => ((self.cw - width) / 2.0).max(0.0),
                TextAlign::Start | TextAlign::Left => 0.0,
            }
        } else {
            0.0
        };
        // Elements still open continue on the next line.
        for el in &mut self.open {
            self.marks.push(Mark {
                key: el.key,
                style: Rc::clone(&el.style),
                x0: el.start,
                x1: width,
                first: el.first,
                last: false,
            });
            el.start = 0.0;
            el.first = false;
        }
        for mark in std::mem::take(&mut self.marks) {
            let m = fonts.metrics(&mark.style);
            let s = &mark.style;
            let top = baseline - m.ascent - s.padding[0].as_px().unwrap_or(0.0) - s.border_width[0];
            let height = m.ascent
                + m.descent
                + s.padding[0].as_px().unwrap_or(0.0)
                + s.padding[2].as_px().unwrap_or(0.0)
                + s.border_width[0]
                + s.border_width[2];
            self.out.fragments.push(Fragment::Inline(InlineFragment {
                key: mark.key,
                style: mark.style,
                x: mark.x0 + shift,
                y: top,
                width: (mark.x1 - mark.x0).max(0.0),
                height,
                first: mark.first,
                last: mark.last,
            }));
        }
        for (x, p) in std::mem::take(&mut self.placed) {
            match p {
                Placed::Text {
                    key,
                    text,
                    style,
                    width,
                    ..
                } => {
                    self.out.fragments.push(Fragment::Text(TextFragment {
                        key,
                        text,
                        x: x + shift,
                        baseline,
                        width,
                        style,
                    }));
                }
                Placed::Atom(mut b) => {
                    b.x = x + shift + b.margin[3];
                    b.y = baseline - b.baseline;
                    self.out.atoms.push(*b);
                }
            }
        }
        if self.out.baseline.is_none() {
            self.out.baseline = Some(baseline);
        }
        self.y += above + below;
        self.x = 0.0;
        self.has_content = false;
    }
}
