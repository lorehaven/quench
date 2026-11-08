//! Heading anchors as GitHub and kramdown make them, so a pager can follow `#anchor` links.

use std::collections::{HashMap, HashSet};

/// The anchor of a heading reading `text`: lowercase, spaces to `-`, punctuation dropped (letters, digits,
/// `-` and `_` are kept).
///
/// ```
/// use quench_markdown_term::slug;
///
/// assert_eq!(slug("What is missing?"), "what-is-missing");
/// assert_eq!(slug("`Options::ALL` & more"), "optionsall--more");
/// ```
#[must_use]
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.trim().chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            out.extend(c.to_lowercase());
        } else if c == ' ' {
            out.push('-');
        }
    }
    out
}

/// Slugs for the headings of one page: the second heading with the same text gets `-1`, the third `-2`...
#[derive(Debug, Clone, Default)]
pub struct Slugger {
    used: HashSet<String>,
    counts: HashMap<String, usize>,
}

impl Slugger {
    /// An empty slugger, for a new page.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The unique anchor of the next heading reading `text`.
    pub fn slug(&mut self, text: &str) -> String {
        self.unique(&slug(text))
    }

    /// `anchor` made unique on the page (a heading's own `{#id}`, or a slug).
    pub fn unique(&mut self, anchor: &str) -> String {
        let mut candidate = anchor.to_owned();
        if self.used.contains(&candidate) {
            let count = self.counts.entry(anchor.to_owned()).or_insert(0);
            loop {
                *count += 1;
                candidate = format!("{anchor}-{count}");
                if !self.used.contains(&candidate) {
                    break;
                }
            }
        }
        self.used.insert(candidate.clone());
        candidate
    }
}
