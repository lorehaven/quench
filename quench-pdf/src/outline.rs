//! The outline (bookmarks) of a file and the links on its pages.

#![allow(
    clippy::chunks_exact_to_as_chunks,
    clippy::doc_markdown,
    clippy::manual_let_else
)]

use crate::doc::{Document, Page};
use crate::object::{Dict, Object};

/// One entry of the outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineItem {
    pub title: String,
    /// 0 for the top level.
    pub level: usize,
    /// The page it leads to, from 0, when it leads to one.
    pub page: Option<usize>,
}

/// A link on a page: where it is and where it leads.
#[derive(Debug, Clone, PartialEq)]
pub struct PageLink {
    /// The box, in points from the top left of the page as shown.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// A web or mail address.
    pub uri: Option<String>,
    /// A page of the file, from 0.
    pub page: Option<usize>,
}

/// A text string of a file: UTF-16 with a byte order mark, or Latin-1 (close enough to PDFDocEncoding).
fn text_string(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xfe, 0xff]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from(c[0]) << 8 | u16::from(c[1]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        bytes.iter().map(|b| char::from(*b)).collect()
    }
}

impl Document {
    /// The index of the page whose object is `n`.
    fn page_of_object(pages: &[Page], n: u32) -> Option<usize> {
        pages.iter().position(|p| p.object == Some(n))
    }

    /// A named destination, looked up in the catalog's `Dests` and name tree.
    fn named_destination(&self, name: &[u8]) -> Option<Object> {
        let catalog = self.catalog()?;
        if let Some(Object::Dict(d)) = self.get(&catalog, "Dests")
            && let Some(v) = self.get(&d, &String::from_utf8_lossy(name))
        {
            return Some(v);
        }
        let names = match self.get(&catalog, "Names") {
            Some(Object::Dict(n)) => n,
            _ => return None,
        };
        let Some(Object::Dict(tree)) = self.get(&names, "Dests") else {
            return None;
        };
        self.search_name_tree(&tree, name, 0)
    }

    fn search_name_tree(&self, node: &Dict, name: &[u8], depth: usize) -> Option<Object> {
        if depth > 12 {
            return None;
        }
        if let Some(Object::Array(list)) = self.get(node, "Names") {
            for pair in list.chunks_exact(2) {
                if matches!(self.resolve(&pair[0]), Object::String(s) if s == name) {
                    return Some(self.resolve(&pair[1]));
                }
            }
        }
        if let Some(Object::Array(kids)) = self.get(node, "Kids") {
            for kid in &kids {
                if let Object::Dict(k) = self.resolve(kid)
                    && let Some(found) = self.search_name_tree(&k, name, depth + 1)
                {
                    return Some(found);
                }
            }
        }
        None
    }

    /// The page a destination (an array, a name, or a dictionary holding an array) leads to.
    fn destination_page(&self, dest: &Object, pages: &[Page]) -> Option<usize> {
        match self.resolve(dest) {
            Object::Array(a) => match a.first()? {
                Object::Ref(n, _) => Self::page_of_object(pages, *n),
                // Some files give the page number itself.
                other => other.as_int().and_then(|v| usize::try_from(v).ok()),
            },
            Object::Dict(d) => self.destination_page(d.get("D")?, pages),
            Object::String(s) => {
                let d = self.named_destination(&s)?;
                self.destination_page(&d, pages)
            }
            Object::Name(n) => {
                let d = self.named_destination(n.as_bytes())?;
                self.destination_page(&d, pages)
            }
            _ => None,
        }
    }

    /// Where an action or destination of an outline item or link leads: a page, or an address.
    fn target(&self, dict: &Dict, pages: &[Page]) -> (Option<usize>, Option<String>) {
        if let Some(d) = dict.get("Dest") {
            return (self.destination_page(d, pages), None);
        }
        if let Some(Object::Dict(a)) = self.get(dict, "A") {
            match self.get(&a, "S").as_ref().and_then(Object::as_name) {
                Some("URI") => {
                    let uri = match self.get(&a, "URI") {
                        Some(Object::String(s)) => Some(text_string(&s)),
                        _ => None,
                    };
                    return (None, uri);
                }
                Some("GoTo") => {
                    return (
                        a.get("D").and_then(|d| self.destination_page(d, pages)),
                        None,
                    );
                }
                _ => {}
            }
        }
        (None, None)
    }

    /// The outline of the file, in reading order, with the level of each entry.
    #[must_use]
    pub fn outline(&self) -> Vec<OutlineItem> {
        let pages = self.pages();
        let mut out = Vec::new();
        let Some(catalog) = self.catalog() else {
            return out;
        };
        let Some(Object::Dict(root)) = self.get(&catalog, "Outlines") else {
            return out;
        };
        let mut budget = 5000;
        self.walk_outline(root.get("First"), 0, &pages, &mut out, &mut budget);
        out
    }

    fn walk_outline(
        &self,
        first: Option<&Object>,
        level: usize,
        pages: &[Page],
        out: &mut Vec<OutlineItem>,
        budget: &mut usize,
    ) {
        let mut next = first.cloned();
        while let Some(node) = next {
            if *budget == 0 || level > 16 {
                return;
            }
            *budget -= 1;
            let Object::Dict(item) = self.resolve(&node) else {
                return;
            };
            let title = match self.get(&item, "Title") {
                Some(Object::String(s)) => text_string(&s),
                _ => String::new(),
            };
            let (page, _) = self.target(&item, pages);
            out.push(OutlineItem { title, level, page });
            self.walk_outline(item.get("First"), level + 1, pages, out, budget);
            next = item.get("Next").cloned();
        }
    }

    /// The links of a page (whose index is `index`), with their boxes measured from the top left of the page (`origin` is the
    /// top left of its box in the file's own coordinates).
    #[must_use]
    pub(crate) fn links_of(&self, index: usize, origin: (f64, f64)) -> Vec<PageLink> {
        let pages = self.pages();
        let Some(page) = pages.get(index) else {
            return Vec::new();
        };
        let Some(Object::Array(annots)) = self.get(&page.dict, "Annots") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for a in annots.iter().take(2000) {
            let Object::Dict(d) = self.resolve(a) else {
                continue;
            };
            if self.get(&d, "Subtype").as_ref().and_then(Object::as_name) != Some("Link") {
                continue;
            }
            let Some(Object::Array(r)) = self.get(&d, "Rect") else {
                continue;
            };
            let v: Vec<f64> = r.iter().filter_map(|o| self.resolve(o).as_f64()).collect();
            if v.len() < 4 {
                continue;
            }
            let (x0, x1, y0, y1) = (
                v[0].min(v[2]),
                v[0].max(v[2]),
                v[1].min(v[3]),
                v[1].max(v[3]),
            );
            let (page_index, uri) = self.target(&d, &pages);
            if page_index.is_none() && uri.is_none() {
                continue;
            }
            out.push(PageLink {
                x: x0 - origin.0,
                y: origin.1 - y1,
                width: x1 - x0,
                height: y1 - y0,
                uri,
                page: page_index,
            });
        }
        out
    }
}

/// A markup annotation of a page, in the file's own coordinates (y up).
#[derive(Debug, Clone, PartialEq)]
pub struct Markup {
    /// `Highlight`, `Underline`, `StrikeOut`, `Square` or `Text` (a note).
    pub kind: String,
    /// The boxes it covers: (x0, y0, x1, y1), one for each line of a highlight, else just its `Rect`.
    pub boxes: Vec<(f64, f64, f64, f64)>,
    pub color: (u8, u8, u8),
    /// The text of a note, or the comment on a markup.
    pub contents: String,
}

impl Document {
    /// The markup annotations of page `index` (highlights, underlines, strike-outs, squares and notes).
    #[must_use]
    pub fn markup_of(&self, index: usize) -> Vec<Markup> {
        let pages = self.pages();
        let Some(page) = pages.get(index) else {
            return Vec::new();
        };
        let Some(Object::Array(annots)) = self.get(&page.dict, "Annots") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for a in annots.iter().take(5000) {
            let Object::Dict(d) = self.resolve(a) else {
                continue;
            };
            let kind = match self.get(&d, "Subtype").as_ref().and_then(Object::as_name) {
                Some(k @ ("Highlight" | "Underline" | "StrikeOut" | "Square" | "Text")) => {
                    k.to_string()
                }
                _ => continue,
            };
            let nums = |key: &str| -> Vec<f64> {
                match self.get(&d, key) {
                    Some(Object::Array(v)) => {
                        v.iter().filter_map(|o| self.resolve(o).as_f64()).collect()
                    }
                    _ => Vec::new(),
                }
            };
            let mut boxes: Vec<(f64, f64, f64, f64)> = nums("QuadPoints")
                .chunks_exact(8)
                .map(|q| {
                    let xs = [q[0], q[2], q[4], q[6]];
                    let ys = [q[1], q[3], q[5], q[7]];
                    (
                        xs.iter().copied().fold(f64::MAX, f64::min),
                        ys.iter().copied().fold(f64::MAX, f64::min),
                        xs.iter().copied().fold(f64::MIN, f64::max),
                        ys.iter().copied().fold(f64::MIN, f64::max),
                    )
                })
                .collect();
            if boxes.is_empty() {
                let r = nums("Rect");
                if r.len() >= 4 {
                    boxes.push((
                        r[0].min(r[2]),
                        r[1].min(r[3]),
                        r[0].max(r[2]),
                        r[1].max(r[3]),
                    ));
                }
            }
            let c = nums("C");
            let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            let color = match c.len() {
                1 => (byte(c[0]), byte(c[0]), byte(c[0])),
                3 => (byte(c[0]), byte(c[1]), byte(c[2])),
                4 => {
                    let k = 1.0 - c[3];
                    (
                        byte((1.0 - c[0]) * k),
                        byte((1.0 - c[1]) * k),
                        byte((1.0 - c[2]) * k),
                    )
                }
                _ => (255, 235, 59),
            };
            let contents = match self.get(&d, "Contents") {
                Some(Object::String(s)) => text_string(&s),
                _ => String::new(),
            };
            out.push(Markup {
                kind,
                boxes,
                color,
                contents,
            });
        }
        out
    }
}
