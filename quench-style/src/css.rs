use std::fmt::Write as _;
#[derive(Debug)]
pub struct CssRule {
    selector: String,
    properties: Vec<(String, String)>,
    children: Vec<Self>,
}

impl CssRule {
    #[must_use]
    pub fn new(selector: &str) -> Self {
        Self {
            selector: selector.to_string(),
            properties: Vec::new(),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn property(mut self, name: &str, value: &str) -> Self {
        self.properties.push((name.to_string(), value.to_string()));
        self
    }

    #[must_use]
    pub fn child(mut self, rule: Self) -> Self {
        self.children.push(rule);
        self
    }

    #[must_use]
    pub fn render(&self) -> String {
        self.render_internal(0)
    }

    fn render_internal(&self, indent: usize) -> String {
        let indent_str = "    ".repeat(indent);
        let inner_indent = "    ".repeat(indent + 1);

        let mut css = String::new();

        let _ = writeln!(css, "{}{} {{", indent_str, self.selector);

        for (name, value) in &self.properties {
            let _ = writeln!(css, "{inner_indent}{name}: {value};");
        }

        for child in &self.children {
            css.push_str(&child.render_internal(indent + 1));
        }

        let _ = writeln!(css, "{indent_str}}}");
        css
    }
}
