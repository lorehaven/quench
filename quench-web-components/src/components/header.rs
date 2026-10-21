use quench_web::{Element, div, h2, header, nav_button};

#[derive(Clone, Debug, Default)]
pub struct HeaderBuilder {
    label: String,
    with_nav: bool,
    nav_panel: Option<Element>,
}

impl HeaderBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn label(mut self, value: impl Into<String>) -> Self {
        self.label = value.into();
        self
    }

    #[must_use]
    pub fn with_nav(mut self, value: Element) -> Self {
        self.with_nav = true;
        self.nav_panel = Some(value);
        self
    }

    pub fn build(self) -> Element {
        header().class("q-shell-header").child(
            div()
                .class("left-panel")
                .class("q-shell-header-left")
                .child_opt(self.with_nav.then(nav_button))
                .child(h2().attr("data-i18n", &self.label))
                .child_opt(self.nav_panel),
        )
    }
}
