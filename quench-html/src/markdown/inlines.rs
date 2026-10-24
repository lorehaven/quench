//! Inline content: emphasis, code spans, links, images, autolinks, raw HTML and breaks.

use super::blocks::Reference;
use super::scan;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Inline {
    Text(String),
    Code(String),
    Emph(Vec<Self>),
    Strong(Vec<Self>),
    Link {
        destination: String,
        title: String,
        children: Vec<Self>,
    },
    Image {
        destination: String,
        title: String,
        children: Vec<Self>,
    },
    Html(String),
    SoftBreak,
    HardBreak,
}

/// An item while parsing: finished content, or a delimiter run / bracket still to be resolved.
#[derive(Debug, Clone)]
enum Item {
    Node(Inline),
    Delim {
        ch: u8,
        count: usize,
        original: usize,
        can_open: bool,
        can_close: bool,
    },
    Bracket {
        image: bool,
        active: bool,
        /// Byte offset of the `[` in the subject.
        index: usize,
        bracket_after: bool,
    },
}

struct Parser<'a> {
    subject: &'a str,
    pos: usize,
    items: Vec<Item>,
    references: &'a HashMap<String, Reference>,
}

const fn is_unicode_whitespace(c: char) -> bool {
    c.is_whitespace()
}

fn is_punctuation(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_punctuation()
    } else {
        // Non-ASCII punctuation and symbols: anything printable that is not a letter, digit or space.
        !c.is_alphanumeric() && !c.is_whitespace() && !c.is_control() && !is_combining(c)
    }
}

const fn is_combining(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

/// Parses the inline content of a paragraph or heading.
pub fn parse(content: &str, references: &HashMap<String, Reference>) -> Vec<Inline> {
    let subject = content.trim_matches([' ', '\t', '\n']);
    let mut parser = Parser {
        subject,
        pos: 0,
        items: Vec::new(),
        references,
    };
    while parser.pos < subject.len() {
        parser.parse_inline();
    }
    let items = std::mem::take(&mut parser.items);
    resolve(items)
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.subject.as_bytes().get(self.pos).copied()
    }

    fn rest(&self) -> &str {
        &self.subject[self.pos..]
    }

    fn push_text(&mut self, text: &str) {
        if let Some(Item::Node(Inline::Text(last))) = self.items.last_mut() {
            last.push_str(text);
        } else {
            self.items.push(Item::Node(Inline::Text(text.to_string())));
        }
    }

    fn parse_inline(&mut self) {
        let c = self.subject.as_bytes()[self.pos];
        match c {
            b'\n' => self.newline(),
            b'\\' => self.backslash(),
            b'`' => self.backticks(),
            b'*' | b'_' => self.delimiters(c),
            b'[' => {
                let index = self.pos;
                self.pos += 1;
                self.open_bracket("[", false, index);
            }
            b'!' => {
                if self.subject[self.pos + 1..].starts_with('[') {
                    let index = self.pos + 1;
                    self.pos += 2;
                    self.open_bracket("![", true, index);
                } else {
                    self.pos += 1;
                    self.push_text("!");
                }
            }
            b']' => self.close_bracket(),
            b'<' => self.angle(),
            b'&' => self.entity(),
            _ => {
                // Plain text up to the next special character.
                let end = self
                    .rest()
                    .find(['\n', '\\', '`', '*', '_', '[', ']', '!', '<', '&'])
                    .map_or(self.subject.len(), |i| self.pos + i);
                let text = &self.subject[self.pos..end.max(self.pos + 1)];
                let text = text.to_string();
                self.pos += text.len();
                self.push_text(&text);
            }
        }
    }

    fn newline(&mut self) {
        self.pos += 1;
        // Trailing spaces of the previous text decide between a soft and a hard break.
        let mut hard = false;
        if let Some(Item::Node(Inline::Text(last))) = self.items.last_mut() {
            let trimmed = last.trim_end_matches(' ');
            hard = last.len() - trimmed.len() >= 2;
            let len = trimmed.len();
            last.truncate(len);
        }
        self.items.push(Item::Node(if hard {
            Inline::HardBreak
        } else {
            Inline::SoftBreak
        }));
        // The next line's leading spaces are dropped.
        while self.peek() == Some(b' ') {
            self.pos += 1;
        }
    }

    fn backslash(&mut self) {
        self.pos += 1;
        match self.peek() {
            Some(b'\n') => {
                self.pos += 1;
                self.items.push(Item::Node(Inline::HardBreak));
                while self.peek() == Some(b' ') {
                    self.pos += 1;
                }
            }
            Some(c) if scan::is_escapable(c) => {
                self.pos += 1;
                self.push_text(&(c as char).to_string());
            }
            _ => self.push_text("\\"),
        }
    }

    fn backticks(&mut self) {
        let run = self.rest().bytes().take_while(|&c| c == b'`').count();
        let after = self.pos + run;
        // Find a closing run of exactly the same length.
        let mut search = after;
        loop {
            let Some(found) = self.subject[search..].find('`') else {
                self.pos = after;
                self.push_text(&"`".repeat(run));
                return;
            };
            let start = search + found;
            let len = self.subject[start..]
                .bytes()
                .take_while(|&c| c == b'`')
                .count();
            if len == run {
                let inner = self.subject[after..start].replace('\n', " ");
                let stripped = if inner.len() > 2
                    && inner.starts_with(' ')
                    && inner.ends_with(' ')
                    && !inner.bytes().all(|c| c == b' ')
                {
                    inner[1..inner.len() - 1].to_string()
                } else {
                    inner
                };
                self.items.push(Item::Node(Inline::Code(stripped)));
                self.pos = start + len;
                return;
            }
            search = start + len;
        }
    }

    fn delimiters(&mut self, ch: u8) {
        let start = self.pos;
        let count = self.rest().bytes().take_while(|&c| c == ch).count();
        self.pos += count;
        let before = self.subject[..start].chars().next_back().unwrap_or('\n');
        let after = self.subject[self.pos..].chars().next().unwrap_or('\n');
        let after_ws = is_unicode_whitespace(after);
        let after_punct = is_punctuation(after);
        let before_ws = is_unicode_whitespace(before);
        let before_punct = is_punctuation(before);
        let left = !after_ws && (!after_punct || before_ws || before_punct);
        let right = !before_ws && (!before_punct || after_ws || after_punct);
        let (can_open, can_close) = if ch == b'_' {
            (
                left && (!right || before_punct),
                right && (!left || after_punct),
            )
        } else {
            (left, right)
        };
        self.items.push(Item::Delim {
            ch,
            count,
            original: count,
            can_open,
            can_close,
        });
    }

    fn open_bracket(&mut self, text: &str, image: bool, index: usize) {
        // An earlier bracket now has another after it, which rules out its shortcut form.
        for item in &mut self.items {
            if let Item::Bracket { bracket_after, .. } = item {
                *bracket_after = true;
            }
        }
        let _ = text;
        self.items.push(Item::Bracket {
            image,
            active: true,
            index,
            bracket_after: false,
        });
    }

    fn close_bracket(&mut self) {
        self.pos += 1;
        let start_pos = self.pos;
        let Some(opener_at) = self
            .items
            .iter()
            .rposition(|i| matches!(i, Item::Bracket { .. }))
        else {
            self.push_text("]");
            return;
        };
        let Item::Bracket {
            image,
            active,
            index,
            bracket_after,
        } = self.items[opener_at].clone()
        else {
            return;
        };
        if !active {
            self.items[opener_at] =
                Item::Node(Inline::Text(if image { "![" } else { "[" }.to_string()));
            self.push_text("]");
            return;
        }

        let mut destination = String::new();
        let mut title = String::new();
        let mut matched = false;

        // An inline link: `(destination "title")`.
        if self.peek() == Some(b'(') {
            let save = self.pos;
            self.pos += 1;
            self.skip_spnl();
            if let Some((dest, used)) = scan::link_destination(self.rest()) {
                self.pos += used;
                let before_title = self.pos;
                self.skip_spnl();
                let mut found_title = String::new();
                if self.pos > before_title
                    && let Some((t, used)) = scan::link_title(self.rest())
                {
                    found_title = t;
                    self.pos += used;
                }
                self.skip_spnl();
                if self.peek() == Some(b')') {
                    self.pos += 1;
                    destination = dest;
                    title = found_title;
                    matched = true;
                }
            }
            if !matched {
                self.pos = save;
            }
        }

        // A reference link: `[text][label]`, `[text][]` or `[text]`.
        if !matched {
            let before_label = self.pos;
            let n = scan::link_label(self.rest());
            let label = if n > 2 {
                self.pos += n;
                Some(self.subject[before_label..before_label + n].to_string())
            } else if !bracket_after {
                // Collapsed or shortcut: the text itself is the label (when it has no bracket).
                if n == 2 {
                    self.pos += 2;
                }
                Some(self.subject[index..start_pos].to_string()).filter(|s| s.ends_with(']'))
            } else {
                if n == 2 {
                    self.pos += 2;
                }
                None
            };
            if n == 0 {
                self.pos = before_label;
            }
            if let Some(label) = label
                && label.len() > 2
                && let Some(reference) = self.references.get(&scan::normalize_label(&label))
            {
                destination = reference.destination.clone();
                title = reference.title.clone();
                matched = true;
            }
        }

        if matched {
            let inner: Vec<Item> = self.items.split_off(opener_at + 1);
            self.items.pop();
            let children = resolve(inner);
            let node = if image {
                Inline::Image {
                    destination,
                    title,
                    children,
                }
            } else {
                // Links cannot contain links: earlier `[` openers go inactive.
                for item in &mut self.items {
                    if let Item::Bracket {
                        image: false,
                        active,
                        ..
                    } = item
                    {
                        *active = false;
                    }
                }
                Inline::Link {
                    destination,
                    title,
                    children,
                }
            };
            self.items.push(Item::Node(node));
        } else {
            self.items[opener_at] =
                Item::Node(Inline::Text(if image { "![" } else { "[" }.to_string()));
            self.pos = start_pos;
            self.push_text("]");
        }
    }

    fn skip_spnl(&mut self) {
        while self.peek() == Some(b' ') {
            self.pos += 1;
        }
        if self.peek() == Some(b'\n') {
            self.pos += 1;
            while self.peek() == Some(b' ') {
                self.pos += 1;
            }
        }
    }

    fn angle(&mut self) {
        let rest = self.rest();
        if let Some((len, email)) = scan::autolink(&rest[1..]) {
            let text = rest[1..=len].to_string();
            let destination = if email {
                format!("mailto:{text}")
            } else {
                text.clone()
            };
            self.pos += len + 2;
            self.items.push(Item::Node(Inline::Link {
                destination,
                title: String::new(),
                children: vec![Inline::Text(text)],
            }));
            return;
        }
        let n = scan::inline_html(rest);
        if n > 0 {
            let html = rest[..n].to_string();
            self.pos += n;
            self.items.push(Item::Node(Inline::Html(html)));
            return;
        }
        self.pos += 1;
        self.push_text("<");
    }

    fn entity(&mut self) {
        if let Some((text, used)) = crate::entities::reference(self.rest(), true) {
            self.pos += used;
            self.push_text(&text);
        } else {
            self.pos += 1;
            self.push_text("&");
        }
    }
}

/// Turns items into final inlines: delimiter runs are matched into emphasis, leftovers become text.
fn resolve(mut items: Vec<Item>) -> Vec<Inline> {
    let mut i = 0;
    while i < items.len() {
        let Item::Delim {
            ch,
            count,
            original,
            can_close: true,
            can_open: closer_can_open,
        } = items[i].clone()
        else {
            i += 1;
            continue;
        };
        // Look back for a matching opener.
        let opener = (0..i).rev().find(|&j| {
            matches!(&items[j], Item::Delim { ch: c, can_open: true, original: o, can_close: oc, .. }
                if *c == ch && {
                    let both = closer_can_open || *oc;
                    !both || (o + original) % 3 != 0 || (o % 3 == 0 && original % 3 == 0)
                })
        });
        let Some(j) = opener else {
            if !closer_can_open {
                // This closer can never match: it is plain text.
                items[i] = Item::Node(Inline::Text((ch as char).to_string().repeat(count)));
            }
            i += 1;
            continue;
        };
        let Item::Delim {
            count: open_count, ..
        } = items[j].clone()
        else {
            i += 1;
            continue;
        };
        let used = if count >= 2 && open_count >= 2 { 2 } else { 1 };
        // Everything between the two delimiters becomes the emphasis content.
        let inner: Vec<Item> = items.drain(j + 1..i).collect();
        let children = resolve(inner);
        let node = if used == 2 {
            Inline::Strong(children)
        } else {
            Inline::Emph(children)
        };
        // After draining, the closer is at j + 1.
        let closer_index = j + 1;
        items.insert(closer_index, Item::Node(node));
        let closer_at = closer_index + 1;
        if let Item::Delim { count, .. } = &mut items[closer_at] {
            *count -= used;
        }
        if let Item::Delim { count, .. } = &mut items[j] {
            *count -= used;
        }
        let mut next = closer_at;
        // Remove exhausted delimiters; the node sits between them.
        let closer_empty = matches!(&items[closer_at], Item::Delim { count: 0, .. });
        if closer_empty {
            items.remove(closer_at);
        }
        if matches!(&items[j], Item::Delim { count: 0, .. }) {
            items.remove(j);
            next -= 1;
        }
        i = next;
    }
    items
        .into_iter()
        .map(|item| match item {
            Item::Node(n) => n,
            Item::Delim { ch, count, .. } => Inline::Text((ch as char).to_string().repeat(count)),
            Item::Bracket { image, .. } => Inline::Text(if image { "![" } else { "[" }.to_string()),
        })
        .collect()
}
