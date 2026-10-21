use std::collections::HashMap;
use std::fmt::Write as _;

#[derive(Clone, Debug)]
pub struct Element {
    tag: String,
    attributes: HashMap<String, String>,
    children: Vec<Self>,
    text_content: Option<String>,
    onclick: Option<String>,
    onchange: Option<String>,
    raw: bool,
    defer: bool,
}

// The builder takes `impl ToString` so callers can pass `&str`, `String`, numbers or
// enums alike; it only ever borrows them.
#[allow(clippy::needless_pass_by_value)]
impl Element {
    pub(crate) fn new(tag: impl ToString) -> Self {
        Self {
            tag: tag.to_string(),
            attributes: HashMap::new(),
            children: Vec::new(),
            text_content: None,
            onclick: None,
            onchange: None,
            raw: false,
            defer: false,
        }
    }

    #[must_use]
    pub fn attr(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.attributes.insert(key.to_string(), value.to_string());
        self
    }

    #[must_use]
    pub fn class(mut self, class: impl ToString) -> Self {
        let mut classes = self
            .attributes
            .get("class")
            .unwrap_or(&String::new())
            .clone();
        if !classes.is_empty() {
            classes.push(' ');
        }
        classes.push_str(class.to_string().as_str());
        self.attributes.insert("class".to_string(), classes);
        self
    }

    #[must_use]
    pub fn text(mut self, text: impl ToString) -> Self {
        self.text_content = Some(text.to_string());
        self
    }

    #[must_use]
    pub fn child(mut self, child: Self) -> Self {
        self.children.push(child);
        self
    }

    #[must_use]
    pub fn child_opt(mut self, child: Option<Self>) -> Self {
        if let Some(child) = child {
            self.children.push(child);
        }
        self
    }

    #[must_use]
    pub fn on_click(mut self, js_code: impl ToString) -> Self {
        self.onclick = Some(js_code.to_string());
        self
    }

    #[must_use]
    pub fn on_change(mut self, js_code: impl ToString) -> Self {
        self.onchange = Some(js_code.to_string());
        self
    }

    #[must_use]
    pub const fn raw(mut self) -> Self {
        self.raw = true;
        self
    }

    #[must_use]
    pub const fn defer(mut self) -> Self {
        self.defer = true;
        self
    }

    #[must_use]
    pub fn render(&self) -> String {
        let mut html = format!("<{}", self.tag);

        for (key, value) in &self.attributes {
            let _ = write!(html, " {}=\"{}\"", key, html_escape(value));
        }

        if let Some(onclick) = &self.onclick {
            let _ = write!(html, " onclick=\"{}\"", html_escape(onclick));
        }

        if let Some(onchange) = &self.onchange {
            let _ = write!(html, " onchange=\"{}\"", html_escape(onchange));
        }

        if self.defer {
            html.push_str(" defer");
        }

        html.push('>');
        if let Some(text) = &self.text_content {
            if self.raw {
                // Insert as-is, no escaping
                html.push_str(text);
            } else {
                html.push_str(&html_escape(text));
            }
        }

        for child in &self.children {
            html.push_str(&child.render());
        }

        let _ = write!(html, "</{}>", self.tag);
        html
    }
}

// Helper function to escape HTML
pub(crate) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
